//! System / job / context / compaction notice ingestion.
//!
//! Everything that isn't a turn or a card — provider errors, kernel
//! background jobs, context compiler decisions, compaction checkpoints
//! — lands here. `push_job_notice` + `update_job_notice` ride the
//! existing `ContinuityJobSpawned` / `ContinuityJobEnded` pair; the
//! TUI does not invent new frame kinds for retrieval / reviewer / etc.
//! The `job_kind` string flows through untouched so a new kernel-side
//! job type renders with zero ingestion changes (only the glyph
//! lookup in the renderer needs updating).

use rip_kernel::Event;
use serde_json::Value;

use super::super::model::*;
use super::super::CanvasModel;

pub(super) fn push_system_notice(
    canvas: &mut CanvasModel,
    event: &Event,
    level: NoticeLevel,
    text: String,
    origin_event_kind: &str,
) {
    let id = canvas.mint_id();
    canvas.messages.push(CanvasMessage::SystemNotice {
        message_id: id,
        level,
        text,
        origin_event_kind: origin_event_kind.to_string(),
        seq: event.seq,
    });
}

pub(super) fn push_job_notice(
    canvas: &mut CanvasModel,
    event: &Event,
    job_id: &str,
    job_kind: &str,
    details: Option<Value>,
    actor_id: &str,
    origin: &str,
) {
    let id = canvas.mint_id();
    canvas.messages.push(CanvasMessage::JobNotice {
        message_id: id,
        job_id: job_id.to_string(),
        job_kind: job_kind.to_string(),
        details,
        status: JobLifecycle::Running,
        actor_id: actor_id.to_string(),
        origin: origin.to_string(),
        started_at_ms: Some(event.timestamp_ms),
        ended_at_ms: None,
    });
}

pub(super) fn update_job_notice(
    canvas: &mut CanvasModel,
    job_id: &str,
    status_str: &str,
    result: Option<&Value>,
    error: Option<&str>,
    now_ms: u64,
) {
    let lifecycle = match status_str {
        "completed" | "succeeded" | "success" => JobLifecycle::Succeeded {
            result: result.cloned(),
        },
        "cancelled" | "canceled" => JobLifecycle::Cancelled,
        _ => JobLifecycle::Failed {
            error: error.map(ToString::to_string),
        },
    };
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::JobNotice {
            job_id: id,
            status,
            ended_at_ms,
            ..
        } = message
        {
            if id == job_id {
                *status = lifecycle;
                *ended_at_ms = Some(now_ms);
                return;
            }
        }
    }
}

pub(super) fn upsert_context_notice(
    canvas: &mut CanvasModel,
    run_session_id: &str,
    strategy: &str,
    lifecycle: ContextLifecycle,
    bundle_artifact_id: Option<String>,
) {
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::ContextNotice {
            run_session_id: id,
            strategy: current_strategy,
            status,
            bundle_artifact_id: current_bundle,
            ..
        } = message
        {
            if id == run_session_id {
                *current_strategy = strategy.to_string();
                *status = lifecycle;
                if bundle_artifact_id.is_some() {
                    *current_bundle = bundle_artifact_id;
                }
                return;
            }
        }
    }
    let id = canvas.mint_id();
    canvas.messages.push(CanvasMessage::ContextNotice {
        message_id: id,
        run_session_id: run_session_id.to_string(),
        strategy: strategy.to_string(),
        status: lifecycle,
        bundle_artifact_id,
        contributed_artifact_ids: Vec::new(),
    });
}

pub(super) fn push_compaction_checkpoint(
    canvas: &mut CanvasModel,
    checkpoint_id: &str,
    from_seq: u64,
    to_seq: u64,
    summary_artifact_id: &str,
) {
    let id = canvas.mint_id();
    canvas.messages.push(CanvasMessage::CompactionCheckpoint {
        message_id: id,
        checkpoint_id: checkpoint_id.to_string(),
        from_seq,
        to_seq,
        summary_artifact_id: summary_artifact_id.to_string(),
    });
}
