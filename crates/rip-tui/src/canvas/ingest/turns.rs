//! User + agent turn ingestion.
//!
//! Turns the `SessionStarted` / `OutputTextDelta` / `SessionEnded` triple
//! into `UserTurn` / `AgentTurn` messages. The streaming collector
//! re-homes here every delta — see `StreamCollector::from_tail` /
//! `into_tail` — because a single `CanvasMessage` can't own an enum-
//! variant-scoped collector without significant API churn, and the
//! rebuild is O(tail) in the worst case.

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
        blocks: Vec::new(),
        streaming_tail: String::new(),
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
            streaming,
            streaming_tail,
            ..
        } = message
        else {
            continue;
        };
        if !*streaming {
            continue;
        }
        let mut collector = StreamCollector::from_tail(std::mem::take(streaming_tail));
        let step = collector.push(delta);
        blocks.extend(step.new_stable);
        *streaming_tail = collector.into_tail();
        return;
    }
}

pub(super) fn finalize_agent_turn(canvas: &mut CanvasModel, now_ms: u64) {
    for message in canvas.messages.iter_mut().rev() {
        let CanvasMessage::AgentTurn {
            streaming,
            streaming_tail,
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
        let mut collector = StreamCollector::from_tail(std::mem::take(streaming_tail));
        blocks.extend(collector.finalize());
        *streaming_tail = String::new();
        *streaming = false;
        *ended_at_ms = Some(now_ms);
        return;
    }
}
