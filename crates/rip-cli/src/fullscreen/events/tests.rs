//! Tests for the small pure helpers in `events/mod.rs` (buffer
//! emptiness / trimming, selection movement, card-expand detection,
//! `last_user_prompt` traversal) and for the `handle_term_event`
//! non-Key/Mouse/Resize fallthrough. The `handle_key_event` /
//! `handle_mouse_event` dispatches live in the sibling tests for the
//! respective submodules and in `fullscreen/tests.rs`.

use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui_textarea::TextArea;
use rip_kernel::ToolTaskExecutionMode;
use rip_tui::canvas::{Block, CachedText, CanvasMessage};
use rip_tui::{AgentRole, RenderMode, TaskCardStatus, ToolCardStatus, TuiState};

fn cached(text: &str) -> CachedText {
    CachedText::plain(text)
}

#[test]
fn buffer_is_effectively_empty_reports_true_for_whitespace_only_lines() {
    let mut input = TextArea::default();
    // A fresh textarea is one empty line.
    assert!(buffer_is_effectively_empty(&input));
    input.insert_str("   \t  ");
    assert!(buffer_is_effectively_empty(&input));
}

#[test]
fn buffer_is_effectively_empty_reports_false_once_content_appears() {
    let mut input = TextArea::default();
    input.insert_str("hi");
    assert!(!buffer_is_effectively_empty(&input));
}

#[test]
fn buffer_trimmed_prompt_joins_lines_with_newlines_and_trims() {
    let mut input = TextArea::default();
    input.insert_str("  line one");
    input.insert_newline();
    input.insert_str("line two  ");
    assert_eq!(buffer_trimmed_prompt(&input), "line one\nline two");
}

#[test]
fn buffer_trimmed_prompt_returns_empty_for_whitespace_only_buffer() {
    let mut input = TextArea::default();
    input.insert_str("   ");
    assert_eq!(buffer_trimmed_prompt(&input), "");
}

#[test]
fn card_expand_target_true_for_tool_card_focus() {
    let mut state = TuiState::new(16);
    state.canvas.messages.push(CanvasMessage::ToolCard {
        message_id: "m-tool".to_string(),
        tool_id: "t1".to_string(),
        tool_name: "shell".to_string(),
        args_block: Block::Paragraph(cached("")),
        status: ToolCardStatus::Running,
        body: Vec::new(),
        expanded: false,
        artifact_ids: Vec::new(),
        started_seq: 1,
        started_at_ms: 0,
    });
    state.focus_next_message();
    assert!(card_expand_target(&state));
}

#[test]
fn card_expand_target_true_for_task_card_focus() {
    let mut state = TuiState::new(16);
    state.canvas.messages.push(CanvasMessage::TaskCard {
        message_id: "m-task".to_string(),
        task_id: "task-1".to_string(),
        tool_name: "bash".to_string(),
        title: None,
        execution_mode: ToolTaskExecutionMode::Pipes,
        status: TaskCardStatus::Running,
        body: Vec::new(),
        expanded: false,
        artifact_ids: Vec::new(),
        started_at_ms: Some(0),
    });
    state.focus_next_message();
    assert!(card_expand_target(&state));
}

#[test]
fn card_expand_target_false_when_focus_is_empty_or_non_card() {
    let mut state = TuiState::new(16);
    assert!(!card_expand_target(&state));
    // `push_user_turn` is the canonical seam for synthesizing a
    // UserTurn without poking at the private `mint_id` on `CanvasModel`.
    state.canvas.push_user_turn("user", "tui", "hi", 0);
    state.focus_next_message();
    assert!(!card_expand_target(&state));
}

#[test]
fn move_selected_seeds_from_last_seq_when_none_and_respects_delta() {
    let mut state = TuiState::new(16);
    // Seed one frame so last_seq() returns Some.
    state.update(rip_kernel::Event {
        id: "e0".to_string(),
        session_id: "s1".to_string(),
        timestamp_ms: 0,
        seq: 7,
        kind: rip_kernel::EventKind::SessionStarted {
            input: "hi".to_string(),
        },
    });
    state.selected_seq = None;
    move_selected(&mut state, -1);
    assert_eq!(state.selected_seq, Some(7));

    // Further backward motion clamps to first_seq (which equals last_seq
    // here because there's only one frame).
    move_selected(&mut state, -5);
    assert_eq!(state.selected_seq, Some(7));
}

