use super::super::*;
use axum::{
    extract::{Path, State},
    response::{sse::Event as SseEvent, IntoResponse, Sse},
};
use futures_util::StreamExt;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;

#[utoipa::path(
    post,
    path = "/threads/ensure",
    responses(
        (status = 200, description = "Default thread ensured", body = ThreadEnsureResponse)
    )
)]
pub(crate) async fn thread_ensure(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.engine.continuities();
    match store.ensure_default() {
        Ok(thread_id) => Json(ThreadEnsureResponse { thread_id }).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/threads",
    responses(
        (status = 200, description = "List threads", body = [ThreadMeta])
    )
)]
pub(crate) async fn thread_list(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.engine.continuities();
    let mut out = Vec::new();
    for meta in store.list() {
        out.push(ThreadMeta {
            thread_id: meta.continuity_id,
            created_at_ms: meta.created_at_ms,
            title: meta.title,
            archived: meta.archived,
        });
    }
    Json(out).into_response()
}

#[utoipa::path(
    get,
    path = "/threads/{id}",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    responses(
        (status = 200, description = "Thread metadata", body = ThreadMeta),
        (status = 404, description = "Thread not found")
    )
)]
pub(crate) async fn thread_get(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let store = state.engine.continuities();
    match store.get(&thread_id) {
        Some(meta) => Json(ThreadMeta {
            thread_id,
            created_at_ms: meta.created_at_ms,
            title: meta.title,
            archived: meta.archived,
        })
        .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/messages",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = ThreadPostMessagePayload,
    responses(
        (status = 202, description = "Message accepted and run started", body = ThreadPostMessageResponse),
        (status = 404, description = "Thread not found")
    )
)]
pub(crate) async fn thread_post_message(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<ThreadPostMessagePayload>,
) -> impl IntoResponse {
    let ThreadPostMessagePayload {
        content,
        actor_id,
        origin,
        openresponses,
    } = payload;

    let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
    let origin = origin.unwrap_or_else(|| "server".to_string());

    let store = state.engine.continuities();
    let (resolved_openresponses, _loaded) = crate::config::resolve_openresponses_config(
        store.workspace_root(),
        crate::config::OpenResponsesOverrideInput {
            endpoint: openresponses.as_ref().and_then(|cfg| cfg.endpoint.clone()),
            model: openresponses.as_ref().and_then(|cfg| cfg.model.clone()),
            stateless_history: openresponses.as_ref().and_then(|cfg| cfg.stateless_history),
            parallel_tool_calls: openresponses
                .as_ref()
                .and_then(|cfg| cfg.parallel_tool_calls),
            followup_user_message: openresponses
                .as_ref()
                .and_then(|cfg| cfg.followup_user_message.clone()),
        },
    );
    let openresponses_override = resolved_openresponses.map(|cfg| OpenResponsesConfig {
        endpoint: cfg.endpoint,
        api_key: cfg.api_key,
        model: cfg.model,
        headers: cfg.headers,
        tool_choice: ToolChoiceParam::auto(),
        followup_user_message: cfg.followup_user_message,
        stateless_history: cfg.stateless_history,
        parallel_tool_calls: cfg.parallel_tool_calls,
    });
    let message_id = match store.append_message(
        &thread_id,
        actor_id.clone(),
        origin.clone(),
        content.clone(),
    ) {
        Ok(id) => id,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let handle = state.engine.create_session();
    let session_id = handle.session_id.clone();
    {
        let mut sessions = state.sessions.lock().await;
        sessions.insert(session_id.clone(), handle.clone());
    }

    let run_link = crate::continuities::ContinuityRunLink {
        continuity_id: thread_id.clone(),
        message_id: message_id.clone(),
        actor_id: actor_id.clone(),
        origin: origin.clone(),
    };
    if store
        .append_run_spawned(&thread_id, &message_id, &session_id, actor_id, origin)
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    state
        .engine
        .spawn_session(handle, content, Some(run_link), openresponses_override);

    (
        StatusCode::ACCEPTED,
        Json(ThreadPostMessageResponse {
            thread_id,
            message_id,
            session_id,
        }),
    )
        .into_response()
}

#[utoipa::path(
    post,
    path = "/threads/{id}/branch",
    params(
        ("id" = String, Path, description = "Parent thread id")
    ),
    request_body = ThreadBranchPayload,
    responses(
        (status = 201, description = "Branch created", body = ThreadBranchResponse),
        (status = 400, description = "Invalid branch request"),
        (status = 404, description = "Thread or branch point not found")
    )
)]
pub(crate) async fn thread_branch(
    Path(parent_thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<ThreadBranchPayload>,
) -> impl IntoResponse {
    let actor_id = payload.actor_id.unwrap_or_else(|| "user".to_string());
    let origin = payload.origin.unwrap_or_else(|| "server".to_string());

    let store = state.engine.continuities();
    match store.branch(
        &parent_thread_id,
        payload.title,
        payload.from_message_id,
        payload.from_seq,
        actor_id,
        origin,
    ) {
        Ok((thread_id, parent_seq, parent_message_id)) => (
            StatusCode::CREATED,
            Json(ThreadBranchResponse {
                thread_id,
                parent_thread_id,
                parent_seq,
                parent_message_id,
            }),
        )
            .into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("out of range") || err_lower.contains("requires only one of") {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("does not exist") || err_lower.contains("not found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/handoff",
    params(
        ("id" = String, Path, description = "Source thread id")
    ),
    request_body = ThreadHandoffPayload,
    responses(
        (status = 201, description = "Handoff thread created", body = ThreadHandoffResponse),
        (status = 400, description = "Invalid handoff request"),
        (status = 404, description = "Thread or handoff point not found")
    )
)]
pub(crate) async fn thread_handoff(
    Path(from_thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<ThreadHandoffPayload>,
) -> impl IntoResponse {
    let actor_id = payload.actor_id.unwrap_or_else(|| "user".to_string());
    let origin = payload.origin.unwrap_or_else(|| "server".to_string());

    let store = state.engine.continuities();
    match store.handoff(
        &from_thread_id,
        payload.title,
        (payload.summary_markdown, payload.summary_artifact_id),
        payload.from_message_id,
        payload.from_seq,
        (actor_id, origin),
    ) {
        Ok((thread_id, from_seq, from_message_id)) => (
            StatusCode::CREATED,
            Json(ThreadHandoffResponse {
                thread_id,
                from_thread_id,
                from_seq,
                from_message_id,
            }),
        )
            .into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("out of range")
                || err_lower.contains("requires only one of")
                || err_lower.contains("requires summary")
            {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("does not exist") || err_lower.contains("not found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/provider-cursor-status",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::ProviderCursorStatusV1Request,
    responses(
        (status = 200, description = "Provider cursor status projection", body = crate::ProviderCursorStatusV1Response),
        (status = 404, description = "Thread not found")
    )
)]
pub(crate) async fn thread_provider_cursor_status(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<crate::ProviderCursorStatusV1Request>,
) -> impl IntoResponse {
    let store = state.engine.continuities();
    match store.provider_cursor_status_v1(&thread_id, payload) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/context-selection-status",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::ContextSelectionStatusV1Request,
    responses(
        (status = 200, description = "Context selection strategy decisions (truth-derived)", body = crate::ContextSelectionStatusV1Response),
        (status = 404, description = "Thread not found")
    )
)]
pub(crate) async fn thread_context_selection_status(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<crate::ContextSelectionStatusV1Request>,
) -> impl IntoResponse {
    let store = state.engine.continuities();
    match store.context_selection_status_v1(&thread_id, payload) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/provider-cursor-rotate",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::ProviderCursorRotateV1Request,
    responses(
        (status = 200, description = "Provider cursor rotation logged", body = crate::ProviderCursorRotateV1Response),
        (status = 404, description = "Thread not found")
    )
)]
pub(crate) async fn thread_provider_cursor_rotate(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(mut payload): Json<crate::ProviderCursorRotateV1Request>,
) -> impl IntoResponse {
    if payload.actor_id.trim().is_empty() {
        payload.actor_id = "user".to_string();
    }
    if payload.origin.trim().is_empty() {
        payload.origin = "server".to_string();
    }

    let store = state.engine.continuities();
    match store.provider_cursor_rotate_v1(&thread_id, payload) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    get,
    path = "/threads/{id}/events",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    responses(
        (status = 200, description = "SSE stream of thread event frames"),
        (status = 404, description = "Thread not found")
    )
)]
pub(crate) async fn thread_stream_events(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let store = state.engine.continuities();
    let receiver = store.subscribe();

    let past = match store.replay_events(&thread_id) {
        Ok(events) => events,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    if past.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let last_seq = past.last().map(|event| event.seq);
    let past_stream = tokio_stream::iter(past).filter_map(|event| async move {
        let json = serde_json::to_string(&event).ok()?;
        Some(Ok::<SseEvent, Infallible>(SseEvent::default().data(json)))
    });

    let thread_id_live = thread_id.clone();
    let last_seq_live = last_seq;
    let live_stream = BroadcastStream::new(receiver).filter_map(move |result| {
        let last_seq = last_seq_live;
        let thread_id = thread_id_live.clone();
        async move {
            match result {
                Ok(event) => {
                    if event.session_id != thread_id {
                        return None;
                    }
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
