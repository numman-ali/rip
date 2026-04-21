//! Frame → `CanvasMessage` ingestion rules (Phase B.1).
//!
//! Takes frames from the continuity/run stream and translates them into the
//! structured canvas model. The renderer (B.2+) reads this model instead of
//! the string-based `output_text`.
//!
//! Invariants the revamp plan fixes in stone:
//!
//! - Nothing here reads external state; dispatch happens only on the frame's
//!   own fields. The kernel is responsible for emitting correct
//!   `job_kind` / `role` / `actor_id` / `origin`; the TUI renders them.
//! - `JobNotice` rides existing `ContinuityJobSpawned` / `ContinuityJobEnded`
//!   frames, not imagined `ContinuityRetrieval*` / `ContinuityReviewer*`
//!   kinds. A new `job_kind` value flows through with zero ingestion
//!   changes (glyph lookup is a render concern).
//! - `ExtensionPanel` is declared but never produced here — it lands when
//!   the P2 `extension.ui` capability ships.
//!
//! Dispatch lives here; per-message-type handlers live in the sibling
//! modules (`turns`, `cards`, `notices`) so each concern stays readable
//! on its own.

mod cards;
mod notices;
mod turns;

#[cfg(test)]
use rip_kernel::ToolTaskStatus;
use rip_kernel::{Event, EventKind, ProviderEventStatus};
use serde_json::Value;

#[cfg(test)]
use super::model::*;
use super::CanvasModel;
use crate::provider_event;

