use super::*;
use crate::test_env;
use crossterm::event::{
    Event as TermEvent, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::terminal::size as terminal_size;
use httpmock::Method::GET;
use httpmock::MockServer;
use rip_kernel::{EventKind, ProviderEventStatus};
use rip_tui::palette::modes::models::{
    default_endpoint_for_provider, infer_provider_id_from_endpoint, parse_model_route,
    push_route_from_string, upsert_model_route,
};
use rip_tui::{canvas_hit_message_id, hero_click_target, HeroClickTarget};
use rip_tui::{ModelRoute, ModelsMode, PaletteSource};
use std::collections::BTreeMap;
use std::ffi::OsString;
use tokio::time::timeout;

fn seed_state() -> TuiState {
    let mut state = TuiState::new(100);
    state.update(FrameEvent {
        id: "e0".to_string(),
        session_id: "s1".to_string(),
        timestamp_ms: 0,
        seq: 0,
        kind: EventKind::SessionStarted {
            input: "hi".to_string(),
        },
    });
    state.update(FrameEvent {
        id: "e1".to_string(),
        session_id: "s1".to_string(),
        timestamp_ms: 0,
        seq: 1,
        kind: EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Done,
            event_name: None,
            data: None,
            raw: None,
            errors: Vec::new(),
            response_errors: Vec::new(),
        },
    });
    state.update(FrameEvent {
        id: "e2".to_string(),
        session_id: "s1".to_string(),
        timestamp_ms: 0,
        seq: 2,
        kind: EventKind::SessionEnded {
            reason: "done".to_string(),
        },
    });
    state
}

#[test]
fn parse_theme_accepts_known_values() {
    assert_eq!(
        parse_theme("default-dark").unwrap(),
        Some(rip_tui::ThemeId::DefaultDark)
    );
    assert_eq!(
        parse_theme("light").unwrap(),
        Some(rip_tui::ThemeId::DefaultLight)
    );
    assert!(parse_theme("nope").is_err());
}

#[test]
fn parse_theme_empty_returns_none() {
    assert_eq!(parse_theme("   ").unwrap(), None);
}

#[test]
fn osc52_sequence_wraps_base64_payload() {
    let seq = osc52_sequence(b"hi");
    assert!(seq.starts_with("\u{1b}]52;c;"));
    assert!(seq.ends_with('\u{7}'));
    assert!(seq.contains("aGk="));
}

#[test]
fn base64_encode_matches_known_vectors() {
    assert_eq!(base64_encode(b""), "");
    assert_eq!(base64_encode(b"hi"), "aGk=");
    assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
}