#[test]
fn move_selected_forward_saturates_at_last_seq() {
    let mut state = TuiState::new(16);
    for (i, id) in ["e0", "e1"].iter().enumerate() {
        state.update(rip_kernel::Event {
            id: (*id).to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: i as u64,
            kind: rip_kernel::EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        });
    }
    state.selected_seq = Some(0);
    move_selected(&mut state, 10);
    assert_eq!(state.selected_seq, Some(1));
}

#[test]
fn last_user_prompt_returns_none_when_no_user_turn() {
    let state = TuiState::new(16);
    assert_eq!(last_user_prompt(&state), None);
}

#[test]
fn last_user_prompt_returns_none_when_user_turn_has_only_blank_blocks() {
    let mut state = TuiState::new(16);
    state.canvas.messages.push(CanvasMessage::UserTurn {
        message_id: "m-u".to_string(),
        actor_id: "user".to_string(),
        origin: "tui".to_string(),
        // Paragraph with only whitespace + a non-text block (ArtifactChip)
        // must both be skipped. Result: fall through to None.
        blocks: vec![
            Block::Paragraph(cached("   \n  ")),
            Block::ArtifactChip {
                artifact_id: "a1".to_string(),
                bytes: None,
            },
        ],
        submitted_at_ms: 0,
    });
    assert_eq!(last_user_prompt(&state), None);
}

#[test]
fn last_user_prompt_finds_most_recent_user_turn() {
    let mut state = TuiState::new(16);
    state.canvas.push_user_turn("user", "tui", "older", 0);
    // Intermix an agent turn to verify the reverse-scan picks the
    // newest UserTurn, not the newest message overall.
    state.canvas.messages.push(CanvasMessage::AgentTurn {
        message_id: "m-a".to_string(),
        run_session_id: "s1".to_string(),
        agent_id: None,
        role: AgentRole::Primary,
        actor_id: "agent".to_string(),
        model: None,
        reasoning_text: String::new(),
        reasoning_summary: String::new(),
        blocks: vec![Block::Paragraph(cached("answer"))],
        streaming_tail: String::new(),
        streaming: false,
        started_at_ms: 0,
        ended_at_ms: None,
    });
    state.canvas.messages.push(CanvasMessage::UserTurn {
        message_id: "m-u2".to_string(),
        actor_id: "user".to_string(),
        origin: "tui".to_string(),
        blocks: vec![Block::Heading {
            level: 2,
            text: cached("newer header"),
        }],
        submitted_at_ms: 0,
    });
    assert_eq!(last_user_prompt(&state).as_deref(), Some("newer header"));
}

#[test]
fn last_user_prompt_joins_multiline_blocks_with_newlines() {
    let mut state = TuiState::new(16);
    state.canvas.messages.push(CanvasMessage::UserTurn {
        message_id: "m-u".to_string(),
        actor_id: "user".to_string(),
        origin: "tui".to_string(),
        blocks: vec![Block::Markdown(cached("line one\nline two"))],
        submitted_at_ms: 0,
    });
    assert_eq!(
        last_user_prompt(&state).as_deref(),
        Some("line one\nline two"),
    );
}

#[test]
fn handle_term_event_focus_gained_is_a_noop() {
    // The `_ =>` arm in handle_term_event covers Paste / FocusGained /
    // FocusLost — anything that isn't Key / Mouse / Resize.
    let keymap = super::super::keymap::Keymap::default();
    let mut state = TuiState::new(16);
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();

    let action = handle_term_event(
        crossterm::event::Event::FocusGained,
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(action, UiAction::None);
}

#[test]
fn handle_term_event_resize_is_a_noop() {
    let keymap = super::super::keymap::Keymap::default();
    let mut state = TuiState::new(16);
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();

    let action = handle_term_event(
        crossterm::event::Event::Resize(80, 24),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(action, UiAction::None);
}

#[test]
fn handle_term_event_routes_key_to_handle_key_event() {
    // A plain Char key against a default keymap with an empty state and
    // a non-running session should write into the textarea (textarea
    // passthrough path). Proves handle_term_event's Key arm dispatches.
    let keymap = super::super::keymap::Keymap::default();
    let mut state = TuiState::new(16);
    let mut mode = RenderMode::Json;
    let mut input = TextArea::default();

    handle_term_event(
        crossterm::event::Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty())),
        &mut state,
        &mut mode,
        &mut input,
        false,
        &keymap,
    );
    assert_eq!(input.lines(), &["a".to_string()]);
}
