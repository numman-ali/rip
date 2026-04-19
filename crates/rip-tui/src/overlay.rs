//! Overlay trait + stack infrastructure (Phase A.3 of the TUI revamp).
//!
//! The stack model replaces the "single active overlay" field with an
//! ordered collection. Input routing and focus (Phase C) will consume
//! the top of the stack; for now, render continues to display one
//! overlay at a time and the stack is effectively 0/1 elements wide.
//!
//! The trait intentionally starts metadata-only (id/title) — render /
//! on_key / on_mouse land when Phase C stands up the full `Overlay`
//! contract from Part 6.1 of the plan. Keeping the trait narrow now
//! avoids churn while still giving later phases a single seam to
//! extend.

use crate::Overlay;

/// Metadata for a TUI overlay.
///
/// Implemented for the `Overlay` enum so future extensions can plug in
/// bespoke overlay types (Help, ArtifactViewer, ThreadPicker, …) as
/// plain structs without widening the enum.
pub trait OverlayView {
    fn id(&self) -> &'static str;
    fn title(&self) -> &str;
}

impl OverlayView for Overlay {
    fn id(&self) -> &'static str {
        match self {
            Overlay::None => "none",
            Overlay::Activity => "activity",
            Overlay::Palette(_) => "palette",
            Overlay::TaskList => "task_list",
            Overlay::ToolDetail { .. } => "tool_detail",
            Overlay::TaskDetail { .. } => "task_detail",
            Overlay::ErrorDetail { .. } => "error_detail",
            Overlay::StallDetail => "stall_detail",
            Overlay::Debug => "debug",
        }
    }

    fn title(&self) -> &str {
        match self {
            Overlay::None => "",
            Overlay::Activity => "Activity",
            Overlay::Palette(palette) => palette.mode.label(),
            Overlay::TaskList => "Tasks",
            Overlay::ToolDetail { .. } => "Tool Detail",
            Overlay::TaskDetail { .. } => "Task Detail",
            Overlay::ErrorDetail { .. } => "Error Detail",
            Overlay::StallDetail => "Stalled",
            Overlay::Debug => "Debug",
        }
    }
}

/// Ordered stack of overlays. The top is what renders and receives
/// input; popping reveals whatever was summoned under it. Phase A
/// treats the stack as effectively 0/1 deep — single overlay at a
/// time. Phase C widens usage (palette over error, help over palette,
/// etc.).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlayStack {
    items: Vec<Overlay>,
}

impl OverlayStack {
    const NO_OVERLAY: Overlay = Overlay::None;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn top(&self) -> &Overlay {
        self.items.last().unwrap_or(&Self::NO_OVERLAY)
    }

    pub fn top_mut(&mut self) -> Option<&mut Overlay> {
        self.items.last_mut()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Replace the stack with `overlay`. Passing `Overlay::None` clears
    /// the stack entirely — matching the legacy "assign the None
    /// variant to dismiss" contract.
    pub fn set(&mut self, overlay: Overlay) {
        self.items.clear();
        if !matches!(overlay, Overlay::None) {
            self.items.push(overlay);
        }
    }

    pub fn push(&mut self, overlay: Overlay) {
        if matches!(overlay, Overlay::None) {
            return;
        }
        self.items.push(overlay);
    }

    pub fn pop(&mut self) -> Option<Overlay> {
        self.items.pop()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}
