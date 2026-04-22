//! Keyboard event routing.
//!
//! Resolves a single `KeyEvent` to a `UiAction`, honoring the overlay
//! stack (ErrorRecovery claims `r/c/m/x/⎋` globally; palette + thread
//! picker each swallow their navigation keys); the D.5 vim layer
//! (intercepts before the keymap so Normal-mode letters don't fall
//! through to the global bindings); the user-configurable keymap
//! (`~/.rip/keybindings.json`, merged with defaults); the
//! Alt/Shift-Enter newline quirk (many terminals can't distinguish
//! Shift-Enter from plain Enter, so we accept both as the newline
//! modifier); and the textarea's own editing bindings.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::{Input, TextArea};
use rip_tui::{RenderMode, TuiState};

use super::super::keymap::{Command as KeyCommand, Keymap};
use super::vim::try_vim_intercept;
use super::{buffer_is_effectively_empty, card_expand_target, move_selected, UiAction};

fn editor_prefers_text_input(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char(_))
        && !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
}

pub(in crate::fullscreen) fn handle_key_event(
    key: KeyEvent,
    state: &mut TuiState,
    mode: &mut RenderMode,
    input: &mut TextArea<'static>,
    session_running: bool,
    keymap: &Keymap,
) -> UiAction {
    // C.10: ErrorRecovery owns the key stream while it's on top of
    // the overlay stack. `r/c/m/x` dispatch to capabilities; `⎋`
    // dismisses. We intercept here so recovery actions don't have
    // to be bound globally in the keymap.
    if let rip_tui::Overlay::ErrorRecovery { .. } = state.overlay() {
        return match key.code {
            KeyCode::Char('r') => UiAction::ErrorRecoveryRetry,
            KeyCode::Char('c') => UiAction::ErrorRecoveryRotateCursor,
            KeyCode::Char('m') => UiAction::ErrorRecoverySwitchModel,
            KeyCode::Char('x') => UiAction::ErrorRecoveryXray,
            KeyCode::Esc => UiAction::CloseOverlay,
            _ => UiAction::None,
        };
    }

    if state.is_thread_picker_open() {
        if let Some(cmd) = keymap.command_for(key) {
            return match cmd {
                KeyCommand::Quit => UiAction::Quit,
                KeyCommand::Submit => UiAction::ApplyThreadPicker,
                KeyCommand::CloseOverlay | KeyCommand::TogglePalette => UiAction::CloseOverlay,
                KeyCommand::SelectPrev => {
                    state.thread_picker_move_selection(-1);
                    UiAction::None
                }
                KeyCommand::SelectNext => {
                    state.thread_picker_move_selection(1);
                    UiAction::None
                }
                KeyCommand::ScrollCanvasUp => {
                    state.thread_picker_move_selection(-5);
                    UiAction::None
                }
                KeyCommand::ScrollCanvasDown => {
                    state.thread_picker_move_selection(5);
                    UiAction::None
                }
                _ => UiAction::None,
            };
        }
        return UiAction::None;
    }

    if state.is_palette_open() {
        if let Some(cmd) = keymap.command_for(key) {
            return match cmd {
                KeyCommand::Quit => UiAction::Quit,
                KeyCommand::Submit => UiAction::ApplyPalette,
                KeyCommand::CloseOverlay | KeyCommand::TogglePalette => UiAction::CloseOverlay,
                KeyCommand::PaletteCycleMode => UiAction::PaletteCycleMode,
                KeyCommand::SelectPrev => {
                    state.palette_move_selection(-1);
                    UiAction::None
                }
                KeyCommand::SelectNext => {
                    state.palette_move_selection(1);
                    UiAction::None
                }
                KeyCommand::ScrollCanvasUp => {
                    state.palette_move_selection(-5);
                    UiAction::None
                }
                KeyCommand::ScrollCanvasDown => {
                    state.palette_move_selection(5);
                    UiAction::None
                }
                KeyCommand::ToggleTheme => {
                    state.toggle_theme();
                    UiAction::None
                }
                _ => UiAction::None,
            };
        }

        return match key.code {
            KeyCode::Backspace => {
                state.palette_backspace();
                UiAction::None
            }
            KeyCode::Char(ch) => {
                state.palette_push_char(ch);
                UiAction::None
            }
            _ => UiAction::None,
        };
    }

    // D.5: vim layer gets first refusal on non-overlay keys, but only
    // when the session isn't streaming and no palette / overlay has
    // already claimed the input. Normal mode fully owns plain-keyed
    // input (letters, motions, Esc-as-no-op); Insert mode only takes
    // Esc so the textarea's emacs-ish bindings still work for typing.
    // We intercept BEFORE the global keymap consult so vim's Esc isn't
    // eaten by the keymap's default `Esc → CloseOverlay` binding, and
    // so Normal-mode letter keys can't fall through to bindings like
    // `x = ToggleOutputView` that would otherwise fire.
    if state.vim_input_mode && !session_running {
        if let Some(action) = try_vim_intercept(key, state, input) {
            return action;
        }
    }

    if !session_running && editor_prefers_text_input(key) {
        if matches!(key.code, KeyCode::Char('?')) && buffer_is_effectively_empty(input) {
            return UiAction::ShowHelp;
        }
        let _ = input.input(Input::from(key));
        return UiAction::None;
    }

    if let Some(cmd) = keymap.command_for(key) {
        return match cmd {
            KeyCommand::Quit => UiAction::Quit,
            KeyCommand::Submit => {
                // `⏎` is contextual per the revamp plan (Part 9.1): if a
                // tool/task card is focused and the input is empty, Enter
                // toggles expand on that card. Otherwise: submit (when
                // the editor is the focus) or open the detail overlay
                // for the selected frame (when a run is active).
                if buffer_is_effectively_empty(input) && card_expand_target(state) {
                    UiAction::ExpandFocusedCard
                } else if session_running {
                    UiAction::OpenSelectedDetail
                } else {
                    UiAction::Submit
                }
            }
            KeyCommand::CloseOverlay => UiAction::CloseOverlay,
            KeyCommand::TogglePalette => UiAction::TogglePalette,
            KeyCommand::PaletteModels => UiAction::OpenPaletteModels,
            KeyCommand::PaletteGoTo => UiAction::OpenPaletteGoTo,
            KeyCommand::PaletteThreads => UiAction::OpenPaletteThreads,
            KeyCommand::PaletteOptions => UiAction::OpenPaletteOptions,
            KeyCommand::ShowHelp => {
                if buffer_is_effectively_empty(input) {
                    UiAction::ShowHelp
                } else {
                    let _ = input.input(Input::from(key));
                    UiAction::None
                }
            }
            KeyCommand::PaletteCycleMode => UiAction::None,
            KeyCommand::ToggleActivity => UiAction::ToggleActivity,
            KeyCommand::ToggleTasks => UiAction::ToggleTasks,
            KeyCommand::FocusPrevMessage => {
                state.focus_prev_message();
                UiAction::None
            }
            KeyCommand::FocusNextMessage => {
                state.focus_next_message();
                UiAction::None
            }
            KeyCommand::FocusClear => {
                state.clear_focus();
                UiAction::None
            }
            KeyCommand::OpenFocusedDetail => UiAction::OpenFocusedDetail,
            KeyCommand::ToggleDetailsMode => {
                *mode = match mode {
                    RenderMode::Json => RenderMode::Decoded,
                    RenderMode::Decoded => RenderMode::Json,
                };
                UiAction::None
            }
            KeyCommand::ToggleFollow => {
                state.auto_follow = !state.auto_follow;
                UiAction::None
            }
            KeyCommand::ToggleOutputView => {
                state.toggle_output_view();
                UiAction::None
            }
            KeyCommand::ToggleTheme => {
                state.toggle_theme();
                UiAction::None
            }
            KeyCommand::CopySelected => UiAction::CopySelected,
            KeyCommand::SelectPrev => {
                state.auto_follow = false;
                move_selected(state, -1);
                UiAction::None
            }
            KeyCommand::SelectNext => {
                state.auto_follow = false;
                move_selected(state, 1);
                UiAction::None
            }
            KeyCommand::CompactionAuto => UiAction::CompactionAuto,
            KeyCommand::CompactionAutoSchedule => UiAction::CompactionAutoSchedule,
            KeyCommand::CompactionCutPoints => UiAction::CompactionCutPoints,
            KeyCommand::CompactionStatus => UiAction::CompactionStatus,
            KeyCommand::ProviderCursorStatus => UiAction::ProviderCursorStatus,
            KeyCommand::ProviderCursorRotate => UiAction::ProviderCursorRotate,
            KeyCommand::ContextSelectionStatus => UiAction::ContextSelectionStatus,
            KeyCommand::ScrollCanvasUp => UiAction::ScrollCanvasUp,
            KeyCommand::ScrollCanvasDown => UiAction::ScrollCanvasDown,
        };
    }

    if session_running {
        return UiAction::None;
    }

    // Alt-Enter / Shift-Enter inserts a newline (Part 7: "multi-line
    // with ⇧⏎ newline"). Alt is more reliable across terminals than
    // Shift-Enter, which many terminals don't distinguish from Enter;
    // accepting both is harmless and matches the keylight's advertised
    // `⇧⏎ newline` affordance. The textarea's own `Enter` handler
    // inserts a newline only when `input.input(...)` receives bare
    // Enter, so we intercept the modifier combo and splice manually —
    // bare Enter stays bound to `UiAction::Submit` via the keymap.
    if key.code == KeyCode::Enter
        && key
            .modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT)
    {
        input.insert_newline();
        return UiAction::None;
    }

    // Everything else the editor needs — Backspace, arrow keys,
    // Home/End, Ctrl-A/E (BOL/EOL), Ctrl-U (kill-bol), Ctrl-W
    // (kill-word), Char insertion — is already implemented by
    // ratatui-textarea. Rather than re-implementing cursor math over a
    // `String`, we hand the event off via `Input::from(key)` and let
    // the textarea drive its own buffer + cursor + undo history.
    let _ = input.input(Input::from(key));
    UiAction::None
}

#[cfg(test)]
mod tests;
