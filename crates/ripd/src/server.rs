use std::{collections::HashMap, convert::Infallible, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::{header::CONTENT_TYPE, StatusCode},
    response::{sse::Event as SseEvent, IntoResponse, Sse},
    routing::get,
    Json, Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_stream::wrappers::BroadcastStream;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::provider_openresponses::OpenResponsesConfig;
use crate::runner::{SessionEngine, SessionHandle};
use crate::tasks::{
    TaskCancelPayload, TaskCreated, TaskEngine, TaskHandle, TaskOutputQuery, TaskOutputResponse,
    TaskSpawnPayload, TaskStatusResponse,
};

#[cfg(not(test))]
use std::net::SocketAddr;
#[cfg(not(test))]
use tokio::net::TcpListener;

#[derive(Clone)]
pub(crate) struct AppState {
    sessions: Arc<Mutex<HashMap<String, SessionHandle>>>,
    tasks: Arc<Mutex<HashMap<String, TaskHandle>>>,
    engine: Arc<SessionEngine>,
    openapi_json: Arc<String>,
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

#[cfg(not(test))]
pub(crate) fn build_app(data_dir: std::path::PathBuf) -> Router {
    build_app_with_workspace_root_and_provider(
        data_dir,
        workspace_root(),
        OpenResponsesConfig::from_env(),
    )
}

#[cfg(test)]
pub(crate) fn build_app_with_workspace_root(
    data_dir: std::path::PathBuf,
    workspace_root: std::path::PathBuf,
) -> Router {
    build_app_with_workspace_root_and_provider(data_dir, workspace_root, None)
}

pub(crate) fn build_app_with_workspace_root_and_provider(
    data_dir: std::path::PathBuf,
    workspace_root: std::path::PathBuf,
    openresponses: Option<OpenResponsesConfig>,
) -> Router {
    let (router, openapi_json) = build_openapi_router();

    let engine = Arc::new(
        SessionEngine::new(data_dir, workspace_root, openresponses).expect("session engine"),
    );

    let state = AppState {
        sessions: Arc::new(Mutex::new(HashMap::new())),
        tasks: Arc::new(Mutex::new(HashMap::new())),
        engine,
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
        .routes(routes!(create_task))
        .routes(routes!(list_tasks))
        .routes(routes!(task_status))
        .routes(routes!(task_output))
        .routes(routes!(stream_task_events))
        .routes(routes!(cancel_task))
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
    let handle = state.engine.create_session();
    let session_id = handle.session_id.clone();

    let mut sessions = state.sessions.lock().await;
    sessions.insert(session_id.clone(), handle);

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
    let handle = {
        let sessions = state.sessions.lock().await;
        match sessions.get(&session_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    state.engine.spawn_session(handle, payload.input);

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
    let handle = {
        let sessions = state.sessions.lock().await;
        match sessions.get(&session_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let receiver = handle.subscribe();
    let past = handle.events_snapshot().await;

    let last_seq = past.last().map(|event| event.seq);
    let past_stream = tokio_stream::iter(past).filter_map(|event| async move {
        let json = serde_json::to_string(&event).ok()?;
        Some(Ok::<SseEvent, Infallible>(SseEvent::default().data(json)))
    });

    let last_seq_live = last_seq;
    let live_stream = BroadcastStream::new(receiver).filter_map(move |result| {
        let last_seq = last_seq_live;
        async move {
            match result {
                Ok(event) => {
                    if last_seq.map(|last| event.seq <= last).unwrap_or(false) {
                        return None;
                    }
                    let json = serde_json::to_string(&event).ok()?;
                    Some(Ok::<SseEvent, Infallible>(SseEvent::default().data(json)))
                }
                Err(_) => None,
            }
        }
    });

    let stream = past_stream.chain(live_stream);

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

#[utoipa::path(
    post,
    path = "/tasks",
    request_body = TaskSpawnPayload,
    responses(
        (status = 201, description = "Task created", body = TaskCreated),
        (status = 400, description = "Invalid task request")
    )
)]
async fn create_task(
    State(state): State<AppState>,
    Json(payload): Json<TaskSpawnPayload>,
) -> impl IntoResponse {
    let mode = payload
        .execution_mode
        .unwrap_or(crate::tasks::ApiToolTaskExecutionMode::Pipes);
    if mode != crate::tasks::ApiToolTaskExecutionMode::Pipes {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if payload.tool != "bash" && payload.tool != "shell" {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let engine: Arc<TaskEngine> = state.engine.tasks();
    let handle = engine.create_task(&payload);
    let task_id = handle.task_id.clone();
    {
        let mut tasks = state.tasks.lock().await;
        tasks.insert(task_id.clone(), handle.clone());
    }

    engine.spawn_task(handle, payload);

    (StatusCode::CREATED, Json(TaskCreated { task_id })).into_response()
}

#[utoipa::path(
    get,
    path = "/tasks",
    responses(
        (status = 200, description = "List tasks", body = [TaskStatusResponse])
    )
)]
async fn list_tasks(State(state): State<AppState>) -> impl IntoResponse {
    let handles = {
        let tasks = state.tasks.lock().await;
        tasks.values().cloned().collect::<Vec<_>>()
    };
    let mut out = Vec::with_capacity(handles.len());
    for handle in handles {
        out.push(handle.status().await);
    }
    Json(out).into_response()
}

#[utoipa::path(
    get,
    path = "/tasks/{id}",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    responses(
        (status = 200, description = "Task status", body = TaskStatusResponse),
        (status = 404, description = "Task not found")
    )
)]
async fn task_status(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };
    Json(handle.status().await).into_response()
}

#[utoipa::path(
    get,
    path = "/tasks/{id}/output",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    responses(
        (status = 200, description = "Fetch task output (range)", body = TaskOutputResponse),
        (status = 404, description = "Task not found")
    )
)]
async fn task_output(
    Path(task_id): Path<String>,
    Query(query): Query<TaskOutputQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let engine: Arc<TaskEngine> = state.engine.tasks();
    let offset = query.offset_bytes.unwrap_or(0);
    let max_bytes = query.max_bytes.unwrap_or(engine.config().max_bytes);

    match handle
        .output(engine.config(), query.stream, offset, max_bytes)
        .await
    {
        Ok(output) => Json(output).into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/tasks/{id}/events",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    responses(
        (status = 200, description = "SSE stream of task event frames"),
        (status = 404, description = "Task not found")
    )
)]
async fn stream_task_events(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let receiver = handle.subscribe();
    let past = handle.events_snapshot().await;

    let last_seq = past.last().map(|event| event.seq);
    let past_stream = tokio_stream::iter(past).filter_map(|event| async move {
        let json = serde_json::to_string(&event).ok()?;
        Some(Ok::<SseEvent, Infallible>(SseEvent::default().data(json)))
    });

    let last_seq_live = last_seq;
    let live_stream = BroadcastStream::new(receiver).filter_map(move |result| {
        let last_seq = last_seq_live;
        async move {
            match result {
                Ok(event) => {
                    if last_seq.map(|last| event.seq <= last).unwrap_or(false) {
                        return None;
                    }
                    let json = serde_json::to_string(&event).ok()?;
                    Some(Ok::<SseEvent, Infallible>(SseEvent::default().data(json)))
                }
                Err(_) => None,
            }
        }
    });

    let stream = past_stream.chain(live_stream);
    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().text("ping"))
        .into_response()
}

#[utoipa::path(
    post,
    path = "/tasks/{id}/cancel",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    request_body = TaskCancelPayload,
    responses(
        (status = 202, description = "Cancel requested"),
        (status = 404, description = "Task not found")
    )
)]
async fn cancel_task(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<TaskCancelPayload>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let reason = payload.reason.unwrap_or_else(|| "cancel".to_string());
    handle.cancel(reason);
    StatusCode::ACCEPTED.into_response()
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
