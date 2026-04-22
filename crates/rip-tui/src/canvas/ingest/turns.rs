//! User + agent turn ingestion.
//!
//! Turns the `SessionStarted` / `OutputTextDelta` / `SessionEnded` triple
//! into `UserTurn` / `AgentTurn` messages. The streaming collector
//! lives on the `AgentTurn` itself so long code blocks only scan newly
//! appended lines instead of rebuilding from the whole tail every time.

use rip_kernel::Event;

use super::super::model::*;
use super::super::stream_collector::StreamCollector;
use super::super::CanvasModel;

pub(super) fn on_session_started(canvas: &mut CanvasModel, event: &Event, input: &str) {
    // If `begin_pending_turn` already pushed a `UserTurn`, the last message is
    // it; skip duplicating. Otherwise the run started without surface-side
    // submission (replay, backend-originated run) and we materialize the
    // implied user turn from the frame's `input`.
    let already_pending = matches!(canvas.messages.last(), Some(CanvasMessage::UserTurn { .. }));
    if !already_pending {
        let trimmed = input.trim();
        if !trimmed.is_empty() {
            let id = canvas.mint_id();
            canvas.messages.push(CanvasMessage::UserTurn {
                message_id: id,
                actor_id: "user".to_string(),
                origin: "frame".to_string(),
                blocks: vec![Block::Paragraph(CachedText::plain(trimmed))],
                submitted_at_ms: event.timestamp_ms,
            });
        }
    }

    let id = canvas.mint_id();
    canvas.messages.push(CanvasMessage::AgentTurn {
        message_id: id,
        run_session_id: event.session_id.clone(),
        agent_id: None,
        role: AgentRole::Primary,
        actor_id: "agent".to_string(),
        model: None,
        reasoning_seen: false,
        reasoning_text: String::new(),
        reasoning_summary: String::new(),
        blocks: Vec::new(),
        streaming_tail: String::new(),
        streaming_collector: StreamCollector::new(),
        streaming: true,
        started_at_ms: event.timestamp_ms,
        ended_at_ms: None,
    });
}

/// Feed a streaming delta through the collector and promote any
/// completed paragraphs (B.5). Tail text lives on the message itself.
pub(super) fn append_agent_delta(canvas: &mut CanvasModel, delta: &str) {
    if delta.is_empty() {
        return;
    }
    for message in canvas.messages.iter_mut().rev() {
        let CanvasMessage::AgentTurn {
            blocks,
            reasoning_text: _,
            reasoning_summary: _,
            streaming,
            streaming_tail,
            streaming_collector,
            ..
        } = message
        else {
            continue;
        };
        if !*streaming {
            continue;
        }
        let step = streaming_collector.push(streaming_tail, delta);
        blocks.extend(step.new_stable);
        return;
    }
}

pub(super) fn append_agent_reasoning_delta(canvas: &mut CanvasModel, delta: &str) {
    if delta.is_empty() {
        return;
    }
    if let Some(CanvasMessage::AgentTurn {
        reasoning_seen,
        reasoning_text,
        ..
    }) = canvas.messages.iter_mut().rev().find(|message| {
        matches!(
            message,
            CanvasMessage::AgentTurn {
                streaming: true,
                ..
            }
        )
    }) {
        *reasoning_seen = true;
        reasoning_text.push_str(delta);
    }
}

pub(super) fn finalize_agent_reasoning(canvas: &mut CanvasModel, text: Option<&str>) {
    if let Some(CanvasMessage::AgentTurn {
        reasoning_seen,
        reasoning_text,
        ..
    }) = canvas.messages.iter_mut().rev().find(|message| {
        matches!(
            message,
            CanvasMessage::AgentTurn {
                streaming: true,
                ..
            }
        )
    }) {
        *reasoning_seen = true;
        if let Some(text) = text.filter(|value| !value.trim().is_empty()) {
            reasoning_text.clear();
            reasoning_text.push_str(text);
        }
    }
}

pub(super) fn append_agent_reasoning_summary_delta(canvas: &mut CanvasModel, delta: &str) {
    if delta.is_empty() {
        return;
    }
    if let Some(CanvasMessage::AgentTurn {
        reasoning_seen,
        reasoning_summary,
        ..
    }) = canvas.messages.iter_mut().rev().find(|message| {
        matches!(
            message,
            CanvasMessage::AgentTurn {
                streaming: true,
                ..
            }
        )
    }) {
        *reasoning_seen = true;
        reasoning_summary.push_str(delta);
    }
}

pub(super) fn finalize_agent_reasoning_summary(canvas: &mut CanvasModel, text: Option<&str>) {
    if let Some(CanvasMessage::AgentTurn {
        reasoning_seen,
        reasoning_summary,
        ..
    }) = canvas.messages.iter_mut().rev().find(|message| {
        matches!(
            message,
            CanvasMessage::AgentTurn {
                streaming: true,
                ..
            }
        )
    }) {
        *reasoning_seen = true;
        if let Some(text) = text.filter(|value| !value.trim().is_empty()) {
            reasoning_summary.clear();
            reasoning_summary.push_str(text);
        }
    }
}

pub(super) fn ingest_reasoning_item(canvas: &mut CanvasModel, item: &serde_json::Value) -> bool {
    let Some(item) = item.as_object() else {
        return false;
    };
    let Some(item_type) = item.get("type").and_then(serde_json::Value::as_str) else {
        return false;
    };
    if item_type != "reasoning" {
        return false;
    }

    if let Some(CanvasMessage::AgentTurn { reasoning_seen, .. }) =
        canvas.messages.iter_mut().rev().find(|message| {
            matches!(
                message,
                CanvasMessage::AgentTurn {
                    streaming: true,
                    ..
                }
            )
        })
    {
        *reasoning_seen = true;
    }

    if let Some(summary) = reasoning_summary_from_item(item) {
        finalize_agent_reasoning_summary(canvas, Some(summary.as_str()));
    }
    if let Some(content) = reasoning_content_from_item(item) {
        finalize_agent_reasoning(canvas, Some(content.as_str()));
    }
    true
}

fn reasoning_summary_from_item(
    item: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let summary = item.get("summary")?.as_array()?;
    join_reasoning_text_parts(summary, &["summary_text"])
}

fn reasoning_content_from_item(
    item: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let content = item.get("content")?.as_array()?;
    join_reasoning_text_parts(content, &["output_text", "reasoning_text"])
}

fn join_reasoning_text_parts(
    parts: &[serde_json::Value],
    allowed_types: &[&str],
) -> Option<String> {
    let mut out = Vec::new();
    for part in parts {
        let Some(part) = part.as_object() else {
            continue;
        };
        let Some(part_type) = part.get("type").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if !allowed_types.contains(&part_type) {
            continue;
        }
        let Some(text) = part.get("text").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        out.push(trimmed.to_string());
    }

    (!out.is_empty()).then(|| out.join("\n"))
}

pub(super) fn finalize_agent_turn(canvas: &mut CanvasModel, now_ms: u64) {
    for message in canvas.messages.iter_mut().rev() {
        let CanvasMessage::AgentTurn {
            streaming,
            streaming_tail,
            streaming_collector,
            ended_at_ms,
            blocks,
            ..
        } = message
        else {
            continue;
        };
        if !*streaming {
            continue;
        }
        blocks.extend(streaming_collector.finalize(streaming_tail));
        *streaming = false;
        *ended_at_ms = Some(now_ms);
        return;
    }
}
