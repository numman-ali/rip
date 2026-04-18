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
}
