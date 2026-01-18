use axum::body::Body;
use axum::http::Request;
use axum::Router;
use http_body_util::BodyExt;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;
use tokio::time::{sleep, timeout, Duration};
use tower::util::ServiceExt;

use crate::server::{
    build_app_with_workspace_root, build_openapi_router, workspace_root, SessionCreated,
};

fn build_test_app(dir: &tempfile::TempDir) -> Router {
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace dir");
    build_app_with_workspace_root(data_dir, workspace_dir)
}

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
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let payload: SessionCreated = serde_json::from_slice(&body).expect("json");
    payload.session_id
}

#[test]
fn workspace_root_returns_value() {
    let root = workspace_root();
    let func: fn() -> PathBuf = workspace_root;
    let pointer_root = func();
    assert!(!root.as_os_str().is_empty());
    assert!(!pointer_root.as_os_str().is_empty());
}

#[tokio::test]
async fn openapi_spec_served() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);

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

    assert_eq!(response.status(), axum::http::StatusCode::OK);
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
    let app = build_test_app(&dir);
    let session_id = create_session_id(&app).await;
    assert!(!session_id.is_empty());
}

#[tokio::test]
async fn send_input_unknown_session_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
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
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stream_events_unknown_session_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
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
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cancel_unknown_session_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
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
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cancel_existing_session_no_content() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
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
    assert_eq!(response.status(), axum::http::StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn send_input_accepts_and_writes_snapshot() {
    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace dir");
    let app = build_app_with_workspace_root(data_dir.clone(), workspace_dir);
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
    assert_eq!(response.status(), axum::http::StatusCode::ACCEPTED);

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
    let app = build_test_app(&dir);
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
    assert_eq!(response.status(), axum::http::StatusCode::OK);
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
    assert_eq!(send_response.status(), axum::http::StatusCode::ACCEPTED);

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
    let app = build_test_app(&dir);
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
    assert_eq!(response.status(), axum::http::StatusCode::OK);
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
    assert_eq!(send_response.status(), axum::http::StatusCode::ACCEPTED);

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
    let app = build_test_app(&dir);
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
    assert_eq!(response.status(), axum::http::StatusCode::OK);
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
    assert_eq!(send_response.status(), axum::http::StatusCode::ACCEPTED);

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

#[tokio::test]
async fn tool_input_emits_checkpoint_events() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
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
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let payload = serde_json::json!({
        "tool": "write",
        "args": {"path": "a.txt", "content": "hello"}
    })
    .to_string();

    let body = serde_json::json!({ "input": payload }).to_string();
    let send_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sessions/{session_id}/input"))
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(send_response.status(), axum::http::StatusCode::ACCEPTED);

    let mut saw_checkpoint = false;
    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                if value.get("type").and_then(|value| value.as_str()) == Some("checkpoint_created")
                {
                    saw_checkpoint = true;
                    break;
                }
            }
        }
    })
    .await
    .expect("checkpoint timeout");

    assert!(saw_checkpoint, "expected checkpoint_created event");
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

            let frame = self.body.frame().await?.ok()?;
            let data = frame.into_data().ok()?;
            let text = String::from_utf8_lossy(&data);
            self.buffer.push_str(&text);
        }
    }
}

fn split_sse_message(buffer: &str) -> Option<(String, String)> {
    let marker = "\n\n";
    let idx = buffer.find(marker)?;
    let message = buffer[..idx].to_string();
    let remainder = buffer[idx + marker.len()..].to_string();
    Some((message, remainder))
}

fn extract_data_json(message: &str) -> Option<serde_json::Value> {
    let data_line = message.lines().find(|line| line.starts_with("data:"))?;
    let json = data_line.trim_start_matches("data:").trim();
    serde_json::from_str(json).ok()
}
