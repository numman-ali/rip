use super::super::*;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
};

#[utoipa::path(
    post,
    path = "/threads/{id}/compaction-checkpoint",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = ThreadCompactionCheckpointPayload,
    responses(
        (status = 201, description = "Compaction checkpoint created", body = ThreadCompactionCheckpointResponse),
        (status = 400, description = "Invalid checkpoint request"),
        (status = 404, description = "Thread or cut point not found")
    )
)]
pub(crate) async fn thread_compaction_checkpoint(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<ThreadCompactionCheckpointPayload>,
) -> impl IntoResponse {
    let actor_id = payload.actor_id.unwrap_or_else(|| "user".to_string());
    let origin = payload.origin.unwrap_or_else(|| "server".to_string());

    let store = state.engine.continuities();
    match store.compaction_checkpoint_cumulative_v1(
        &thread_id,
        crate::CompactionCheckpointCumulativeV1Request {
            summary_markdown: payload.summary_markdown,
            summary_artifact_id: payload.summary_artifact_id,
            to_message_id: payload.to_message_id,
            to_seq: payload.to_seq,
            stride_messages: payload.stride_messages,
            actor_id,
            origin,
        },
    ) {
        Ok((checkpoint_id, summary_artifact_id, to_seq, to_message_id, cut_rule_id)) => (
            StatusCode::CREATED,
            Json(ThreadCompactionCheckpointResponse {
                thread_id,
                checkpoint_id,
                cut_rule_id,
                summary_artifact_id,
                to_seq,
                to_message_id,
            }),
        )
            .into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("out of range")
                || err_lower.contains("requires")
                || err_lower.contains("must")
                || err_lower.contains("mismatch")
                || err_lower.contains("stride")
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
    path = "/threads/{id}/compaction-cut-points",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::CompactionCutPointsV1Request,
    responses(
        (status = 200, description = "Computed cut points", body = crate::CompactionCutPointsV1Response),
        (status = 400, description = "Invalid cut point request"),
        (status = 404, description = "Thread not found")
    )
)]
pub(crate) async fn thread_compaction_cut_points(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<crate::CompactionCutPointsV1Request>,
) -> impl IntoResponse {
    let store = state.engine.continuities();
    match store.compaction_cut_points_v1(&thread_id, payload) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("invalid_stride") {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/compaction-status",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::CompactionStatusV1Request,
    responses(
        (status = 200, description = "Compaction status projection", body = crate::CompactionStatusV1Response),
        (status = 400, description = "Invalid status request"),
        (status = 404, description = "Thread not found")
    )
)]
pub(crate) async fn thread_compaction_status(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<crate::CompactionStatusV1Request>,
) -> impl IntoResponse {
    let store = state.engine.continuities();
    match store.compaction_status_v1(&thread_id, payload) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("invalid_stride") {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/compaction-auto",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::CompactionAutoV1Request,
    responses(
        (status = 200, description = "Auto-compaction no-op / dry-run result", body = crate::CompactionAutoV1Response),
        (status = 202, description = "Auto-compaction job spawned", body = crate::CompactionAutoV1Response),
        (status = 400, description = "Invalid auto-compaction request"),
        (status = 404, description = "Thread not found")
    )
)]
pub(crate) async fn thread_compaction_auto(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(mut payload): Json<crate::CompactionAutoV1Request>,
) -> impl IntoResponse {
    if payload.actor_id.trim().is_empty() {
        payload.actor_id = "user".to_string();
    }
    if payload.origin.trim().is_empty() {
        payload.origin = "server".to_string();
    }

    let store = state.engine.continuities();
    let actor_id = payload.actor_id.clone();
    let origin = payload.origin.clone();

    let response = match store.compaction_auto_spawn_job_v1(&thread_id, payload) {
        Ok(response) => response,
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("invalid_stride") {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if response.status == "spawned" {
        let Some(job_id) = response.job_id.clone() else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        let planned = response.planned.clone();
        let stride_messages = response.stride_messages;
        let cut_rule_id = response.cut_rule_id.clone();
        let store = store.clone();
        let thread_id = thread_id.clone();
        tokio::spawn(async move {
            let _ = tokio::task::spawn_blocking(move || {
                let _ = store.compaction_auto_run_spawned_job_v1(
                    &thread_id,
                    &job_id,
                    stride_messages,
                    &cut_rule_id,
                    &planned,
                    (actor_id.as_str(), origin.as_str()),
                );
            })
            .await;
        });

        return (StatusCode::ACCEPTED, Json(response)).into_response();
    }

    (StatusCode::OK, Json(response)).into_response()
}

#[utoipa::path(
    post,
    path = "/threads/{id}/compaction-auto-schedule",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::CompactionAutoScheduleV1Request,
    responses(
        (status = 200, description = "Scheduler no-op / dry-run / skipped result", body = crate::CompactionAutoScheduleV1Response),
        (status = 202, description = "Scheduler started a job", body = crate::CompactionAutoScheduleV1Response),
        (status = 400, description = "Invalid schedule request"),
        (status = 404, description = "Thread not found")
    )
)]
pub(crate) async fn thread_compaction_auto_schedule(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(mut payload): Json<crate::CompactionAutoScheduleV1Request>,
) -> impl IntoResponse {
    if payload.actor_id.trim().is_empty() {
        payload.actor_id = "user".to_string();
    }
    if payload.origin.trim().is_empty() {
        payload.origin = "server".to_string();
    }

    let store = state.engine.continuities();
    let actor_id = payload.actor_id.clone();
    let origin = payload.origin.clone();

    let response = match store.compaction_auto_schedule_spawn_job_v1(&thread_id, payload) {
        Ok(response) => response,
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("invalid_stride") {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if response.decision == "scheduled" {
        if response.execute {
            let Some(job_id) = response.job_id.clone() else {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            };
            let planned = response.planned.clone();
            let stride_messages = response.stride_messages;
            let cut_rule_id = response.cut_rule_id.clone();
            let store = store.clone();
            let thread_id = thread_id.clone();
            tokio::spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = store.compaction_auto_run_spawned_job_v1(
                        &thread_id,
                        &job_id,
                        stride_messages,
                        &cut_rule_id,
                        &planned,
                        (actor_id.as_str(), origin.as_str()),
                    );
                })
                .await;
            });
        }

        return (StatusCode::ACCEPTED, Json(response)).into_response();
    }

    (StatusCode::OK, Json(response)).into_response()
}
