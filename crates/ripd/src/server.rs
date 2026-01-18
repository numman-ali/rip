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
use rip_log::EventLog;
use rip_tools::{register_builtin_tools, BuiltinToolConfig, ToolRegistry, ToolRunner};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};
use tokio_stream::wrappers::BroadcastStream;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

use crate::checkpoints::WorkspaceCheckpointHook;
use crate::session::{run_session, SessionContext};

#[cfg(not(test))]
use std::net::SocketAddr;
#[cfg(not(test))]
use tokio::net::TcpListener;

const TOOL_MAX_CONCURRENCY: usize = 4;

#[derive(Clone)]
pub(crate) struct AppState {
    sessions: Arc<Mutex<HashMap<String, SessionHandle>>>,
    event_log: Arc<EventLog>,
    snapshot_dir: Arc<std::path::PathBuf>,
    runtime: Arc<Runtime>,
    tool_runner: Arc<ToolRunner>,
    openapi_json: Arc<String>,
}

#[derive(Clone)]
struct SessionHandle {
    sender: broadcast::Sender<rip_kernel::Event>,
    events: Arc<Mutex<Vec<rip_kernel::Event>>>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub(crate) struct SessionCreated {
    pub(crate) session_id: String,
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
pub(crate) async fn serve(data_dir: std::path::PathBuf) {
    let app = build_app(data_dir);

    let addr: SocketAddr = "127.0.0.1:7341".parse().expect("addr");
    eprintln!("ripd listening on http://{addr}");

    let listener = TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("server");
}

pub(crate) fn build_app(data_dir: std::path::PathBuf) -> Router {
    let (router, openapi_json) = build_openapi_router();
    let workspace_root = workspace_root();

    let registry = Arc::new(ToolRegistry::default());
    register_builtin_tools(
        &registry,
        BuiltinToolConfig {
            workspace_root: workspace_root.clone(),
            ..BuiltinToolConfig::default()
        },
    );

    let checkpoint_hook =
        WorkspaceCheckpointHook::new(workspace_root).expect("workspace checkpoint hook");
    let tool_runner = Arc::new(ToolRunner::with_checkpoint_hook(
        registry,
        TOOL_MAX_CONCURRENCY,
        Arc::new(checkpoint_hook),
    ));

    let state = AppState {
        sessions: Arc::new(Mutex::new(HashMap::new())),
        event_log: Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("event log")),
        snapshot_dir: Arc::new(data_dir.join("snapshots")),
        runtime: Arc::new(Runtime::new()),
        tool_runner,
        openapi_json: Arc::new(openapi_json),
    };

    router
        .route("/openapi.json", get(openapi_spec))
        .with_state(state)
}

pub(crate) fn build_openapi_router() -> (Router<AppState>, String) {
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
    let tool_runner = state.tool_runner.clone();
    let server_session_id = session_id.clone();

    tokio::spawn(async move {
        run_session(SessionContext {
            runtime,
            tool_runner,
            sender,
            events,
            event_log,
            snapshot_dir,
            server_session_id,
            input: payload.input,
        })
        .await;
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

#[cfg(not(test))]
pub(crate) fn data_dir() -> std::path::PathBuf {
    if let Ok(value) = std::env::var("RIP_DATA_DIR") {
        return std::path::PathBuf::from(value);
    }
    std::path::PathBuf::from("data")
}

#[cfg_attr(test, inline(never))]
pub(crate) fn workspace_root() -> std::path::PathBuf {
    if let Ok(value) = std::env::var("RIP_WORKSPACE_ROOT") {
        return std::path::PathBuf::from(value);
    }
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}
