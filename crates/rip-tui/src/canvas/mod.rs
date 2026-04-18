//! Structured canvas model (Phase B of the TUI revamp).
//!
//! [`CanvasModel`] owns the append-only list of [`CanvasMessage`]s that the
//! renderer walks in B.2+. B.1 populates it alongside the old `output_text`;
//! B.2 deletes the string path.

mod ingest;
pub mod model;

pub use model::*;

use rip_kernel::Event;

#[derive(Debug, Clone, Default)]
pub struct CanvasModel {
    pub messages: Vec<CanvasMessage>,
    next_message_id: u64,
}

impl CanvasModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.next_message_id = 0;
    }

    pub fn ingest(&mut self, event: &Event) {
        ingest::apply(self, event);
    }

    pub(crate) fn mint_id(&mut self) -> String {
        let id = self.next_message_id;
        self.next_message_id += 1;
        format!("m{id:06}")
    }

    /// Append a surface-side `UserTurn` before the run starts. Used by
    /// `TuiState::begin_pending_turn` so a just-submitted prompt lands on
    /// the canvas immediately instead of waiting for the `SessionStarted`
    /// frame to come back.
    pub fn push_user_turn(
        &mut self,
        actor_id: impl Into<String>,
        origin: impl Into<String>,
        text: &str,
        submitted_at_ms: u64,
    ) -> String {
        let id = self.mint_id();
        self.messages.push(CanvasMessage::UserTurn {
            message_id: id.clone(),
            actor_id: actor_id.into(),
            origin: origin.into(),
            blocks: vec![Block::Paragraph(CachedText::plain(text))],
            submitted_at_ms,
        });
        id
    }

    /// Toggle the `expanded` flag on a tool/task card. Returns true if the
    /// target was a card (and was toggled). Non-cards are ignored so callers
    /// can call this unconditionally from a focus handler.
    pub fn toggle_card_expanded(&mut self, message_id: &str) -> bool {
        for message in self.messages.iter_mut() {
            if message.message_id() != message_id {
                continue;
            }
            return match message {
                CanvasMessage::ToolCard { expanded, .. }
                | CanvasMessage::TaskCard { expanded, .. } => {
                    *expanded = !*expanded;
                    true
                }
                _ => false,
            };
        }
        false
    }

    /// Seq range that backs a canvas message — used by X-ray and per-item
    /// overlays to scope the timeline they show. `ToolCard`/`TaskCard` are
    /// anchored on their spawn frame and extend to the tail; other messages
    /// collapse to a single-seq range. Messages without a known anchor
    /// return `None` so the caller can fall back to "whole stream".
    pub fn seq_range_for(&self, message_id: &str) -> Option<(u64, u64)> {
        for message in &self.messages {
            if message.message_id() != message_id {
                continue;
            }
            return match message {
                CanvasMessage::ToolCard { started_seq, .. } => Some((*started_seq, u64::MAX)),
                CanvasMessage::SystemNotice { seq, .. } => Some((*seq, *seq)),
                CanvasMessage::CompactionCheckpoint {
                    from_seq, to_seq, ..
                } => Some((*from_seq, *to_seq)),
                _ => None,
            };
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mint_id_returns_monotonically_increasing_ids() {
        let mut canvas = CanvasModel::new();
        let a = canvas.mint_id();
        let b = canvas.mint_id();
        assert_ne!(a, b);
        assert!(b > a);
    }

    #[test]
    fn clear_resets_messages_and_ids() {
        let mut canvas = CanvasModel::new();
        canvas.push_user_turn("user", "tui", "hi", 100);
        canvas.clear();
        assert!(canvas.messages.is_empty());
        // Cleared canvases start fresh at id 0 so snapshots don't carry
        // stale message-ids across turns.
        assert_eq!(canvas.next_message_id, 0);
    }

    #[test]
    fn toggle_card_expanded_flips_tool_and_task_cards_and_ignores_others() {
        let mut canvas = CanvasModel::new();
        canvas.messages.push(CanvasMessage::ToolCard {
            message_id: "mt".into(),
            tool_id: "t1".into(),
            tool_name: "write".into(),
            args_block: Block::Paragraph(CachedText::empty()),
            status: ToolCardStatus::Running,
            body: Vec::new(),
            expanded: false,
            artifact_ids: Vec::new(),
            started_seq: 0,
            started_at_ms: 0,
        });
        assert!(canvas.toggle_card_expanded("mt"));
        let expanded = match &canvas.messages[0] {
            CanvasMessage::ToolCard { expanded, .. } => *expanded,
            _ => unreachable!(),
        };
        assert!(expanded);

        // Non-card messages short-circuit to false and stay untouched.
        canvas.messages.push(CanvasMessage::SystemNotice {
            message_id: "mn".into(),
            level: NoticeLevel::Info,
            text: "hi".into(),
            origin_event_kind: "x".into(),
            seq: 0,
        });
        assert!(!canvas.toggle_card_expanded("mn"));
        // Unknown ids return false.
        assert!(!canvas.toggle_card_expanded("missing"));
    }

    #[test]
    fn seq_range_for_anchors_tool_cards_at_started_seq_and_collapses_notices() {
        let mut canvas = CanvasModel::new();
        canvas.messages.push(CanvasMessage::ToolCard {
            message_id: "mt".into(),
            tool_id: "t1".into(),
            tool_name: "write".into(),
            args_block: Block::Paragraph(CachedText::empty()),
            status: ToolCardStatus::Running,
            body: Vec::new(),
            expanded: false,
            artifact_ids: Vec::new(),
            started_seq: 12,
            started_at_ms: 0,
        });
        canvas.messages.push(CanvasMessage::SystemNotice {
            message_id: "mn".into(),
            level: NoticeLevel::Danger,
            text: "bad".into(),
            origin_event_kind: "provider_event".into(),
            seq: 33,
        });
        assert_eq!(canvas.seq_range_for("mt"), Some((12, u64::MAX)));
        assert_eq!(canvas.seq_range_for("mn"), Some((33, 33)));
        assert_eq!(canvas.seq_range_for("missing"), None);
    }
}
