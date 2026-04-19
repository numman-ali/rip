use super::super::*;
use axum::{
    extract::{Path, State},
    response::{sse::Event as SseEvent, IntoResponse, Sse},
};
use futures_util::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

#[utoipa::path(
    post,
    path = "/sessions",
    responses(
        (status = 201, description = "Session created", body = SessionCreated)
    )
)]
pub(crate) async fn create_session(State(state): State<AppState>) -> impl IntoResponse {
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
pub(crate) async fn send_input(
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

    state
        .engine
        .spawn_session(handle, payload.input, None, None);

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
pub(crate) async fn stream_events(
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
        Some(Ok::<SseEvent, std::convert::Infallible>(
            SseEvent::default().data(json),
        ))
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
                    Some(Ok::<SseEvent, std::convert::Infallible>(
                        SseEvent::default().data(json),
                    ))
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
pub(crate) async fn cancel_session(
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
