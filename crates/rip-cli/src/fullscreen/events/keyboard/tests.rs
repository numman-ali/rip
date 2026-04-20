//! Minimal keyboard tests for the branches not already exercised by
//! `fullscreen/tests.rs`:
//! - the ErrorRecovery overlay owns the key stream until dismissed,
//! - the thread picker overlay consults the keymap for apply/move,
//! - the palette overlay handles bulk-scroll via PageUp/PageDown,
//! - the default (no-overlay) keymap dispatch runs a binding body
//!   (C-f → `ToggleFollow`, chosen because it mutates state so the
//!   test can prove the match arm actually ran).
//!
//! Each test covers a distinct branch; we intentionally don't enumerate
//! every arm of the keymap match — that'd be enum-echo coverage, not
//! honest tests. The goal here is just to walk into the four outer
//! branches of `handle_key_event` at least once.

use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::TextArea;
use rip_tui::{
    Overlay, PaletteEntry, PaletteMode, PaletteOrigin, RenderMode, ThreadPickerEntry, TuiState,
};

use crate::fullscreen::keymap::Keymap;

fn press(
    state: &mut TuiState,
    input: &mut TextArea<'static>,
    code: KeyCode,
    mods: KeyModifiers,
) -> UiAction {
    let keymap = Keymap::default();
    let mut mode = RenderMode::Json;
    handle_key_event(
        KeyEvent::new(code, mods),
        state,
        &mut mode,
        input,
        false,
        &keymap,
    )
}

#[test]
fn error_recovery_dispatches_every_action_and_falls_through_to_noop() {
    let mut state = TuiState::new(100);
    state.set_overlay(Overlay::ErrorRecovery { seq: 42 });
    let mut input = TextArea::default();

    for (code, expected) in [
        (KeyCode::Char('r'), UiAction::ErrorRecoveryRetry),
        (KeyCode::Char('c'), UiAction::ErrorRecoveryRotateCursor),
        (KeyCode::Char('m'), UiAction::ErrorRecoverySwitchModel),
        (KeyCode::Char('x'), UiAction::ErrorRecoveryXray),
        (KeyCode::Esc, UiAction::CloseOverlay),
    ] {
        let action = press(&mut state, &mut input, code, KeyModifiers::empty());
        assert_eq!(action, expected, "keycode {code:?}");
        assert!(matches!(state.overlay(), Overlay::ErrorRecovery { .. }));
    }

    // Unknown key inside ErrorRecovery → None (guards the `_` arm).
    let action = press(&mut state, &mut input, KeyCode::Down, KeyModifiers::empty());
    assert_eq!(action, UiAction::None);
}

#[test]
fn thread_picker_down_then_enter_applies_selection() {
    let mut state = TuiState::new(100);
    state.open_thread_picker(vec![
        ThreadPickerEntry {
            thread_id: "t1".to_string(),
            title: "one".to_string(),
            preview: "p".to_string(),
            chips: vec![],
        },
        ThreadPickerEntry {
            thread_id: "t2".to_string(),
            title: "two".to_string(),
            preview: "p".to_string(),
            chips: vec![],
        },
    ]);
    let mut input = TextArea::default();

    // Down moves selection inside the picker.
    let action = press(&mut state, &mut input, KeyCode::Down, KeyModifiers::empty());
    assert_eq!(action, UiAction::None);
    match state.overlay() {
        Overlay::ThreadPicker(picker) => assert_eq!(picker.selected, 1),
        other => panic!("expected ThreadPicker, got {other:?}"),
    }

    // Enter fires ApplyThreadPicker.
    let action = press(
        &mut state,
        &mut input,
        KeyCode::Enter,
        KeyModifiers::empty(),
    );
    assert_eq!(action, UiAction::ApplyThreadPicker);
}

#[test]
fn palette_pagedown_bulk_scrolls_selection() {
    let entries: Vec<PaletteEntry> = (0..8)
        .map(|i| PaletteEntry {
            value: format!("e{i}"),
            title: format!("e{i}"),
            subtitle: None,
            chips: vec![],
        })
        .collect();
    let mut state = TuiState::new(100);
    state.open_palette(
        PaletteMode::Command,
        PaletteOrigin::TopCenter,
        entries,
        "No entries",
        false,
        "",
    );
    let mut input = TextArea::default();

    let action = press(
        &mut state,
        &mut input,
        KeyCode::PageDown,
        KeyModifiers::empty(),
    );
    assert_eq!(action, UiAction::None);
    let after = state
        .palette_selected_value()
        .expect("palette still open with selection");
    assert_ne!(
        after, "e0",
        "PageDown should have moved past the first entry"
    );
}

#[test]
fn default_keymap_ctrl_f_toggles_auto_follow() {
    // Single representative from the default-keymap dispatch arms.
    // Walks into `handle_key_event` with no overlay and a command
    // binding that mutates state in-place, proving the default-path
    // match is reachable and its body runs.
    let mut state = TuiState::new(100);
    let before = state.auto_follow;
    let mut input = TextArea::default();
    let action = press(
        &mut state,
        &mut input,
        KeyCode::Char('f'),
        KeyModifiers::CONTROL,
    );
    assert_eq!(action, UiAction::None);
    assert_ne!(state.auto_follow, before, "C-f should toggle auto_follow");
}

#[test]
fn question_mark_opens_help_only_when_input_is_empty() {
    let mut state = TuiState::new(100);
    let mut input = TextArea::default();

    let action = press(
        &mut state,
        &mut input,
        KeyCode::Char('?'),
        KeyModifiers::empty(),
    );
    assert_eq!(action, UiAction::ShowHelp);
    assert_eq!(input.lines(), &[String::new()]);

    let mut state = TuiState::new(100);
    let mut input = TextArea::default();
    input.insert_str("why");

    let action = press(
        &mut state,
        &mut input,
        KeyCode::Char('?'),
        KeyModifiers::empty(),
    );
    assert_eq!(action, UiAction::None);
    assert_eq!(input.lines(), &["why?".to_string()]);
}
