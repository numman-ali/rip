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
            if ingest_hosted_tool_item_event(canvas, event, event_name.as_deref(), data.as_ref()) {
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
        "response.output_item.added" | "response.output_item.done" => {
            if let Some(item) = payload.and_then(|value| value.get("item")) {
                return turns::ingest_reasoning_item(canvas, item);
            }
        }
        "response.completed" => {
            if let Some(items) = payload
                .and_then(|value| value.get("response"))
                .and_then(|value| value.get("output"))
                .and_then(Value::as_array)
            {
                let mut matched = false;
                for item in items {
                    matched |= turns::ingest_reasoning_item(canvas, item);
                }
                return matched;
            }
        }
        _ => {}
    }
    false
}

fn ingest_hosted_tool_item_event(
    canvas: &mut CanvasModel,
    event: &Event,
    event_name: Option<&str>,
    data: Option<&Value>,
) -> bool {
    let Some(event_type) = provider_event::event_type(event_name, data) else {
        return false;
    };
    if !matches!(
        event_type,
        "response.output_item.added" | "response.output_item.done"
    ) {
        return false;
    }
    let Some(item) = data
        .and_then(Value::as_object)
        .and_then(|payload| payload.get("item"))
    else {
        return false;
    };
    let Some(hosted) = HostedToolItem::from_value(item) else {
        return false;
    };

    match event_type {
        "response.output_item.added" => {
            let args = serde_json::json!({
                "provider_item_type": hosted.item_type,
                "status": hosted.status,
            });
            cards::push_tool_card(canvas, event, hosted.id, hosted.tool_name, &args);
        }
        "response.output_item.done" => {
            if hosted.is_failed() {
                cards::finalize_tool_card_failure(
                    canvas,
                    hosted.id,
                    &hosted.error.unwrap_or_else(|| hosted.status.to_string()),
                );
            } else {
                cards::finalize_tool_card_success_at(canvas, hosted.id, event.timestamp_ms);
            }
        }
        _ => {}
    }
    true
}

struct HostedToolItem<'a> {
    id: &'a str,
    item_type: &'a str,
    tool_name: &'a str,
    status: &'a str,
    error: Option<String>,
}

impl<'a> HostedToolItem<'a> {
    fn from_value(value: &'a Value) -> Option<Self> {
        let map = value.as_object()?;
        let id = map.get("id").and_then(Value::as_str)?.trim();
        let item_type = map.get("type").and_then(Value::as_str)?.trim();
        let status = map.get("status").and_then(Value::as_str)?.trim();
        if id.is_empty() || item_type.is_empty() || status.is_empty() {
            return None;
        }
        let tool_name = hosted_tool_name(item_type)?;
        let error = map
            .get("error")
            .and_then(extract_hosted_tool_error)
            .filter(|value| !value.trim().is_empty());
        Some(Self {
            id,
            item_type,
            tool_name,
            status,
            error,
        })
    }

    fn is_failed(&self) -> bool {
        matches!(self.status, "failed" | "incomplete")
    }
}

fn hosted_tool_name(item_type: &str) -> Option<&str> {
    match item_type {
        "web_search_call"
        | "file_search_call"
        | "code_interpreter_call"
        | "image_generation_call"
        | "mcp_call"
        | "computer_call" => Some(item_type.trim_end_matches("_call")),
        prefixed if is_prefixed_extension_item_type(prefixed) => Some(prefixed),
        _ => None,
    }
}

fn is_prefixed_extension_item_type(item_type: &str) -> bool {
    let Some((slug, name)) = item_type.split_once(':') else {
        return false;
    };
    !name.trim().is_empty()
        && slug
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

fn extract_hosted_tool_error(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| {
            value
                .get("message")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .or_else(|| Some(value.to_string()))
}

fn provider_error_notice_text(errors: &[String], response_errors: &[String]) -> String {
    let first = errors
        .iter()
        .chain(response_errors.iter())
        .find(|value| !value.trim().is_empty())
        .cloned();
    match first {
        Some(message) => summarize_provider_error_message(&message),
        None => "Provider error".to_string(),
    }
}

fn summarize_provider_error_message(message: &str) -> String {
    let compact = collapse_whitespace(message);
    let status = extract_http_status(message);
    let provider_message = extract_json_error_message(message).unwrap_or_else(|| compact.clone());
    match status {
        Some(status) if provider_message != compact => {
            format!("Provider error: {provider_message} ({status})")
        }
        _ => format!("Provider error: {provider_message}"),
    }
}

fn extract_http_status(message: &str) -> Option<String> {
    let rest = message.strip_prefix("provider http error:")?;
    let status = rest
        .split('{')
        .next()
        .unwrap_or(rest)
        .trim()
        .trim_end_matches(':')
        .trim();
    (!status.is_empty()).then(|| status.to_string())
}

fn extract_json_error_message(message: &str) -> Option<String> {
    let json_start = message.find('{')?;
    let value: Value = serde_json::from_str(message[json_start..].trim()).ok()?;
    let message = value
        .get("error")
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))?
        .trim();
    (!message.is_empty()).then(|| message.to_string())
}

fn collapse_whitespace(message: &str) -> String {
    message.split_whitespace().collect::<Vec<_>>().join(" ")
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
