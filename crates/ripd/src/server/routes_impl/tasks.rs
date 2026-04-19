use super::super::*;
use axum::{
    extract::{Path, Query, State},
    response::{sse::Event as SseEvent, IntoResponse, Sse},
};
use futures_util::StreamExt;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;

use crate::tasks::{
    TaskCancelPayload, TaskCreated, TaskEngine, TaskOutputQuery, TaskOutputResponse,
    TaskResizePayload, TaskSignalPayload, TaskSpawnPayload, TaskStatusResponse,
    TaskWriteStdinPayload,
};

#[utoipa::path(
    post,
    path = "/tasks",
    request_body = TaskSpawnPayload,
    responses(
        (status = 201, description = "Task created", body = TaskCreated),
        (status = 400, description = "Invalid task request")
    )
)]
pub(crate) async fn create_task(
    State(state): State<AppState>,
    Json(payload): Json<TaskSpawnPayload>,
) -> impl IntoResponse {
    let mode = payload
        .execution_mode
        .unwrap_or(crate::tasks::ApiToolTaskExecutionMode::Pipes);
    if mode == crate::tasks::ApiToolTaskExecutionMode::Pty && !state.allow_pty_tasks {
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
pub(crate) async fn list_tasks(State(state): State<AppState>) -> impl IntoResponse {
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
pub(crate) async fn task_status(
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
pub(crate) async fn task_output(
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
pub(crate) async fn stream_task_events(
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
pub(crate) async fn cancel_task(
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

#[utoipa::path(
    post,
    path = "/tasks/{id}/stdin",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    request_body = TaskWriteStdinPayload,
    responses(
        (status = 202, description = "Stdin accepted"),
        (status = 400, description = "Invalid stdin request"),
        (status = 404, description = "Task not found")
    )
)]
pub(crate) async fn task_write_stdin(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<TaskWriteStdinPayload>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    match handle.write_stdin(payload).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/tasks/{id}/resize",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    request_body = TaskResizePayload,
    responses(
        (status = 202, description = "Resize accepted"),
        (status = 400, description = "Invalid resize request"),
        (status = 404, description = "Task not found")
    )
)]
pub(crate) async fn task_resize(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<TaskResizePayload>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    match handle.resize(payload).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/tasks/{id}/signal",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    request_body = TaskSignalPayload,
    responses(
        (status = 202, description = "Signal accepted"),
        (status = 400, description = "Invalid signal request"),
        (status = 404, description = "Task not found")
    )
)]
pub(crate) async fn task_signal(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<TaskSignalPayload>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    match handle.signal(payload).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}
