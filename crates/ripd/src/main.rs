use std::{collections::HashMap, convert::Infallible, sync::Arc};

use axum::{
    extract::{Path, State},
    http::{header::CONTENT_TYPE, StatusCode},
    response::{sse::Event as SseEvent, IntoResponse, Sse},
    routing::get,
    Json, Router,
};
use futures_util::StreamExt;
use rip_kernel::Runtime;
use rip_log::{write_snapshot, EventLog};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};
use tokio_stream::wrappers::BroadcastStream;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

#[cfg(not(test))]
use std::net::SocketAddr;
#[cfg(not(test))]
use tokio::net::TcpListener;

#[derive(Clone)]
struct AppState {
    sessions: Arc<Mutex<HashMap<String, SessionHandle>>>,
    event_log: Arc<EventLog>,
    snapshot_dir: Arc<std::path::PathBuf>,
    runtime: Arc<Runtime>,
    openapi_json: Arc<String>,
}

#[derive(Clone)]
struct SessionHandle {
    sender: broadcast::Sender<rip_kernel::Event>,
    events: Arc<Mutex<Vec<rip_kernel::Event>>>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
struct SessionCreated {
    session_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
struct InputPayload {
    input: String,
}

#[derive(OpenApi)]
#[openapi(info(
    title = "RIP Agent Server",
    description = "Agent session control plane (HTTP/SSE).",
    version = "0.1.0"
))]
struct ApiDoc;

#[cfg(not(test))]
#[tokio::main]
async fn main() {
    let app = build_app(data_dir());

    let addr: SocketAddr = "127.0.0.1:7341".parse().expect("addr");
    eprintln!("ripd listening on http://{addr}");

    let listener = TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("server");
}

fn build_app(data_dir: std::path::PathBuf) -> Router {
    let (router, openapi_json) = build_openapi_router();
    let state = AppState {
        sessions: Arc::new(Mutex::new(HashMap::new())),
        event_log: Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("event log")),
        snapshot_dir: Arc::new(data_dir.join("snapshots")),
        runtime: Arc::new(Runtime::new()),
        openapi_json: Arc::new(openapi_json),
    };

    router
        .route("/openapi.json", get(openapi_spec))
        .with_state(state)
}

fn build_openapi_router() -> (Router<AppState>, String) {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(create_session))
        .routes(routes!(send_input))
        .routes(routes!(stream_events))
        .routes(routes!(cancel_session))
        .split_for_parts();
    let json = api
        .to_pretty_json()
        .map(|value| format!("{value}\n"))
        .expect("openapi json");
    (router, json)
}

