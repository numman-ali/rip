//! Key/mouse/vim event handling for the fullscreen TUI.
//!
//! Everything that turns a crossterm `TermEvent` into a `UiAction`
//! lives here: the top-level dispatch (`handle_term_event`), mouse
//! geometry (`handle_mouse_event` + `mouse_*` helpers), the keymap-
//! driven keyboard pipe (`handle_key_event`), and the D.5 vim-mode
//! intercept (`try_vim_intercept` + `handle_vim_normal_key`).
//!
//! `UiAction` is the single enum the driver's run loop pattern-matches
//! on. It is deliberately coarse-grained — every palette mode, overlay
//! recovery action, and scroll primitive folds into one of its variants
//! so the loop body stays readable.

use crossterm::event::{
    Event as TermEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::terminal::size as terminal_size;
use ratatui_textarea::{Input, TextArea};
use rip_tui::{canvas_hit_message_id, hero_click_target, HeroClickTarget, RenderMode, TuiState};

use super::keymap::{Command as KeyCommand, Keymap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UiAction {
    None,
    Quit,
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
        TermEvent::Mouse(mouse) => handle_mouse_event(mouse, state),
        TermEvent::Resize(_, _) => UiAction::None,
        _ => UiAction::None,
    }
}

pub(super) fn handle_mouse_event(mouse: MouseEvent, state: &mut TuiState) -> UiAction {
    if state.is_palette_open() {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                state.palette_move_selection(-1);
                return UiAction::None;
            }
            MouseEventKind::ScrollDown => {
                state.palette_move_selection(1);
                return UiAction::None;
            }
            _ => {}
        }
        return UiAction::None;
    }

    if state.is_thread_picker_open() {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                state.thread_picker_move_selection(-1);
                return UiAction::None;
            }
            MouseEventKind::ScrollDown => {
                state.thread_picker_move_selection(1);
                return UiAction::None;
            }
            _ => {}
        }
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return UiAction::ApplyThreadPicker;
        }
        return UiAction::None;
    }

    let (width, height) = match terminal_size() {
        Ok(size) => size,
        Err(_) => return UiAction::None,
    };

    if mouse.row == 0 && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return match hero_click_target(state, width, mouse.column) {
            Some(HeroClickTarget::Thread) => UiAction::OpenPaletteThreads,
            Some(HeroClickTarget::Agent) => UiAction::TogglePalette,
            Some(HeroClickTarget::Model) => UiAction::OpenPaletteModels,
            None => UiAction::None,
        };
    }

    if mouse_hits_activity_surface(state, width, height, mouse.column, mouse.row) {
        return match mouse.kind {
            MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown => {
                state.set_overlay(rip_tui::Overlay::Activity);
                UiAction::None
            }
            _ => UiAction::None,
        };
    }

    if matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
    ) {
        if let Some((viewport_width, viewport_height, row_in_canvas)) =
            mouse_canvas_hit_geometry(state, width, height, mouse.column, mouse.row)
        {
            if let Some(message_id) =
                canvas_hit_message_id(state, viewport_width, viewport_height, row_in_canvas)
            {
                state.focused_message_id = Some(message_id);
                state.auto_follow = false;
            }
            return UiAction::None;
        }
    }

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if state.output_view == rip_tui::OutputViewMode::Rendered {
                UiAction::ScrollCanvasUp
            } else {
                state.auto_follow = false;
                move_selected(state, -6);
                UiAction::None
            }
        }
        MouseEventKind::ScrollDown => {
            if state.output_view == rip_tui::OutputViewMode::Rendered {
                UiAction::ScrollCanvasDown
            } else {
                state.auto_follow = false;
                move_selected(state, 6);
                UiAction::None
            }
        }
        _ => UiAction::None,
    }
}

fn mouse_hits_activity_surface(
    state: &TuiState,
    width: u16,
    height: u16,
    column: u16,
    row: u16,
) -> bool {
    if state.activity_pinned && width >= 100 {
        let rail_width = 32u16;
        let rail_start = width.saturating_sub(rail_width);
        if column >= rail_start && row > 0 && row < height.saturating_sub(2) {
            return true;
        }
    }

    let Some(activity_row) = mouse_footer_activity_row(height) else {
        return false;
    };
    row == activity_row
}

pub(super) fn mouse_footer_activity_row(height: u16) -> Option<u16> {
    (height >= 4).then_some(height.saturating_sub(3))
}

pub(super) fn mouse_canvas_hit_geometry(
    state: &TuiState,
    width: u16,
    height: u16,
    column: u16,
    row: u16,
) -> Option<(u16, u16, u16)> {
    let body_top = 1u16;
    let bottom_reserved = 3u16;
    let body_height = height.saturating_sub(body_top + bottom_reserved);
    if body_height == 0 || row < body_top || row >= body_top.saturating_add(body_height) {
        return None;
    }

    let viewport_width = if state.activity_pinned && width >= 100 {
        let canvas_width = width.saturating_sub(32);
        if column >= canvas_width {
            return None;
        }
        canvas_width
    } else {
        width
    };

    Some((viewport_width, body_height, row.saturating_sub(body_top)))
}

pub(super) fn handle_key_event(
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
            KeyCommand::ShowHelp => UiAction::ShowHelp,
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

/// D.5: decides whether the vim layer owns a given key press. Returns
/// `Some` when the vim dispatcher has consumed it, `None` to let the
/// global keymap + textarea pipe continue. Normal mode claims all
/// non-Ctrl keys (letters, motions, Esc-as-no-op). Insert mode only
/// claims Esc so the ambient emacs-ish textarea bindings keep working
/// for actual typing, which is the whole point of making Insert mode
/// "the textarea's native mode" rather than an alternate keymap.
fn try_vim_intercept(
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

/// D.5: dispatcher for vim Normal-mode keys. Covers the subset named
/// in the revamp plan (Esc / i / a / o / dd / yy / p / gg / G) plus
/// enough cursor motion (h/j/k/l, w/b, 0/$) and edit primitives (x, A,
/// I, O) to make Normal mode actually usable. Anything we don't
/// implement is silently swallowed rather than handed to the textarea
/// — that way the user can't accidentally type text into the buffer
/// while they think they're in Normal. The `vim_pending` field on
/// `TuiState` tracks the waiting-for-second-char state for `dd`, `yy`,
/// and `gg`; every path through this function must either set it or
/// clear it so a stale prefix can't survive a completed action.
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
fn card_expand_target(state: &TuiState) -> bool {
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