#[test]
fn handle_key_event_applies_keymap_commands() {
    let keymap = Keymap::default();
    let mut state = seed_state();
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();

    // Up selects previous event.
    assert_eq!(state.selected_seq, Some(2));
    let action = handle_key_event(
        KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        true,
        &keymap,
    );
    assert_eq!(action, UiAction::None);
    assert_eq!(state.selected_seq, Some(1));

    // Phase C.8 reassigns `Ctrl-R` from "toggle raw global view"
    // to "open X-ray on focused item". The X-ray overlay is a
    // per-item drill-down, not a canvas-wide mode swap.
    assert_eq!(state.output_view, rip_tui::OutputViewMode::Rendered);
    let action = handle_key_event(
        KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
        &mut state,
        &mut mode,
        &mut input,
        true,
        &keymap,
    );
    assert_eq!(action, UiAction::OpenFocusedDetail);
    // Global output_view is unchanged — no more mode swap.
    assert_eq!(state.output_view, rip_tui::OutputViewMode::Rendered);

    // Ctrl+Y triggers copy.
    let action = handle_key_event(
        KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
        &mut state,
        &mut mode,
        &mut input,
        true,
        &keymap,
    );
    assert_eq!(action, UiAction::CopySelected);

    // Enter submits only when not running.
    let action = handle_key_event(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        true,
        &keymap,
    );
    assert_eq!(action, UiAction::OpenSelectedDetail);

    let action = handle_key_event(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(action, UiAction::Submit);

    let action = handle_key_event(
        KeyEvent::new(KeyCode::PageUp, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        true,
        &keymap,
    );
    assert_eq!(action, UiAction::ScrollCanvasUp);
}

#[test]
fn handle_key_event_routes_palette_input_and_selection() {
    let keymap = Keymap::default();
    let mut state = seed_state();
    state.open_palette(
        rip_tui::PaletteMode::Model,
        rip_tui::PaletteOrigin::TopRight,
        vec![
            rip_tui::PaletteEntry {
                value: "openrouter/openai/gpt-oss-20b".to_string(),
                title: "openrouter/openai/gpt-oss-20b".to_string(),
                subtitle: Some("OpenRouter".to_string()),
                chips: vec!["current".to_string()],
            },
            rip_tui::PaletteEntry {
                value: "openai/gpt-5-nano-2025-08-07".to_string(),
                title: "openai/gpt-5-nano-2025-08-07".to_string(),
                subtitle: Some("OpenAI".to_string()),
                chips: vec![],
            },
        ],
        "No models",
        true,
        "Use typed route",
    );
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();

    let action = handle_key_event(
        KeyEvent::new(KeyCode::Char('n'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        true,
        &keymap,
    );
    assert_eq!(action, UiAction::None);
    assert_eq!(state.palette_query(), Some("n"));

    let action = handle_key_event(
        KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        true,
        &keymap,
    );
    assert_eq!(action, UiAction::None);
    assert_eq!(
        state.palette_selected_value().as_deref(),
        Some("openai/gpt-5-nano-2025-08-07")
    );

    let action = handle_key_event(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        true,
        &keymap,
    );
    assert_eq!(action, UiAction::ApplyPalette);

    let action = handle_key_event(
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        &mut state,
        &mut mode,
        &mut input,
        true,
        &keymap,
    );
    assert_eq!(action, UiAction::CloseOverlay);
}

#[test]
fn handle_key_event_inserts_text_when_idle() {
    let keymap = Keymap::default();
    let mut state = seed_state();
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();

    let action = handle_key_event(
        KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(action, UiAction::None);
    assert_eq!(input.lines(), &["a".to_string()]);

    let action = handle_key_event(
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(action, UiAction::None);
    assert_eq!(input.lines(), &[String::new()]);
}

#[test]
fn handle_key_event_alt_enter_inserts_newline() {
    // C.4 multi-line input: ⌥⏎ / ⇧⏎ sends a `\n` instead of
    // submitting the turn.
    let keymap = Keymap::default();
    let mut state = seed_state();
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();
    input.insert_str("first");

    let action = handle_key_event(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(action, UiAction::None);
    assert_eq!(input.lines(), &["first".to_string(), String::new()]);

    let action = handle_key_event(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(action, UiAction::None);
    assert_eq!(
        input.lines(),
        &["first".to_string(), String::new(), String::new()]
    );
}

#[test]
fn handle_key_event_routes_editor_keys_to_textarea() {
    // After C.4 the driver hands raw key events to ratatui-textarea
    // via `Input::from(key)`. We only assert that the pipe is wired
    // up end-to-end — the specific binding set is the textarea's to
    // define, not ours to duplicate. A Char insert and a Backspace
    // are enough to prove the plumbing without coupling the test to
    // ratatui-textarea's default shortcut table.
    let keymap = Keymap::default();
    let mut state = seed_state();
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();

    handle_key_event(
        KeyEvent::new(KeyCode::Char('h'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    handle_key_event(
        KeyEvent::new(KeyCode::Char('i'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.lines(), &["hi".to_string()]);

    handle_key_event(
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.lines(), &["h".to_string()]);
}

#[test]
fn vim_mode_off_passes_every_key_through_to_textarea() {
    // D.5: the vim layer must stay fully out of the way when the
    // Options toggle is off — pressing `i` or `Esc` while typing
    // should not flip any mode, should not eat the key, and should
    // leave the buffer in the same state it would be in without
    // the layer wired at all.
    let keymap = Keymap::default();
    let mut state = seed_state();
    assert!(!state.vim_input_mode);
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();

    for ch in "hi".chars() {
        handle_key_event(
            KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty()),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
    }
    handle_key_event(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.lines(), &["hi".to_string()]);
    assert_eq!(state.vim_mode, rip_tui::VimMode::Insert);
}

#[test]
fn vim_mode_esc_in_insert_switches_to_normal_and_i_returns_to_insert() {
    let keymap = Keymap::default();
    let mut state = seed_state();
    state.vim_input_mode = true;
    state.vim_mode = rip_tui::VimMode::Insert;
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();
    input.insert_str("hello");

    // Esc: Insert → Normal
    handle_key_event(
        KeyEvent::new(KeyCode::Esc, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(state.vim_mode, rip_tui::VimMode::Normal);

    // In Normal, `h` moves back — it does NOT insert text.
    handle_key_event(
        KeyEvent::new(KeyCode::Char('h'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.lines(), &["hello".to_string()]);

    // `i` drops back into Insert, then a Char actually types.
    handle_key_event(
        KeyEvent::new(KeyCode::Char('i'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(state.vim_mode, rip_tui::VimMode::Insert);
    handle_key_event(
        KeyEvent::new(KeyCode::Char('X'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    // Cursor was at col 4 after the `h` moved it back from end; then
    // Insert at that position splices `X` before the final `o`.
    assert_eq!(input.lines(), &["hellXo".to_string()]);
}

#[test]
fn vim_mode_dd_yanks_line_and_p_pastes_it_back() {
    // Two-key operator sanity check: `dd` cuts the current line and
    // yanks it into the textarea's yank buffer, `p` then restores
    // it. Also proves the pending-prefix clears between actions.
    let keymap = Keymap::default();
    let mut state = seed_state();
    state.vim_input_mode = true;
    state.vim_mode = rip_tui::VimMode::Normal;
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();
    input.insert_str("first line\nsecond line");
    // Move to the start of the first line.
    input.move_cursor(ratatui_textarea::CursorMove::Top);

    // `d` sets pending, `d` completes the operator.
    handle_key_event(
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(state.vim_pending, Some('d'));
    handle_key_event(
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(state.vim_pending, None);
    assert_eq!(input.lines(), &["second line".to_string()]);
    assert_eq!(input.yank_text(), "first line");

    // `p` pastes on the current line. We don't assert exact
    // position because Vim and textarea differ on where `p` lands
    // for line yanks; what matters is that the yanked text is back
    // in the buffer.
    handle_key_event(
        KeyEvent::new(KeyCode::Char('p'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    let joined = input.lines().join("\n");
    assert!(joined.contains("first line"));
    assert!(joined.contains("second line"));
}

#[test]
fn vim_mode_gg_unmatched_prefix_falls_through_on_third_key() {
    // If the user presses `g` then anything other than `g`, the
    // pending prefix must clear and the follow-up key must be
    // interpreted fresh — otherwise typos strand the editor in a
    // half-committed operator state.
    let keymap = Keymap::default();
    let mut state = seed_state();
    state.vim_input_mode = true;
    state.vim_mode = rip_tui::VimMode::Normal;
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();
    input.insert_str("line one\nline two\nline three");
    input.move_cursor(ratatui_textarea::CursorMove::Bottom);

    handle_key_event(
        KeyEvent::new(KeyCode::Char('g'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(state.vim_pending, Some('g'));
    // Unmatched follow-up: `k` should move up one line and the
    // pending prefix should clear.
    handle_key_event(
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(state.vim_pending, None);
    assert_eq!(input.cursor().0, 1);
}

#[test]
fn vim_normal_mode_motion_and_edit_primitives() {
    // One sweep over every non-operator Normal-mode key we ship,
    // plus the Ctrl-modifier escape hatch. The goal is to prove
    // each branch is wired — not to re-test the textarea itself —
    // so assertions are positional rather than string-equality
    // checks that would duplicate the textarea's own test suite.
    use ratatui_textarea::CursorMove;
    let keymap = Keymap::default();
    let mut state = seed_state();
    state.vim_input_mode = true;
    state.vim_mode = rip_tui::VimMode::Normal;
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();
    input.insert_str("abc def\nghi jkl");

    // Start at (0, 0) for a reproducible motion sequence.
    input.move_cursor(CursorMove::Top);
    input.move_cursor(CursorMove::Head);

    // `l` forward, `h` back — basic horizontal motion.
    handle_key_event(
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.cursor(), (0, 1));
    handle_key_event(
        KeyEvent::new(KeyCode::Char('h'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.cursor(), (0, 0));

    // `w` word forward, `b` word back.
    handle_key_event(
        KeyEvent::new(KeyCode::Char('w'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.cursor().1, 4);
    handle_key_event(
        KeyEvent::new(KeyCode::Char('b'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.cursor().1, 0);

    // `e` word end.
    handle_key_event(
        KeyEvent::new(KeyCode::Char('e'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert!(input.cursor().1 > 0);

    // `$` end of line, `0` head of line.
    handle_key_event(
        KeyEvent::new(KeyCode::Char('$'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.cursor(), (0, 7));
    handle_key_event(
        KeyEvent::new(KeyCode::Char('0'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.cursor(), (0, 0));

    // `j` down, `k` up.
    handle_key_event(
        KeyEvent::new(KeyCode::Char('j'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.cursor().0, 1);
    handle_key_event(
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.cursor().0, 0);

    // `G` bottom.
    handle_key_event(
        KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.cursor().0, 1);

    // Arrow keys still move in Normal mode.
    handle_key_event(
        KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.cursor().0, 0);
    handle_key_event(
        KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.cursor().0, 1);
    handle_key_event(
        KeyEvent::new(KeyCode::Left, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    handle_key_event(
        KeyEvent::new(KeyCode::Right, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    // Home/End both reach CursorMove::Head/End and stay in Normal.
    handle_key_event(
        KeyEvent::new(KeyCode::Home, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.cursor().1, 0);
    handle_key_event(
        KeyEvent::new(KeyCode::End, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    // Vim Normal-mode Backspace is "move cursor left", never delete.
    let before = input.lines().concat();
    handle_key_event(
        KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.lines().concat(), before);
    assert_eq!(state.vim_mode, rip_tui::VimMode::Normal);
}

#[test]
fn vim_normal_mode_insert_entry_keys_flip_to_insert() {
    // `i / a / A / I / o / O` each drop into Insert mode at a
    // different cursor landing spot.
    let keymap = Keymap::default();
    let mut mode = RenderMode::Json;

    let cases: &[(char, KeyModifiers)] = &[
        ('i', KeyModifiers::empty()),
        ('a', KeyModifiers::empty()),
        ('A', KeyModifiers::SHIFT),
        ('I', KeyModifiers::SHIFT),
        ('o', KeyModifiers::empty()),
        ('O', KeyModifiers::SHIFT),
    ];
    for (ch, modifiers) in cases {
        let mut state = seed_state();
        state.vim_input_mode = true;
        state.vim_mode = rip_tui::VimMode::Normal;
        let mut input = TextArea::default();
        input.insert_str("abc\ndef");
        input.move_cursor(ratatui_textarea::CursorMove::Top);

        handle_key_event(
            KeyEvent::new(KeyCode::Char(*ch), *modifiers),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
        assert_eq!(
            state.vim_mode,
            rip_tui::VimMode::Insert,
            "`{ch}` should flip Normal → Insert",
        );
    }
}

#[test]
fn vim_normal_mode_x_deletes_char_and_u_undoes() {
    let keymap = Keymap::default();
    let mut state = seed_state();
    state.vim_input_mode = true;
    state.vim_mode = rip_tui::VimMode::Normal;
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();
    input.insert_str("abc");
    input.move_cursor(ratatui_textarea::CursorMove::Top);
    input.move_cursor(ratatui_textarea::CursorMove::Head);

    handle_key_event(
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.lines(), &["bc".to_string()]);

    handle_key_event(
        KeyEvent::new(KeyCode::Char('u'), KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    // Undo restores the `a` — proves the `u` path reached
    // textarea's undo stack.
    assert_eq!(input.lines(), &["abc".to_string()]);
}

#[test]
fn vim_normal_mode_lets_ctrl_modified_keys_fall_through_to_keymap() {
    // In Normal mode, Ctrl-modifier keys should still be able to
    // reach the global keymap — otherwise Ctrl-K / Ctrl-C / scroll
    // bindings would be unreachable until the user returned to
    // Insert. We assert the palette hotkey still opens the palette.
    let keymap = Keymap::default();
    let mut state = seed_state();
    state.vim_input_mode = true;
    state.vim_mode = rip_tui::VimMode::Normal;
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();

    let action = handle_key_event(
        KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    // Default keymap binds Ctrl-K to TogglePalette.
    assert_eq!(action, UiAction::TogglePalette);
}

#[test]
fn apply_command_action_toggles_vim_input_mode_and_resets_pending() {
    // Toggling the Options entry must flip both the feature flag
    // AND the starting mode (Normal when on, Insert when off) and
    // clear any stale pending prefix so the new state is coherent.
    let mut state = seed_state();
    state.vim_pending = Some('d');
    let catalog = ModelsMode::new(vec![], BTreeMap::new(), None, None, None);

    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ToggleVimInputMode,
        &mut state,
        &catalog,
    );
    assert!(state.vim_input_mode);
    assert_eq!(state.vim_mode, rip_tui::VimMode::Normal);
    assert_eq!(state.vim_pending, None);

    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ToggleVimInputMode,
        &mut state,
        &catalog,
    );
    assert!(!state.vim_input_mode);
    assert_eq!(state.vim_mode, rip_tui::VimMode::Insert);
}

#[test]
fn apply_model_palette_selection_updates_overrides_and_preferred_target() {
    let mut state = seed_state();
    state.open_palette(
        rip_tui::PaletteMode::Model,
        rip_tui::PaletteOrigin::TopRight,
        vec![rip_tui::PaletteEntry {
            value: "openrouter/openai/gpt-oss-20b".to_string(),
            title: "openrouter/openai/gpt-oss-20b".to_string(),
            subtitle: None,
            chips: vec![],
        }],
        "No models",
        true,
        "Use typed route",
    );

    let mut catalog = ModelsMode::new(
        vec![ModelRoute {
            route: "openrouter/openai/gpt-oss-20b".to_string(),
            provider_id: "openrouter".to_string(),
            model_id: "openai/gpt-oss-20b".to_string(),
            endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
            label: None,
            variants: 0,
            sources: vec!["catalog".to_string()],
        }],
        BTreeMap::from([(
            "openrouter".to_string(),
            "https://openrouter.ai/api/v1/responses".to_string(),
        )]),
        None,
        None,
        None,
    );
    let mut overrides = Some(serde_json::json!({
        "parallel_tool_calls": true
    }));

    apply_model_palette_selection(&mut state, &mut overrides, &mut catalog).expect("apply");

    assert_eq!(state.palette_query(), None);
    assert_eq!(
        state.preferred_openresponses_endpoint.as_deref(),
        Some("https://openrouter.ai/api/v1/responses")
    );
    assert_eq!(
        state.preferred_openresponses_model.as_deref(),
        Some("openai/gpt-oss-20b")
    );
    assert_eq!(
        overrides,
        Some(serde_json::json!({
            "parallel_tool_calls": true,
            "endpoint": "https://openrouter.ai/api/v1/responses",
            "model": "openai/gpt-oss-20b"
        }))
    );
    assert_eq!(
        catalog.current_route.as_deref(),
        Some("openrouter/openai/gpt-oss-20b")
    );
    assert_eq!(
        catalog.current_endpoint.as_deref(),
        Some("https://openrouter.ai/api/v1/responses")
    );
    assert_eq!(catalog.current_model.as_deref(), Some("openai/gpt-oss-20b"));
}

#[test]
fn handle_term_event_ignores_resize() {
    let keymap = Keymap::default();
    let mut state = seed_state();
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();
    let action = handle_term_event(
        TermEvent::Resize(10, 10),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(action, UiAction::None);
}

#[test]
fn handle_term_event_routes_key() {
    let keymap = Keymap::default();
    let mut state = seed_state();
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();
    let action = handle_term_event(
        TermEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(action, UiAction::Submit);
}

#[test]
fn handle_term_event_routes_mouse_scroll() {
    let keymap = Keymap::default();
    let mut state = seed_state();
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();
    let action = handle_term_event(
        TermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        }),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(action, UiAction::ScrollCanvasUp);
}

#[test]
fn mouse_clicks_hero_segments_open_expected_palettes() {
    let mut state = seed_state();
    state.set_continuity_id("thread-alpha");
    state.set_preferred_openresponses_target(
        Some("https://openrouter.ai/api/v1/responses".to_string()),
        Some("nvidia/nemotron".to_string()),
    );
    let (width, _) = terminal_size().unwrap_or((80, 24));
    let thread_column = (0..width)
        .find(|column| hero_click_target(&state, width, *column) == Some(HeroClickTarget::Thread))
        .expect("thread target column");
    let agent_column = (0..width)
        .find(|column| hero_click_target(&state, width, *column) == Some(HeroClickTarget::Agent))
        .expect("agent target column");
    let model_column = (0..width)
        .find(|column| hero_click_target(&state, width, *column) == Some(HeroClickTarget::Model))
        .expect("model target column");

    let thread_action = handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: thread_column,
            row: 0,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );
    assert_eq!(thread_action, UiAction::OpenPaletteThreads);

    let agent_action = handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: agent_column,
            row: 0,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );
    assert_eq!(agent_action, UiAction::TogglePalette);

    let model_action = handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: model_column,
            row: 0,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );
    assert_eq!(model_action, UiAction::OpenPaletteModels);
}

#[test]
fn mouse_activity_row_opens_activity_overlay() {
    let mut state = seed_state();
    let (_, height) = terminal_size().unwrap_or((80, 24));
    let row = mouse_footer_activity_row(height).expect("activity row");

    let action = handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );

    assert_eq!(action, UiAction::None);
    assert_eq!(state.overlay(), &rip_tui::Overlay::Activity);
}

#[test]
fn mouse_scroll_with_palette_open_moves_selection_and_ignores_noise() {
    // When the palette is the top of the overlay stack, mouse
    // scrolling must drive selection (not the canvas). Click +
    // scroll-horizontally must fall through to the no-op arm so
    // stray middle/right buttons don't flicker the overlay.
    let mut state = seed_state();
    state.open_palette(
        rip_tui::PaletteMode::Command,
        rip_tui::PaletteOrigin::TopCenter,
        (0..5)
            .map(|i| rip_tui::PaletteEntry {
                value: format!("c-{i}"),
                title: format!("cmd {i}"),
                subtitle: None,
                chips: vec![],
            })
            .collect(),
        "",
        false,
        String::new(),
    );
    let before = state.palette_selected_value().expect("selected");
    assert_eq!(before, "c-0");
    handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );
    let stepped = state.palette_selected_value().expect("selected");
    assert_ne!(stepped, before);
    handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );
    assert_eq!(
        state.palette_selected_value().as_deref(),
        Some(before.as_str())
    );
    // A non-scroll / non-left click with the palette open is a
    // no-op — the palette intercepts but returns UiAction::None.
    let action = handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );
    assert_eq!(action, UiAction::None);
}

#[test]
fn mouse_events_with_thread_picker_open_route_to_picker_helpers() {
    let mut state = seed_state();
    state.open_thread_picker(vec![
        rip_tui::ThreadPickerEntry {
            thread_id: "cont-a".into(),
            title: "alpha".into(),
            preview: "…".into(),
            chips: vec![],
        },
        rip_tui::ThreadPickerEntry {
            thread_id: "cont-b".into(),
            title: "beta".into(),
            preview: "…".into(),
            chips: vec![],
        },
    ]);
    handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );
    assert_eq!(
        state.thread_picker_selected_value().as_deref(),
        Some("cont-b")
    );
    let action = handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );
    assert_eq!(action, UiAction::ApplyThreadPicker);
}

#[test]
fn mouse_click_focuses_canvas_message() {
    let mut state = TuiState::new(10);
    let first = state.canvas.push_user_turn("user", "tui", "hello", 0);
    state.canvas.push_user_turn("user", "tui", "world", 1);
    let (width, height) = terminal_size().unwrap_or((80, 24));
    let (viewport_width, viewport_height, row_in_canvas) =
        mouse_canvas_hit_geometry(&state, width, height, 0, 1).expect("canvas geometry");
    let expected = canvas_hit_message_id(&state, viewport_width, viewport_height, row_in_canvas);

    let action = handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 1,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );

    assert_eq!(action, UiAction::None);
    assert_eq!(expected.as_deref(), Some(first.as_str()));
    assert_eq!(state.focused_message_id.as_deref(), Some(first.as_str()));
    assert!(!state.auto_follow);
}

#[test]
fn thread_picker_mouse_scrolls_and_click_applies() {
    let mut state = seed_state();
    state.open_thread_picker(vec![
        rip_tui::ThreadPickerEntry {
            thread_id: "t1".to_string(),
            title: "one".to_string(),
            preview: "preview —".to_string(),
            chips: vec!["current".to_string()],
        },
        rip_tui::ThreadPickerEntry {
            thread_id: "t2".to_string(),
            title: "two".to_string(),
            preview: "preview —".to_string(),
            chips: vec![],
        },
    ]);

    let scroll = handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 5,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );
    assert_eq!(scroll, UiAction::None);
    assert_eq!(state.thread_picker_selected_value().as_deref(), Some("t2"));

    let click = handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 5,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );
    assert_eq!(click, UiAction::ApplyThreadPicker);
}

#[test]
fn handle_key_event_toggles_follow_and_palette_cycle_is_noop_outside_palette() {
    // Phase C.5 retires Tab's legacy "details-mode toggle" role
    // and reassigns Tab to `PaletteCycleMode`. Outside of an open
    // palette, Tab is a no-op.
    //
    // `Alt+T` no longer toggles the theme — theme switching is a
    // palette action (Options mode). Users who want the legacy
    // binding back can re-add `"M-t": "ToggleTheme"` in
    // `~/.rip/keybindings.json`.
    let keymap = Keymap::default();
    let mut state = seed_state();
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();

    let action = handle_key_event(
        KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
        &mut state,
        &mut mode,
        &mut input,
        true,
        &keymap,
    );
    assert_eq!(action, UiAction::None);
    assert_eq!(mode, RenderMode::Json);

    let action = handle_key_event(
        KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
        &mut state,
        &mut mode,
        &mut input,
        true,
        &keymap,
    );
    assert_eq!(action, UiAction::None);
    assert!(!state.auto_follow);
}

#[test]
fn palette_hotkeys_dispatch_to_correct_ui_actions() {
    // The four new palette openers bound in Phase C.5 all have
    // direct `UiAction::OpenPalette…` arms. We exercise each one
    // to make sure the keymap→UiAction glue didn't regress to
    // generic `TogglePalette` routing.
    let keymap = Keymap::default();
    let mut state = seed_state();
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();

    let k = |ch: char, mods: KeyModifiers| KeyEvent::new(KeyCode::Char(ch), mods);
    assert_eq!(
        handle_key_event(
            k('k', KeyModifiers::CONTROL),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        ),
        UiAction::TogglePalette
    );
    assert_eq!(
        handle_key_event(
            k('g', KeyModifiers::CONTROL),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        ),
        UiAction::OpenPaletteGoTo
    );
    assert_eq!(
        handle_key_event(
            k('m', KeyModifiers::ALT),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        ),
        UiAction::OpenPaletteModels
    );
    assert_eq!(
        handle_key_event(
            k('o', KeyModifiers::ALT),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        ),
        UiAction::OpenPaletteOptions
    );
}

#[test]
fn move_selected_sets_last_seq_and_clamps() {
    let mut state = seed_state();
    state.selected_seq = None;
    move_selected(&mut state, -1);
    assert_eq!(state.selected_seq, Some(2));

    move_selected(&mut state, 10);
    assert_eq!(state.selected_seq, Some(2));

    move_selected(&mut state, -10);
    assert_eq!(state.selected_seq, Some(0));
}

#[tokio::test]
async fn next_sse_event_returns_open() {
    let server = MockServer::start();
    let _events = server.mock(|when, then| {
        when.method(GET).path("/events");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body("data: {}\n\n");
    });

    let client = Client::new();
    let url = format!("{}/events", server.base_url());
    let mut source = Some(client.get(url).eventsource().expect("eventsource"));
    let next = timeout(Duration::from_millis(200), next_sse_event(&mut source))
        .await
        .expect("timeout");
    assert!(next.is_some());
}

#[tokio::test]
async fn next_sse_event_pending_when_none() {
    let mut source: Option<EventSource> = None;
    let result = timeout(Duration::from_millis(10), next_sse_event(&mut source)).await;
    assert!(result.is_err());
}

struct EnvGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = self.previous.take() {
            std::env::set_var(self.key, value);
        } else {
            std::env::remove_var(self.key);
        }
    }
}

fn set_env(key: &'static str, value: impl Into<OsString>) -> EnvGuard {
    let previous = std::env::var_os(key);
    let value = value.into();
    std::env::set_var(key, &value);
    EnvGuard { key, previous }
}

fn remove_env(key: &'static str) -> EnvGuard {
    let previous = std::env::var_os(key);
    std::env::remove_var(key);
    EnvGuard { key, previous }
}

#[test]
fn load_theme_reads_env_and_file() {
    let _lock = test_env::lock_env();
    let _clear_theme = remove_env("RIP_TUI_THEME");
    let temp_root = std::env::temp_dir().join(format!("rip_theme_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_root).expect("temp dir");
    let theme_path = temp_root.join("theme.json");

    let _config = set_env("RIP_CONFIG_HOME", temp_root.as_os_str());
    std::fs::write(&theme_path, "\"default-dark\"").expect("theme");
    assert_eq!(
        load_theme().expect("theme load"),
        Some(rip_tui::ThemeId::DefaultDark)
    );

    std::fs::write(&theme_path, "{ \"theme\": \"light\" }").expect("theme");
    assert_eq!(
        load_theme().expect("theme load"),
        Some(rip_tui::ThemeId::DefaultLight)
    );

    let _theme_env = set_env("RIP_TUI_THEME", "dark");
    assert_eq!(
        load_theme().expect("theme load"),
        Some(rip_tui::ThemeId::DefaultDark)
    );
    drop(_theme_env);

    std::fs::write(&theme_path, "{").expect("theme");
    assert!(load_theme().is_err());
}

#[test]
fn load_model_palette_catalog_reads_config_and_current_override() {
    let _lock = test_env::lock_env();
    let temp_root =
        std::env::temp_dir().join(format!("rip_model_palette_test_{}", std::process::id()));
    let workspace_dir = temp_root.join("workspace");
    std::fs::create_dir_all(&workspace_dir).expect("workspace");
    std::fs::create_dir_all(&temp_root).expect("config dir");
    std::fs::write(
        temp_root.join("config.jsonc"),
        r#"{
  "provider": {
"openrouter": {
  "endpoint": "https://openrouter.ai/api/v1/responses",
  "models": {
    "openai/gpt-oss-20b": { "label": "OSS 20B" }
  }
},
"openai": {
  "endpoint": "https://api.openai.com/v1/responses",
  "models": {
    "gpt-5-nano-2025-08-07": { "label": "GPT-5 Nano" }
  }
}
  },
  "model": "openrouter/openai/gpt-oss-20b"
}"#,
    )
    .expect("config");

    let _config_home = set_env("RIP_CONFIG_HOME", temp_root.as_os_str());
    let _workspace = set_env("RIP_WORKSPACE_ROOT", workspace_dir.as_os_str());
    let _clear_custom = remove_env("RIP_CONFIG");
    let _clear_endpoint = remove_env("RIP_OPENRESPONSES_ENDPOINT");
    let _clear_model = remove_env("RIP_OPENRESPONSES_MODEL");
    let _clear_stateful = remove_env("RIP_OPENRESPONSES_STATELESS_HISTORY");
    let _clear_parallel = remove_env("RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS");
    let _clear_followup = remove_env("RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE");

    let overrides = serde_json::json!({
        "endpoint": "https://openrouter.ai/api/v1/responses",
        "model": "nvidia/nemotron-3-nano-30b-a3b:free"
    });
    let catalog = load_model_palette_catalog(Some(&overrides));

    assert_eq!(
        catalog.current_route.as_deref(),
        Some("openrouter/nvidia/nemotron-3-nano-30b-a3b:free")
    );
    let values = catalog
        .entries()
        .into_iter()
        .map(|entry| entry.value)
        .collect::<Vec<_>>();
    assert!(values.contains(&"openrouter/openai/gpt-oss-20b".to_string()));
    assert!(values.contains(&"openai/gpt-5-nano-2025-08-07".to_string()));
    assert!(values.contains(&"openrouter/nvidia/nemotron-3-nano-30b-a3b:free".to_string()));
}

#[test]
fn model_palette_helper_functions_cover_override_paths() {
    assert_eq!(
        parse_model_route(" openrouter / model-x "),
        Some(("openrouter".to_string(), "model-x".to_string()))
    );
    assert_eq!(parse_model_route("openrouter"), None);
    assert_eq!(
        default_endpoint_for_provider("openrouter").as_deref(),
        Some("https://openrouter.ai/api/v1/responses")
    );
    assert_eq!(default_endpoint_for_provider("missing"), None);
    assert_eq!(
        infer_provider_id_from_endpoint("https://openrouter.ai/api/v1/responses").as_deref(),
        Some("openrouter")
    );
    assert_eq!(
        infer_provider_id_from_endpoint("https://api.openai.com/v1/responses").as_deref(),
        Some("openai")
    );
    assert_eq!(infer_provider_id_from_endpoint("https://example.com"), None);

    let overrides = openresponses_override_input_from_json(Some(&serde_json::json!({
        "endpoint": "https://openrouter.ai/api/v1/responses",
        "model": "openai/gpt-oss-20b",
        "stateless_history": true,
        "parallel_tool_calls": false,
        "followup_user_message": "keep going"
    })));
    assert_eq!(
        overrides.endpoint.as_deref(),
        Some("https://openrouter.ai/api/v1/responses")
    );
    assert_eq!(overrides.model.as_deref(), Some("openai/gpt-oss-20b"));
    assert_eq!(overrides.stateless_history, Some(true));
    assert_eq!(overrides.parallel_tool_calls, Some(false));
    assert_eq!(
        overrides.followup_user_message.as_deref(),
        Some("keep going")
    );

    let mut routes = BTreeMap::new();
    let provider_endpoints = BTreeMap::from([(
        "openrouter".to_string(),
        "https://openrouter.ai/api/v1/responses".to_string(),
    )]);
    push_route_from_string(
        &mut routes,
        &provider_endpoints,
        "openrouter/openai/gpt-oss-20b",
        "config:model",
    );
    upsert_model_route(
        &mut routes,
        "openrouter",
        "openai/gpt-oss-20b",
        "https://openrouter.ai/api/v1/responses",
        Some("OSS 20B".to_string()),
        3,
        "config:roles.primary",
    );
    let record = routes
        .get("openrouter/openai/gpt-oss-20b")
        .expect("route present");
    assert_eq!(record.label.as_deref(), Some("OSS 20B"));
    assert_eq!(record.variants, 3);
    assert!(record.sources.iter().any(|source| source == "config:model"));
    assert!(record
        .sources
        .iter()
        .any(|source| source == "config:roles.primary"));
}

#[test]
fn open_model_palette_uses_catalog_entries_and_mouse_scroll_moves_selection() {
    let mut state = seed_state();
    let catalog = ModelsMode::new(
        vec![
            ModelRoute {
                route: "openrouter/openai/gpt-oss-20b".to_string(),
                provider_id: "openrouter".to_string(),
                model_id: "openai/gpt-oss-20b".to_string(),
                endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
                label: Some("OSS 20B".to_string()),
                variants: 0,
                sources: vec!["catalog".to_string()],
            },
            ModelRoute {
                route: "openai/gpt-5-nano".to_string(),
                provider_id: "openai".to_string(),
                model_id: "gpt-5-nano".to_string(),
                endpoint: "https://api.openai.com/v1/responses".to_string(),
                label: Some("GPT-5 Nano".to_string()),
                variants: 0,
                sources: vec!["catalog".to_string()],
            },
        ],
        BTreeMap::new(),
        Some("openrouter/openai/gpt-oss-20b".to_string()),
        Some("https://openrouter.ai/api/v1/responses".to_string()),
        Some("openai/gpt-oss-20b".to_string()),
    );

    open_model_palette(&mut state, &catalog, PaletteOrigin::TopRight);
    assert!(state.is_palette_open());
    assert_eq!(
        state.palette_selected_value().as_deref(),
        Some("openrouter/openai/gpt-oss-20b")
    );

    let action = handle_mouse_event(
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        },
        &mut state,
    );
    assert_eq!(action, UiAction::None);
    assert_eq!(
        state.palette_selected_value().as_deref(),
        Some("openai/gpt-5-nano")
    );
}

#[test]
fn load_theme_missing_file_returns_none() {
    let _lock = test_env::lock_env();
    let _clear_theme = remove_env("RIP_TUI_THEME");
    let temp_root = std::env::temp_dir().join(format!("rip_theme_missing_{}", std::process::id()));
    let _config = set_env("RIP_CONFIG_HOME", temp_root.as_os_str());
    let value = load_theme().expect("load theme");
    assert!(value.is_none());
}

#[test]
fn config_dir_prefers_env_override() {
    let _lock = test_env::lock_env();
    let temp_root = std::env::temp_dir().join(format!("rip_config_test_{}", std::process::id()));
    let _config = set_env("RIP_CONFIG_HOME", temp_root.as_os_str());
    let _home = set_env("HOME", "/tmp");
    assert_eq!(config_dir().expect("config dir"), temp_root);
    assert_eq!(
        theme_path().expect("theme path"),
        temp_root.join("theme.json")
    );
}

#[test]
fn config_dir_falls_back_to_home() {
    let _lock = test_env::lock_env();
    let _config = remove_env("RIP_CONFIG_HOME");
    let temp_home = std::env::temp_dir().join(format!("rip_home_{}", std::process::id()));
    let _home = set_env("HOME", temp_home.as_os_str());
    assert_eq!(config_dir().expect("config dir"), temp_home.join(".rip"));
}

#[test]
fn init_fullscreen_state_sets_warning_on_theme_error() {
    let _lock = test_env::lock_env();
    let _bad_theme = set_env("RIP_TUI_THEME", "unknown-theme");
    let init = init_fullscreen_state(Some("hello".to_string()));
    assert_eq!(init.mode, RenderMode::Json);
    assert_eq!(init.input.lines(), &["hello".to_string()]);
    let status = init.state.status_message.unwrap_or_default();
    assert!(status.contains("theme:"));
}

#[test]
fn init_fullscreen_state_includes_keymap_warning() {
    let _lock = test_env::lock_env();
    let _clear_theme = set_env("RIP_TUI_THEME", "dark");
    let temp_root = std::env::temp_dir().join(format!("rip_keys_{}", std::process::id()));
    std::fs::create_dir_all(&temp_root).expect("temp dir");
    let keymap_path = temp_root.join("keybindings.json");
    std::fs::write(&keymap_path, "{").expect("keymap");
    let _keymap = set_env("RIP_KEYBINDINGS_PATH", keymap_path.as_os_str());

    let init = init_fullscreen_state(None);
    let status = init.state.status_message.unwrap_or_default();
    assert!(status.contains("keybindings: invalid json"));
}

#[test]
fn prepare_copy_selected_reports_no_selection() {
    let mut state = TuiState::default();
    let action = prepare_copy_selected(&mut state);
    assert_eq!(action, CopySelectedAction::None);
    assert_eq!(
        state.status_message.as_deref(),
        Some("clipboard: no frame selected")
    );
}

#[test]
fn prepare_copy_selected_uses_osc52_for_small_payload() {
    let _lock = test_env::lock_env();
    let _disable = remove_env("RIP_TUI_DISABLE_OSC52");
    let mut state = seed_state();
    let action = prepare_copy_selected(&mut state);
    assert!(matches!(action, CopySelectedAction::Osc52(_)));
}

#[test]
fn prepare_copy_selected_stores_when_disabled() {
    let _lock = test_env::lock_env();
    let _disable = set_env("RIP_TUI_DISABLE_OSC52", "1");
    let mut state = seed_state();
    let action = prepare_copy_selected(&mut state);
    assert_eq!(action, CopySelectedAction::Store);
    assert!(state.clipboard_buffer.is_some());
    let status = state.status_message.unwrap_or_default();
    assert!(status.contains("OSC52 disabled"));
}

#[test]
fn prepare_copy_selected_stores_when_large() {
    let _lock = test_env::lock_env();
    let _disable = remove_env("RIP_TUI_DISABLE_OSC52");
    let mut state = TuiState::default();
    let payload = "x".repeat(super::copy::OSC52_MAX_BYTES + 100);
    state.update(FrameEvent {
        id: "big".to_string(),
        session_id: "s1".to_string(),
        timestamp_ms: 0,
        seq: 0,
        kind: EventKind::SessionStarted { input: payload },
    });
    state.selected_seq = Some(0);

    let action = prepare_copy_selected(&mut state);
    assert_eq!(action, CopySelectedAction::Store);
    let status = state.status_message.unwrap_or_default();
    assert!(status.contains("too large"));
}
