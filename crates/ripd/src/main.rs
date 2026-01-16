use std::{collections::HashMap, convert::Infallible, net::SocketAddr, sync::Arc};

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{sse::Event as SseEvent, IntoResponse, Sse},
    routing::{get, post},
    Json, Router,
};
use futures_util::StreamExt;
use rip_kernel::Runtime;
use rip_log::{write_snapshot, EventLog};
use serde::{Deserialize, Serialize};
use tokio::{
    net::TcpListener,
    sync::{broadcast, Mutex},
};
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    sessions: Arc<Mutex<HashMap<String, SessionHandle>>>,
    event_log: Arc<EventLog>,
    snapshot_dir: Arc<std::path::PathBuf>,
    runtime: Arc<Runtime>,
}

#[derive(Clone)]
struct SessionHandle {
    sender: broadcast::Sender<rip_kernel::Event>,
    events: Arc<Mutex<Vec<rip_kernel::Event>>>,
}

#[derive(Debug, Serialize)]
struct SessionCreated {
    session_id: String,
}

#[derive(Debug, Deserialize)]
struct InputPayload {
    input: String,
}

#[tokio::main]
async fn main() {
    let state = AppState {
        sessions: Arc::new(Mutex::new(HashMap::new())),
        event_log: Arc::new(EventLog::new(data_dir().join("events.jsonl")).expect("event log")),
        snapshot_dir: Arc::new(data_dir().join("snapshots")),
        runtime: Arc::new(Runtime::new()),
    };

    let app = Router::new()
        .route("/sessions", post(create_session))
        .route("/sessions/:id/input", post(send_input))
        .route("/sessions/:id/events", get(stream_events))
        .route("/sessions/:id/cancel", post(cancel_session))
        .with_state(state);

    let addr: SocketAddr = "127.0.0.1:7341".parse().expect("addr");
    eprintln!("ripd listening on http://{addr}");

    let listener = TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("server");
}

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

fn data_dir() -> std::path::PathBuf {
    if let Ok(value) = std::env::var("RIP_DATA_DIR") {
        return std::path::PathBuf::from(value);
    }
    std::path::PathBuf::from("data")
}
