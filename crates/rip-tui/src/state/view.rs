//! UI-view enums.
//!
//! `Overlay` enumerates the exclusive overlay slot the driver pushes on
//! top of the base canvas; `OutputViewMode`, `ThemeId`, and `VimMode`
//! are small per-preference toggles that the driver flips through
//! palette actions. None of these carry continuity truth — they are
//! ephemeral UI state that reloads from `~/.rip/state/tui.json` (or
//! defaults) on startup.

use super::{PaletteState, ThreadPickerState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    None,
    Activity,
    Palette(PaletteState),
    ThreadPicker(ThreadPickerState),
    ToolDetail {
        tool_id: String,
    },
    TaskList,
    TaskDetail {
        task_id: String,
    },
    ErrorDetail {
        seq: u64,
    },
    StallDetail,
    /// C.1: debug tokens previously shown in the status bar (session id,
    /// last seq, handshake/first-byte/event timings, tool/task/job
    /// counters, endpoint, model, theme). Opened from `Command → Show
    /// debug info` in Phase C.5's palette; for now it's reachable via
    /// `set_overlay(Overlay::Debug)` and surfaced in a dedicated
    /// snapshot.
    Debug,
    /// C.7 Help overlay — a searchable keybinding + command reference.
    /// Opened with `?` from the input when empty; closed with `⎋`.
    /// Renders from `CommandAction` metadata (category + title +
    /// bound shortcut) so any new palette entry is automatically
    /// discoverable through Help.
    Help,
    /// C.10 In-UI provider-error recovery. Auto-opens on the first
    /// provider-error frame for a run; carries the frame `seq` for
    /// X-ray linkage. Overlay actions route through capabilities:
    /// `r` → `thread.post_message` (retry last user turn), `c` →
    /// `thread.provider_cursor.rotate`, `m` → Models palette, `x` →
    /// X-ray window scoped to the error, `⎋` → dismiss.
    ErrorRecovery {
        seq: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputViewMode {
    Rendered,
    Raw,
}

impl OutputViewMode {
    pub fn toggle(&mut self) {
        *self = match self {
            Self::Rendered => Self::Raw,
            Self::Raw => Self::Rendered,
        };
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rendered => "rendered",
            Self::Raw => "raw",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    DefaultDark,
    DefaultLight,
}

impl ThemeId {
    pub fn toggle(&mut self) {
        *self = match self {
            Self::DefaultDark => Self::DefaultLight,
            Self::DefaultLight => Self::DefaultDark,
        };
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::DefaultDark => "default-dark",
            Self::DefaultLight => "default-light",
        }
    }
}

/// Vim-style editor mode (D.5). Only consulted when
/// `TuiState::vim_input_mode` is true; otherwise the textarea is
/// driven directly by `Input::from(key)` with no mode layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VimMode {
    Normal,
    #[default]
    Insert,
}

impl VimMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
        }
    }
}