#[utoipa::path(
    post,
    path = "/sessions",
    responses(
        (status = 201, description = "Session created", body = SessionCreated)
    )
)]
async fn create_session(State(state): State<AppState>) -> impl IntoResponse {
    let session_id = Uuid::new_v4().to_string();
    let (sender, _receiver) = broadcast::channel(128);

    let mut sessions = state.sessions.lock().await;
    sessions.insert(
        session_id.clone(),
        SessionHandle {
            sender,
            events: Arc::new(Mutex::new(Vec::new())),
        },
    );

    (StatusCode::CREATED, Json(SessionCreated { session_id }))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/input",
    params(
        ("id" = String, Path, description = "Session id")
    ),
    request_body = InputPayload,
    responses(
        (status = 202, description = "Input accepted"),
        (status = 404, description = "Session not found")
    )
)]
async fn send_input(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<InputPayload>,
) -> impl IntoResponse {
    let sender = {
        let sessions = state.sessions.lock().await;
        match sessions.get(&session_id) {
            Some(handle) => handle.sender.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let events = {
        let sessions = state.sessions.lock().await;
        match sessions.get(&session_id) {
            Some(handle) => handle.events.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let event_log = state.event_log.clone();
    let snapshot_dir = state.snapshot_dir.clone();
    let runtime = state.runtime.clone();

    tokio::spawn(async move {
        let mut session = runtime.start_session(payload.input);
        while let Some(event) = session.next_event() {
            let _ = sender.send(event.clone());
            let mut guard = events.lock().await;
            guard.push(event.clone());
            let _ = event_log.append(&event);
        }

        let guard = events.lock().await;
        let _ = write_snapshot(&*snapshot_dir, &session_id, &guard);
    });

    StatusCode::ACCEPTED.into_response()
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/events",
    params(
        ("id" = String, Path, description = "Session id")
    ),
    responses(
        (status = 200, description = "SSE stream of event frames"),
        (status = 404, description = "Session not found")
    )
)]
async fn stream_events(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let receiver = {
        let sessions = state.sessions.lock().await;
        match sessions.get(&session_id) {
            Some(handle) => handle.sender.subscribe(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let stream = BroadcastStream::new(receiver).filter_map(|result| async move {
        match result {
            Ok(event) => {
                let json = match serde_json::to_string(&event) {
                    Ok(value) => value,
                    Err(_) => return None,
                };
                Some(Ok::<SseEvent, Infallible>(SseEvent::default().data(json)))
            }
            Err(_) => None,
        }
    });

    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().text("ping"))
        .into_response()
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/cancel",
    params(
        ("id" = String, Path, description = "Session id")
    ),
    responses(
        (status = 204, description = "Session canceled"),
        (status = 404, description = "Session not found")
    )
)]
async fn cancel_session(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let mut sessions = state.sessions.lock().await;
    if sessions.remove(&session_id).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

async fn openapi_spec(State(state): State<AppState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        [(CONTENT_TYPE, "application/json")],
        state.openapi_json.as_str().to_owned(),
    )
}

fn data_dir() -> std::path::PathBuf {
    if let Ok(value) = std::env::var("RIP_DATA_DIR") {
        return std::path::PathBuf::from(value);
    }
    std::path::PathBuf::from("data")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use tokio::time::{sleep, timeout, Duration};
    use tower::util::ServiceExt;

    async fn create_session_id(app: &Router) -> String {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        let payload: SessionCreated = serde_json::from_slice(&body).expect("json");
        payload.session_id
    }

    #[tokio::test]
    async fn openapi_spec_served() {
        let dir = tempdir().expect("tmp");
        let app = build_app(dir.path().join("data"));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");

        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        assert!(content_type.starts_with("application/json"));
        let body = response.into_body().collect().await.expect("body");
        let bytes = body.to_bytes();
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(value
            .get("paths")
            .and_then(|paths| paths.get("/sessions"))
            .is_some());
    }

    #[test]
    fn openapi_snapshot_matches() {
        let json = build_openapi_router().1;
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let path = root.join("schemas/ripd/openapi.json");
        if std::env::var("RIPD_UPDATE_OPENAPI").is_ok() {
            std::fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
            std::fs::write(&path, json).expect("write");
            return;
        }
        let existing = std::fs::read_to_string(&path).expect("snapshot missing");
        assert_eq!(existing, json);
    }

    #[tokio::test]
    async fn create_session_returns_id() {
        let dir = tempdir().expect("tmp");
        let app = build_app(dir.path().join("data"));
        let session_id = create_session_id(&app).await;
        assert!(!session_id.is_empty());
    }

    #[tokio::test]
    async fn send_input_unknown_session_404() {
        let dir = tempdir().expect("tmp");
        let app = build_app(dir.path().join("data"));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sessions/unknown/input")
                    .header("content-type", "application/json")
                    .body(Body::from("{\"input\":\"hi\"}"))
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn stream_events_unknown_session_404() {
        let dir = tempdir().expect("tmp");
        let app = build_app(dir.path().join("data"));
        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/sessions/unknown/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn cancel_unknown_session_404() {
        let dir = tempdir().expect("tmp");
        let app = build_app(dir.path().join("data"));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sessions/unknown/cancel")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn cancel_existing_session_no_content() {
        let dir = tempdir().expect("tmp");
        let app = build_app(dir.path().join("data"));
        let session_id = create_session_id(&app).await;
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/sessions/{session_id}/cancel"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn send_input_accepts_and_writes_snapshot() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let app = build_app(data_dir.clone());
        let session_id = create_session_id(&app).await;
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/sessions/{session_id}/input"))
                    .header("content-type", "application/json")
                    .body(Body::from("{\"input\":\"hi\"}"))
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let snapshot_path = data_dir
            .join("snapshots")
            .join(format!("{session_id}.json"));
        let log_path = data_dir.join("events.jsonl");
        timeout(Duration::from_secs(1), async {
            loop {
                let snapshot_ready = snapshot_path.exists();
                let log_ready = log_path
                    .metadata()
                    .map(|meta| meta.len() > 0)
                    .unwrap_or(false);
                if snapshot_ready && log_ready {
                    break;
                }
                sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("snapshot timeout");
    }

    #[tokio::test]
    async fn stream_events_emits_payload() {
        let dir = tempdir().expect("tmp");
        let app = build_app(dir.path().join("data"));
        let session_id = create_session_id(&app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/sessions/{session_id}/events"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let mut body = response.into_body();

        let send_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/sessions/{session_id}/input"))
                    .header("content-type", "application/json")
                    .body(Body::from("{\"input\":\"hi\"}"))
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(send_response.status(), StatusCode::ACCEPTED);

        let frame = timeout(Duration::from_secs(1), body.frame())
            .await
            .expect("timeout")
            .expect("frame")
            .expect("frame");
        let payload = frame
            .into_data()
            .map(|data| String::from_utf8_lossy(&data).to_string())
            .unwrap_or_default();
        assert!(payload.contains("\"type\""));
    }

    #[tokio::test]
    async fn stream_events_sse_compliance() {
        let dir = tempdir().expect("tmp");
        let app = build_app(dir.path().join("data"));
        let session_id = create_session_id(&app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/sessions/{session_id}/events"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        assert!(content_type.starts_with("text/event-stream"));
        let mut reader = TestSseReader::new(response.into_body());

        let send_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/sessions/{session_id}/input"))
                    .header("content-type", "application/json")
                    .body(Body::from("{\"input\":\"hi\"}"))
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(send_response.status(), StatusCode::ACCEPTED);

        let message = reader.next_data_message().await.expect("message");
        let data_line = message
            .lines()
            .find(|line| line.starts_with("data:"))
            .expect("data line");
        let json = data_line.trim_start_matches("data:").trim();
        let value: serde_json::Value = serde_json::from_str(json).expect("json");
        assert!(value.get("type").is_some());

        for line in message.lines() {
            assert!(line.starts_with("data:") || line.starts_with("event:"));
        }
    }

    #[tokio::test]
    async fn stream_events_preserves_order() {
        let dir = tempdir().expect("tmp");
        let app = build_app(dir.path().join("data"));
        let session_id = create_session_id(&app).await;

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/sessions/{session_id}/events"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let mut reader = TestSseReader::new(response.into_body());

        let send_response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/sessions/{session_id}/input"))
                    .header("content-type", "application/json")
                    .body(Body::from("{\"input\":\"hi\"}"))
                    .unwrap(),
            )
            .await
            .expect("response");
        assert_eq!(send_response.status(), StatusCode::ACCEPTED);

        let first = reader.next_data_message().await.expect("first");
        let second = reader.next_data_message().await.expect("second");
        let first_value = extract_data_json(&first).expect("json");
        let second_value = extract_data_json(&second).expect("json");
        let first_seq = first_value
            .get("seq")
            .and_then(|value| value.as_u64())
            .expect("seq");
        let second_seq = second_value
            .get("seq")
            .and_then(|value| value.as_u64())
            .expect("seq");
        assert!(second_seq > first_seq);
    }

    struct TestSseReader {
        body: Body,
        buffer: String,
    }

    impl TestSseReader {
        fn new(body: Body) -> Self {
            Self {
                body,
                buffer: String::new(),
            }
        }

        async fn next_data_message(&mut self) -> Option<String> {
            loop {
                if let Some((message, remainder)) = split_sse_message(&self.buffer) {
                    self.buffer = remainder;
                    if message.lines().any(|line| line.starts_with("data:")) {
                        return Some(message);
                    }
                }

                let frame = match timeout(Duration::from_secs(1), self.body.frame()).await {
                    Ok(Some(Ok(frame))) => frame,
                    Ok(Some(Err(_))) => return None,
                    Ok(None) => return None,
                    Err(_) => return None,
                };
                if let Ok(data) = frame.into_data() {
                    let payload = String::from_utf8_lossy(&data).to_string();
                    self.buffer.push_str(&payload);
                }
            }
        }
    }

    fn split_sse_message(buffer: &str) -> Option<(String, String)> {
        buffer.find("\n\n").map(|idx| {
            let message = buffer[..idx].to_string();
            let remainder = buffer[idx + 2..].to_string();
            (message, remainder)
        })
    }

    fn extract_data_json(message: &str) -> Option<serde_json::Value> {
        let data_line = message.lines().find(|line| line.starts_with("data:"))?;
        let json = data_line.trim_start_matches("data:").trim();
        serde_json::from_str(json).ok()
    }

    #[test]
    fn data_dir_prefers_env_var() {
        let dir = tempdir().expect("tmp");
        std::env::set_var("RIP_DATA_DIR", dir.path());
        let value = data_dir();
        std::env::remove_var("RIP_DATA_DIR");
        assert_eq!(value, dir.path());
    }
}
