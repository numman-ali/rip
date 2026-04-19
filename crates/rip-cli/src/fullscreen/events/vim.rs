//! D.5 vim input mode.
//!
//! Normal / Insert state machine over `ratatui-textarea`. Normal mode
//! owns plain-keyed input (letters, motions, Esc-as-no-op); Insert mode
//! is the textarea's native mode — we only intercept Esc so the ambient
//! emacs-ish bindings keep working for actual typing.
//!
//! The subset named in the revamp plan lands here: `i / a / I / A / o /
//! O / dd / yy / p / gg / G` plus enough cursor motion (`h j k l`,
//! `w b e`, `0 $`) and edit primitives (`x`, `u`) to make Normal mode
//! actually usable. Anything we don't implement is silently swallowed
//! rather than handed to the textarea, so the user can't accidentally
//! type text into the buffer while they think they're in Normal. The
//! `vim_pending` field on `TuiState` tracks the waiting-for-second-char
//! state for `dd`, `yy`, and `gg`; every path through the dispatcher
//! must either set it or clear it so a stale prefix can't survive a
//! completed action.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::TextArea;
use rip_tui::TuiState;

use super::UiAction;

/// Decides whether the vim layer owns a given key press. Returns
/// `Some` when the vim dispatcher has consumed it, `None` to let the
/// global keymap + textarea pipe continue. Normal mode claims all
/// non-Ctrl keys (letters, motions, Esc-as-no-op). Insert mode only
/// claims Esc.
pub(super) fn try_vim_intercept(
    key: KeyEvent,
    state: &mut TuiState,
    input: &mut TextArea<'static>,
) -> Option<UiAction> {
    match state.vim_mode {
        rip_tui::VimMode::Insert => {
            if key.code == KeyCode::Esc && key.modifiers.is_empty() {
                state.vim_mode = rip_tui::VimMode::Normal;
                state.vim_pending = None;
                return Some(UiAction::None);
            }
            None
        }
        rip_tui::VimMode::Normal => {
            // Ctrl-modified keys remain available to the outer keymap
            // so Ctrl-C / Ctrl-K / etc. keep working in Normal mode.
            // Shift is allowed through — `A`, `I`, `O`, `G`, `$` all
            // need it, and vim treats shifted letters as first-class
            // operators rather than as chord prefixes.
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT)
            {
                return None;
            }
            match key.code {
                KeyCode::Char(_)
                | KeyCode::Esc
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::Backspace => Some(handle_vim_normal_key(key, state, input)),
                // Enter / Tab / function keys / everything else stays
                // on the global keymap path — vim's own `:` command-
                // line handling is out of scope, and Submit should
                // still feel like Submit.
                _ => None,
            }
        }
    }
}

fn handle_vim_normal_key(
    key: KeyEvent,
    state: &mut TuiState,
    input: &mut TextArea<'static>,
) -> UiAction {
    use ratatui_textarea::CursorMove;

    let pending = state.vim_pending.take();

    let ch = match key.code {
        KeyCode::Char(c) => c,
        KeyCode::Esc => {
            state.vim_pending = None;
            return UiAction::None;
        }
        KeyCode::Up => {
            input.move_cursor(CursorMove::Up);
            return UiAction::None;
        }
        KeyCode::Down => {
            input.move_cursor(CursorMove::Down);
            return UiAction::None;
        }
        KeyCode::Left => {
            input.move_cursor(CursorMove::Back);
            return UiAction::None;
        }
        KeyCode::Right => {
            input.move_cursor(CursorMove::Forward);
            return UiAction::None;
        }
        KeyCode::Home => {
            input.move_cursor(CursorMove::Head);
            return UiAction::None;
        }
        KeyCode::End => {
            input.move_cursor(CursorMove::End);
            return UiAction::None;
        }
        KeyCode::Backspace => {
            // Vim's Backspace in Normal mode is "move cursor left" —
            // it never deletes. This matches the textarea only after
            // we opt out of `Input::from(key)`'s delete behaviour.
            input.move_cursor(CursorMove::Back);
            return UiAction::None;
        }
        _ => return UiAction::None,
    };

    if let Some(prefix) = pending {
        match (prefix, ch) {
            ('d', 'd') => {
                input.move_cursor(CursorMove::Head);
                input.start_selection();
                input.move_cursor(CursorMove::End);
                let _ = input.cut();
                // Leave the now-empty line behind so `p` pastes on the
                // blank line — matches Vim's `dd` leaving a blank when
                // it's the only line in the buffer. Multi-line buffers
                // get the followup newline swallowed too so the cursor
                // lands on the next logical line.
                input.delete_next_char();
                return UiAction::None;
            }
            ('y', 'y') => {
                input.start_selection();
                input.move_cursor(CursorMove::Head);
                input.start_selection();
                input.move_cursor(CursorMove::End);
                input.copy();
                input.cancel_selection();
                return UiAction::None;
            }
            ('g', 'g') => {
                input.move_cursor(CursorMove::Top);
                return UiAction::None;
            }
            _ => {
                // Unmatched follow-up: fall through so `ch` is
                // interpreted as a fresh Normal-mode key rather than
                // the second half of an operator.
            }
        }
    }

    match ch {
        'i' => state.vim_mode = rip_tui::VimMode::Insert,
        'a' => {
            input.move_cursor(CursorMove::Forward);
            state.vim_mode = rip_tui::VimMode::Insert;
        }
        'I' => {
            input.move_cursor(CursorMove::Head);
            state.vim_mode = rip_tui::VimMode::Insert;
        }
        'A' => {
            input.move_cursor(CursorMove::End);
            state.vim_mode = rip_tui::VimMode::Insert;
        }
        'o' => {
            input.move_cursor(CursorMove::End);
            input.insert_newline();
            state.vim_mode = rip_tui::VimMode::Insert;
        }
        'O' => {
            input.move_cursor(CursorMove::Head);
            input.insert_newline();
            input.move_cursor(CursorMove::Up);
            state.vim_mode = rip_tui::VimMode::Insert;
        }
        'h' => input.move_cursor(CursorMove::Back),
        'l' => input.move_cursor(CursorMove::Forward),
        'j' => input.move_cursor(CursorMove::Down),
        'k' => input.move_cursor(CursorMove::Up),
        'w' => input.move_cursor(CursorMove::WordForward),
        'b' => input.move_cursor(CursorMove::WordBack),
        'e' => input.move_cursor(CursorMove::WordEnd),
        '0' => input.move_cursor(CursorMove::Head),
        '$' => input.move_cursor(CursorMove::End),
        'G' => input.move_cursor(CursorMove::Bottom),
        'x' => {
            input.delete_next_char();
        }
        'p' => {
            input.paste();
        }
        'u' => {
            input.undo();
        }
        'd' | 'y' | 'g' => {
            state.vim_pending = Some(ch);
        }
        _ => {}
    }
    UiAction::None
}
