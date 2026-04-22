//! Key/mouse/vim event handling for the fullscreen TUI.
//!
//! Everything that turns a crossterm `TermEvent` into a `UiAction`
//! lives under this module: the top-level dispatch (`handle_term_event`,
//! here), mouse geometry (`mouse`), the keymap-driven keyboard pipe
//! (`keyboard`), and the D.5 vim-mode intercept (`vim`).
//!
//! `UiAction` is the single enum the driver's run loop pattern-matches
//! on. It is deliberately coarse-grained — every palette mode, overlay
//! recovery action, and scroll primitive folds into one of its variants
//! so the loop body stays readable.
//!
//! Shared editor / canvas helpers (`buffer_is_effectively_empty`,
//! `buffer_trimmed_prompt`, `move_selected`, `card_expand_target`,
//! `last_user_prompt`) live here because they are called from both the
//! submodule handlers and the SSE run loop in the parent `fullscreen`
//! module.

mod keyboard;
mod mouse;
mod vim;

use crossterm::event::Event as TermEvent;
use ratatui_textarea::TextArea;
use rip_tui::{RenderMode, TuiState};

use super::keymap::Keymap;

pub(super) use keyboard::handle_key_event;
pub(super) use mouse::handle_mouse_event;
#[cfg(test)]
pub(super) use mouse::{mouse_canvas_hit_geometry, mouse_footer_activity_row};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UiAction {
    None,
    Quit,
    CancelSession,
    Submit,
    CloseOverlay,
    /// Primary palette trigger — `⌃K` opens the Command palette
    /// (Phase C.5). Backward-compat: when the operator has a
    /// `C-k → TogglePalette` binding in `~/.rip/keybindings.json`,
    /// that still opens a palette; the driver now routes it to the
    /// Command mode instead of straight to Models.
    TogglePalette,
    /// `⌃M` / `Alt+M` → Models palette directly.
    OpenPaletteModels,
    /// `⌃G` → Go To palette.
    OpenPaletteGoTo,
    /// `⌃T` → Threads palette.
    OpenPaletteThreads,
    /// `Alt+O` → Options palette.
    OpenPaletteOptions,
    /// `?` → Help overlay (Phase C.7).
    ShowHelp,
    /// `Tab` inside an open palette cycles through modes in order
    /// Command → Models → Go To → Threads → Options → Command…
    /// Outside the palette this is a no-op (the legacy details-mode
    /// toggle is retired per the plan).
    PaletteCycleMode,
    ApplyPalette,
    ApplyThreadPicker,
    ToggleActivity,
    ToggleTasks,
    OpenSelectedDetail,
    OpenFocusedDetail,
    ExpandFocusedCard,
    CopySelected,
    ScrollCanvasTop,
    ScrollCanvasBottom,
    CompactionAuto,
    CompactionAutoSchedule,
    CompactionCutPoints,
    CompactionStatus,
    ProviderCursorStatus,
    ProviderCursorRotate,
    ContextSelectionStatus,
    ScrollCanvasUp,
    ScrollCanvasDown,
    /// C.10 error-recovery actions. Routed through capabilities —
    /// none of them reach disk or the event log directly.
    /// `r` re-posts the last user message (kernel spawns the retry
    /// run per the capability contract).
    ErrorRecoveryRetry,
    /// `c` rotates the provider cursor.
    ErrorRecoveryRotateCursor,
    /// `m` opens the Models palette so the operator can switch
    /// before retrying.
    ErrorRecoverySwitchModel,
    /// `x` opens the X-ray window scoped to this error's seq (for
    /// now it routes into the existing `ErrorDetail` overlay —
    /// a Phase D follow-up widens it to a proper `XrayOverlay`).
    ErrorRecoveryXray,
}

pub(super) fn handle_term_event(
    event: TermEvent,
    state: &mut TuiState,
    mode: &mut RenderMode,
    input: &mut TextArea<'static>,
    session_running: bool,
    keymap: &Keymap,
) -> UiAction {
    match event {
        TermEvent::Key(key) => handle_key_event(key, state, mode, input, session_running, keymap),
        TermEvent::Paste(text) => {
            if state.is_palette_open() {
                for ch in text.chars() {
                    state.palette_push_char(ch);
                }
            } else if !session_running && matches!(state.overlay(), rip_tui::Overlay::None) {
                input.insert_str(&text);
            }
            UiAction::None
        }
        TermEvent::Mouse(m) => handle_mouse_event(m, state, input),
        TermEvent::Resize(_, _) => UiAction::None,
        _ => UiAction::None,
    }
}

/// Whitespace-only buffer counts as empty for submit / expand-card
/// gating — matches the keylight / placeholder rule in the renderer.
/// Using `TextArea::is_empty` alone would flip to "typing" as soon as
/// the user pressed space, which would swap the keylight mid-pause
/// and let Enter submit an all-whitespace prompt.
pub(super) fn buffer_is_effectively_empty(input: &TextArea<'_>) -> bool {
    input.lines().iter().all(|line| line.trim().is_empty())
}

/// Flatten the textarea's lines back into a `\n`-joined prompt and
/// trim surrounding whitespace. Used whenever we need the user's
/// typed input as a single `String` (sending to the kernel, copying,
/// etc.).
pub(super) fn buffer_trimmed_prompt(input: &TextArea<'_>) -> String {
    input.lines().join("\n").trim().to_string()
}

pub(super) fn move_selected(state: &mut TuiState, delta: i64) {
    let Some(selected) = state.selected_seq else {
        state.selected_seq = state.frames.last_seq();
        return;
    };
    let next = if delta.is_negative() {
        selected.saturating_sub(delta.unsigned_abs())
    } else {
        selected.saturating_add(delta as u64)
    };
    let clamped = next
        .max(state.frames.first_seq().unwrap_or(next))
        .min(state.frames.last_seq().unwrap_or(next));
    state.selected_seq = Some(clamped);
}

/// `⏎` on a focused card expands it — but only if there's something to
/// expand onto. `true` when the focused message is a `ToolCard` or
/// `TaskCard`; `false` when it's a plain turn / notice or when focus is
/// empty (in which case submit falls through to its usual path).
pub(super) fn card_expand_target(state: &TuiState) -> bool {
    use rip_tui::CanvasMessage;
    matches!(
        state.focused_message(),
        Some(CanvasMessage::ToolCard { .. } | CanvasMessage::TaskCard { .. })
    )
}

/// C.10 — dig out the last user message's plain text from the canvas.
/// Used by `ErrorRecoveryRetry` to re-post the turn that triggered
/// the error. Returns `None` when the canvas has no UserTurn to
/// replay (fresh thread, or the user hasn't submitted anything yet).
pub(super) fn last_user_prompt(state: &TuiState) -> Option<String> {
    use rip_tui::canvas::{Block, CanvasMessage};
    let user_turn = state
        .canvas
        .messages
        .iter()
        .rev()
        .find_map(|msg| match msg {
            CanvasMessage::UserTurn { blocks, .. } => Some(blocks),
            _ => None,
        })?;
    for block in user_turn {
        let text = match block {
            Block::Paragraph(t)
            | Block::Markdown(t)
            | Block::Heading { text: t, .. }
            | Block::CodeFence { text: t, .. } => t,
            _ => continue,
        };
        let mut out = String::new();
        for (idx, line) in text.text.lines.iter().enumerate() {
            if idx > 0 {
                out.push('\n');
            }
            for span in &line.spans {
                out.push_str(&span.content);
            }
        }
        if !out.trim().is_empty() {
            return Some(out);
        }
    }
    None
}

#[cfg(test)]
mod tests;