pub(super) fn apply(canvas: &mut CanvasModel, event: &Event) {
    match &event.kind {
        EventKind::SessionStarted { input } => turns::on_session_started(canvas, event, input),
        EventKind::OutputTextDelta { delta } => turns::append_agent_delta(canvas, delta),
        EventKind::SessionEnded { .. } => turns::finalize_agent_turn(canvas, event.timestamp_ms),
        EventKind::ToolStarted {
            tool_id,
            name,
            args,
            ..
        } => cards::push_tool_card(canvas, event, tool_id, name, args),
        EventKind::ToolStdout { tool_id, chunk } => {
            cards::append_tool_body(canvas, tool_id, ToolStream::Stdout, chunk);
        }
        EventKind::ToolStderr { tool_id, chunk } => {
            cards::append_tool_body(canvas, tool_id, ToolStream::Stderr, chunk);
        }
        EventKind::ToolEnded {
            tool_id,
            exit_code,
            duration_ms,
            artifacts,
        } => cards::finalize_tool_card_success(
            canvas,
            tool_id,
            *exit_code,
            *duration_ms,
            artifacts.as_ref(),
        ),
        EventKind::ToolFailed { tool_id, error } => {
            cards::finalize_tool_card_failure(canvas, tool_id, error);
        }
        EventKind::ToolTaskSpawned {
            task_id,
            tool_name,
            title,
            execution_mode,
            artifacts,
            ..
        } => cards::push_task_card(
            canvas,
            task_id,
            tool_name,
            title.clone(),
            *execution_mode,
            artifacts.as_ref(),
        ),
        EventKind::ToolTaskStatus {
            task_id,
            status,
            exit_code,
            started_at_ms,
            artifacts,
            error,
            ..
        } => cards::update_task_card_status(
            canvas,
            task_id,
            *status,
            *exit_code,
            *started_at_ms,
            error.as_deref(),
            artifacts.as_ref(),
        ),
        EventKind::ToolTaskOutputDelta {
            task_id,
            stream,
            chunk,
            ..
        } => {
            let kind = match stream {
                rip_kernel::ToolTaskStream::Stdout => ToolStream::Stdout,
                rip_kernel::ToolTaskStream::Stderr => ToolStream::Stderr,
                rip_kernel::ToolTaskStream::Pty => ToolStream::Stdout,
            };
            cards::append_task_body(canvas, task_id, kind, chunk);
        }
        EventKind::ProviderEvent {
            event_name,
            data,
            status,
            errors,
            response_errors,
            ..
        } => {
            if ingest_reasoning_event(canvas, event_name.as_deref(), data.as_ref()) {
                return;
            }
            if ingest_compat_warning_notice(canvas, event, event_name.as_deref(), data.as_ref()) {
                return;
            }
            if is_provider_error(status, errors, response_errors) {
                notices::push_system_notice(
                    canvas,
                    event,
                    super::model::NoticeLevel::Danger,
                    provider_error_notice_text(errors, response_errors),
                    "provider_event",
                );
            }
        }
        EventKind::CheckpointFailed { error, .. } => notices::push_system_notice(
            canvas,
            event,
            super::model::NoticeLevel::Danger,
            format!("Checkpoint failed: {error}"),
            "checkpoint_failed",
        ),
        EventKind::ContinuityJobSpawned {
            job_id,
            job_kind,
            details,
            actor_id,
            origin,
        } => notices::push_job_notice(
            canvas,
            event,
            job_id,
            job_kind,
            details.clone(),
            actor_id,
            origin,
        ),
        EventKind::ContinuityJobEnded {
            job_id,
            status,
            error,
            result,
            ..
        } => notices::update_job_notice(
            canvas,
            job_id,
            status,
            result.as_ref(),
            error.as_deref(),
            event.timestamp_ms,
        ),
        EventKind::ContinuityContextSelectionDecided {
            run_session_id,
            compiler_strategy,
            ..
        } => notices::upsert_context_notice(
            canvas,
            run_session_id,
            compiler_strategy,
            super::model::ContextLifecycle::Selecting,
            None,
        ),
        EventKind::ContinuityContextCompiled {
            run_session_id,
            compiler_strategy,
            bundle_artifact_id,
            ..
        } => notices::upsert_context_notice(
            canvas,
            run_session_id,
            compiler_strategy,
            super::model::ContextLifecycle::Compiled,
            Some(bundle_artifact_id.clone()),
        ),
        EventKind::ContinuityCompactionCheckpointCreated {
            checkpoint_id,
            from_seq,
            to_seq,
            summary_artifact_id,
            ..
        } => notices::push_compaction_checkpoint(
            canvas,
            checkpoint_id,
            *from_seq,
            *to_seq,
            summary_artifact_id,
        ),
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum ToolStream {
    Stdout,
    Stderr,
}

fn ingest_reasoning_event(
    canvas: &mut CanvasModel,
    event_name: Option<&str>,
    data: Option<&Value>,
) -> bool {
    let Some(event_type) = provider_event::event_type(event_name, data) else {
        return false;
    };
    let payload = data.and_then(Value::as_object);
    match event_type {
        "response.reasoning.delta" | "response.reasoning_text.delta" => {
            if let Some(delta) = payload
                .and_then(|value| value.get("delta"))
                .and_then(Value::as_str)
            {
                turns::append_agent_reasoning_delta(canvas, delta);
                return true;
            }
        }
        "response.reasoning.done" | "response.reasoning_text.done" => {
            let text = payload
                .and_then(|value| value.get("text"))
                .and_then(Value::as_str);
            turns::finalize_agent_reasoning(canvas, text);
            return true;
        }
        "response.reasoning_summary_text.delta" => {
            if let Some(delta) = payload
                .and_then(|value| value.get("delta"))
                .and_then(Value::as_str)
            {
                turns::append_agent_reasoning_summary_delta(canvas, delta);
                return true;
            }
        }
        "response.reasoning_summary_text.done" => {
            let text = payload
                .and_then(|value| value.get("text"))
                .and_then(Value::as_str);
            turns::finalize_agent_reasoning_summary(canvas, text);
            return true;
        }
        _ => {}
    }
    false
}

fn provider_error_notice_text(errors: &[String], response_errors: &[String]) -> String {
    let first = errors
        .iter()
        .chain(response_errors.iter())
        .find(|value| !value.trim().is_empty())
        .cloned();
    match first {
        Some(message) => format!("Provider error: {message}"),
        None => "Provider error".to_string(),
    }
}

fn ingest_compat_warning_notice(
    canvas: &mut CanvasModel,
    event: &Event,
    event_name: Option<&str>,
    data: Option<&Value>,
) -> bool {
    let Some(event_type) = provider_event::event_type(event_name, data) else {
        return false;
    };
    if event_type != "rip.compat.warning" {
        return false;
    }
    let text = data
        .and_then(Value::as_object)
        .and_then(|payload| payload.get("message"))
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .unwrap_or("Compatibility warning");
    notices::push_system_notice(
        canvas,
        event,
        super::model::NoticeLevel::Warn,
        text.to_string(),
        "provider_event",
    );
    true
}

fn is_provider_error(
    status: &ProviderEventStatus,
    errors: &[String],
    response_errors: &[String],
) -> bool {
    *status == ProviderEventStatus::InvalidJson
        || (*status != ProviderEventStatus::Done
            && (!errors.is_empty() || !response_errors.is_empty()))
}

#[cfg(test)]
mod tests;
