use super::update::{extract_artifact_ids, is_error_event, looks_like_artifact_id, push_preview};
use super::*;
use rip_kernel::{
    CheckpointAction, Event, EventKind, ProviderEventStatus, ToolTaskExecutionMode, ToolTaskStatus,
    ToolTaskStream,
};
use serde_json::{json, Value};

fn event(seq: u64, timestamp_ms: u64, kind: EventKind) -> Event {
    Event {
        id: format!("e{seq}"),
        session_id: "s1".to_string(),
        timestamp_ms,
        seq,
        kind,
    }
}

#[test]
fn computes_ttft_and_e2e() {
    let mut state = TuiState::new(100);
    state.update(event(
        0,
        1000,
        EventKind::SessionStarted {
            input: "hi".to_string(),
        },
    ));
    state.update(event(
        1,
        1300,
        EventKind::OutputTextDelta {
            delta: "a".to_string(),
        },
    ));
    state.update(event(
        2,
        1800,
        EventKind::SessionEnded {
            reason: "done".to_string(),
        },
    ));
    assert_eq!(state.ttft_ms(), Some(300));
    assert_eq!(state.e2e_ms(), Some(800));
}

#[test]
fn update_respects_selected_seq_when_auto_follow_disabled() {
    let mut state = TuiState::new(100);
    state.auto_follow = false;
    state.selected_seq = Some(0);
    state.update(event(
        1,
        1000,
        EventKind::SessionStarted {
            input: "hi".to_string(),
        },
    ));
    assert_eq!(state.selected_seq, Some(0));
}

#[test]
fn update_sets_session_id_once() {
    let mut state = TuiState::new(100);
    state.update(event(
        0,
        1000,
        EventKind::SessionStarted {
            input: "hi".to_string(),
        },
    ));
    state.update(Event {
        id: "e2".to_string(),
        session_id: "s2".to_string(),
        timestamp_ms: 1100,
        seq: 1,
        kind: EventKind::SessionEnded {
            reason: "done".to_string(),
        },
    });
    assert_eq!(state.session_id.as_deref(), Some("s1"));
}

#[test]
fn begin_pending_turn_pushes_user_turn_and_resets_run_state() {
    use crate::canvas::CanvasMessage;

    let mut state = TuiState::new(100);
    state.session_id = Some("old-session".to_string());
    state.continuity_id = Some("thread-1".to_string());
    state.selected_seq = Some(9);
    state.last_error_seq = Some(9);
    state.canvas_scroll_from_bottom = 4;
    state.tools.insert(
        "tool-1".to_string(),
        ToolSummary {
            tool_id: "tool-1".to_string(),
            name: "shell".to_string(),
            args: Value::Null,
            started_seq: 1,
            started_at_ms: 100,
            status: ToolStatus::Running,
            stdout_preview: String::new(),
            stderr_preview: String::new(),
            artifact_ids: BTreeSet::new(),
        },
    );

    state.begin_pending_turn("next step");

    // Per-run fields reset.
    assert_eq!(state.session_id, None);
    assert_eq!(state.selected_seq, None);
    assert_eq!(state.last_error_seq, None);
    assert_eq!(state.canvas_scroll_from_bottom, 0);
    assert_eq!(state.continuity_id.as_deref(), Some("thread-1"));
    assert!(state.awaiting_response);
    assert_eq!(state.pending_prompt.as_deref(), Some("next step"));
    assert_eq!(state.status_message.as_deref(), Some("sending..."));
    // Ambient state persists across turns (Plan Part 4.3): a tool from
    // a previous turn must still be on the TuiState when the next
    // `begin_pending_turn` runs. Canvas messages accumulate — the
    // pre-existing canvas is untouched and a fresh UserTurn is
    // appended to the tail.
    assert!(state.tools.contains_key("tool-1"));
    assert!(matches!(
        state.canvas.messages.last(),
        Some(CanvasMessage::UserTurn { .. })
    ));
}

#[test]
fn focus_ring_walks_focusable_messages_and_toggles_expand_on_cards() {
    use crate::canvas::{Block, CachedText, CanvasMessage, ToolCardStatus};

    let mut state = TuiState::new(100);
    // Build an ambient canvas that resembles a mid-conversation state:
    // user → agent → tool card → job notice (non-focusable) → system notice.
    state.canvas.push_user_turn("user", "tui", "hello", 100);
    state.canvas.messages.push(CanvasMessage::AgentTurn {
        message_id: "a".into(),
        run_session_id: "r".into(),
        agent_id: None,
        role: crate::canvas::AgentRole::Primary,
        actor_id: "agent".into(),
        model: None,
        reasoning_seen: false,
        reasoning_text: String::new(),
        reasoning_summary: String::new(),
        blocks: Vec::new(),
        streaming_tail: String::new(),
        streaming_collector: crate::canvas::StreamCollector::new(),
        streaming: false,
        started_at_ms: 0,
        ended_at_ms: None,
    });
    state.canvas.messages.push(CanvasMessage::ToolCard {
        message_id: "tc".into(),
        tool_id: "t1".into(),
        tool_name: "write".into(),
        args_block: Block::Paragraph(CachedText::empty()),
        status: ToolCardStatus::Running,
        body: Vec::new(),
        expanded: false,
        artifact_ids: Vec::new(),
        started_seq: 0,
        started_at_ms: 0,
    });
    state.canvas.messages.push(CanvasMessage::JobNotice {
        message_id: "jn".into(),
        job_id: "j1".into(),
        job_kind: "compaction".into(),
        details: None,
        status: crate::canvas::JobLifecycle::Running,
        actor_id: "user".into(),
        origin: "cli".into(),
        started_at_ms: None,
        ended_at_ms: None,
    });
    state.canvas.messages.push(CanvasMessage::SystemNotice {
        message_id: "sn".into(),
        level: crate::canvas::NoticeLevel::Warn,
        text: "warn".into(),
        origin_event_kind: "x".into(),
        seq: 0,
    });

    // Forward walk skips JobNotice.
    state.focus_next_message();
    assert_eq!(state.focused_message_id.as_deref(), Some("m000000")); // UserTurn
    assert!(!state.auto_follow);
    state.focus_next_message();
    assert_eq!(state.focused_message_id.as_deref(), Some("a")); // AgentTurn
    state.focus_next_message();
    assert_eq!(state.focused_message_id.as_deref(), Some("tc")); // ToolCard
    state.focus_next_message();
    assert_eq!(state.focused_message_id.as_deref(), Some("sn")); // SystemNotice (skipped JobNotice)
    state.focus_next_message();
    assert_eq!(state.focused_message_id.as_deref(), Some("m000000")); // wraps

    // Expand toggles only on cards.
    state.focused_message_id = Some("tc".into());
    assert!(state.toggle_focused_card_expanded());
    match &state.canvas.messages[2] {
        CanvasMessage::ToolCard { expanded, .. } => assert!(expanded),
        _ => unreachable!(),
    }
    state.focused_message_id = Some("a".into());
    assert!(!state.toggle_focused_card_expanded());

    // Backwards walk.
    state.focused_message_id = Some("tc".into());
    state.focus_prev_message();
    assert_eq!(state.focused_message_id.as_deref(), Some("a"));

    // Clearing drops focus outright.
    state.clear_focus();
    assert!(state.focused_message_id.is_none());
}

#[test]
fn rendered_agent_text_includes_reasoning_only_when_visible() {
    use crate::canvas::{Block, CachedText, CanvasMessage};

    let mut state = TuiState::new(80);
    state.canvas.messages.push(CanvasMessage::AgentTurn {
        message_id: "a1".into(),
        run_session_id: "run-1".into(),
        agent_id: None,
        role: crate::canvas::AgentRole::Primary,
        actor_id: "agent".into(),
        model: Some("gpt".into()),
        reasoning_seen: true,
        reasoning_text: "private chain".into(),
        reasoning_summary: "safe summary".into(),
        blocks: vec![Block::Paragraph(CachedText::plain("final answer"))],
        streaming_tail: String::new(),
        streaming_collector: crate::canvas::StreamCollector::new(),
        streaming: false,
        started_at_ms: 0,
        ended_at_ms: Some(1),
    });

    let visible = state.rendered_agent_text();
    assert!(visible.contains("Reasoning summary"));
    assert!(visible.contains("safe summary"));
    assert!(visible.contains("final answer"));

    state.reasoning_visible = false;
    let hidden = state.rendered_agent_text();
    assert!(!hidden.contains("Reasoning summary"));
    assert!(!hidden.contains("safe summary"));
    assert!(hidden.contains("final answer"));
}

#[test]
fn rendered_agent_text_explains_hidden_reasoning_when_provider_returns_no_summary() {
    use crate::canvas::{Block, CachedText, CanvasMessage};

    let mut state = TuiState::new(80);
    state.canvas.messages.push(CanvasMessage::AgentTurn {
        message_id: "a2".into(),
        run_session_id: "run-2".into(),
        agent_id: None,
        role: crate::canvas::AgentRole::Primary,
        actor_id: "agent".into(),
        model: Some("gpt".into()),
        reasoning_seen: true,
        reasoning_text: String::new(),
        reasoning_summary: String::new(),
        blocks: vec![Block::Paragraph(CachedText::plain("final answer"))],
        streaming_tail: String::new(),
        streaming_collector: crate::canvas::StreamCollector::new(),
        streaming: false,
        started_at_ms: 0,
        ended_at_ms: Some(1),
    });

    let visible = state.rendered_agent_text();
    assert!(visible.contains("Reasoning"));
    assert!(visible.contains("provider did not return a visible summary"));
    assert!(visible.contains("final answer"));
}

#[test]
fn set_continuity_id_updates_state() {
    let mut state = TuiState::new(100);
    state.set_continuity_id("thread-2");
    assert_eq!(state.continuity_id.as_deref(), Some("thread-2"));
}

#[test]
fn session_started_does_not_duplicate_pending_prompt() {
    use crate::canvas::CanvasMessage;

    let mut state = TuiState::new(100);
    state.begin_pending_turn("hello");
    let user_turns_before = state
        .canvas
        .messages
        .iter()
        .filter(|m| matches!(m, CanvasMessage::UserTurn { .. }))
        .count();

    state.update(event(
        0,
        1000,
        EventKind::SessionStarted {
            input: "hello".to_string(),
        },
    ));

    let user_turns_after = state
        .canvas
        .messages
        .iter()
        .filter(|m| matches!(m, CanvasMessage::UserTurn { .. }))
        .count();
    assert_eq!(user_turns_before, user_turns_after);
    assert_eq!(state.pending_prompt, None);
    assert!(state.awaiting_response);
}

#[test]
fn canvas_scroll_helpers_clamp_at_zero() {
    let mut state = TuiState::default();
    state.scroll_canvas_up(12);
    assert!(!state.auto_follow);
    state.scroll_canvas_down(5);
    assert_eq!(state.canvas_scroll_from_bottom, 7);
    assert!(!state.auto_follow);

    state.scroll_canvas_down(99);
    assert_eq!(state.canvas_scroll_from_bottom, 0);
    assert!(state.auto_follow);

    state.scroll_canvas_up(5);
    state.set_focused_message("m000001");
    state.scroll_canvas_to_bottom();
    assert_eq!(state.canvas_scroll_from_bottom, 0);
    assert!(state.auto_follow);
    assert!(!state.focus_reveal_pending());
}

fn artifact(fill: char) -> String {
    std::iter::repeat_n(fill, 64).collect()
}

#[test]
fn overlay_and_status_helpers_toggle_cleanly() {
    let mut state = TuiState::default();
    assert_eq!(state.output_view.as_str(), "rendered");
    assert_eq!(state.theme.as_str(), "default-dark");

    state.toggle_output_view();
    state.toggle_theme();
    assert_eq!(state.output_view.as_str(), "raw");
    assert_eq!(state.theme.as_str(), "default-light");

    state.toggle_activity_overlay();
    assert_eq!(state.overlay(), &Overlay::Activity);
    state.toggle_activity_overlay();
    assert_eq!(state.overlay(), &Overlay::None);

    state.toggle_tasks_overlay();
    assert_eq!(state.overlay(), &Overlay::TaskList);
    state.close_overlay();
    assert_eq!(state.overlay(), &Overlay::None);

    state.set_status_message("watching");
    state.set_now_ms(2_000);
    assert!(!state.is_stalled(100));
    state.last_event_ms = Some(1_500);
    assert!(state.is_stalled(400));
    assert!(!state.is_stalled(600));
    state.end_ms = Some(1_600);
    assert!(!state.is_stalled(400));
}

#[test]
fn palette_helpers_filter_select_and_allow_custom_routes() {
    let mut state = TuiState::default();
    state.open_palette(
        PaletteMode::Model,
        PaletteOrigin::TopCenter,
        vec![
            PaletteEntry {
                value: "openrouter/openai/gpt-oss-20b".to_string(),
                title: "openrouter/openai/gpt-oss-20b".to_string(),
                subtitle: Some("OpenRouter".to_string()),
                chips: vec!["current".to_string()],
            },
            PaletteEntry {
                value: "openai/gpt-5-nano-2025-08-07".to_string(),
                title: "openai/gpt-5-nano-2025-08-07".to_string(),
                subtitle: Some("OpenAI".to_string()),
                chips: vec![],
            },
        ],
        "no models",
        true,
        "Use typed route",
    );
    assert!(state.is_palette_open());
    assert_eq!(
        state.palette_selected_value().as_deref(),
        Some("openrouter/openai/gpt-oss-20b")
    );

    state.palette_move_selection(1);
    assert_eq!(
        state.palette_selected_value().as_deref(),
        Some("openai/gpt-5-nano-2025-08-07")
    );

    state.palette_push_char('n');
    state.palette_push_char('a');
    state.palette_push_char('n');
    state.palette_push_char('o');
    assert_eq!(state.palette_query(), Some("nano"));
    assert_eq!(
        state.palette_selected_value().as_deref(),
        Some("openai/gpt-5-nano-2025-08-07")
    );

    for _ in 0..4 {
        state.palette_backspace();
    }
    state.palette_push_char('c');
    state.palette_push_char('u');
    state.palette_push_char('s');
    state.palette_push_char('t');
    state.palette_push_char('o');
    state.palette_push_char('m');
    state.palette_push_char('/');
    state.palette_push_char('m');
    state.palette_push_char('o');
    state.palette_push_char('d');
    state.palette_push_char('e');
    state.palette_push_char('l');
    assert_eq!(
        state.palette_selected_value().as_deref(),
        Some("custom/model")
    );
}

#[test]
fn open_selected_detail_prefers_errors_and_toggles_tool_and_task_details() {
    let mut state = TuiState::default();
    state.update(event(
        0,
        100,
        EventKind::ToolStarted {
            tool_id: "tool-1".to_string(),
            name: "ls".to_string(),
            args: json!({"path": "."}),
            timeout_ms: None,
        },
    ));
    state.update(event(
        1,
        110,
        EventKind::ToolTaskSpawned {
            task_id: "task-1".to_string(),
            tool_name: "shell".to_string(),
            args: json!({"cmd": "pwd"}),
            cwd: None,
            title: Some("pwd".to_string()),
            execution_mode: ToolTaskExecutionMode::Pty,
            origin_session_id: None,
            artifacts: None,
        },
    ));
    state.update(event(
        2,
        120,
        EventKind::SessionStarted {
            input: "hello".to_string(),
        },
    ));

    state.last_error_seq = Some(99);
    state.open_selected_detail();
    assert_eq!(state.overlay(), &Overlay::ErrorDetail { seq: 99 });
    state.open_selected_detail();
    assert_eq!(state.overlay(), &Overlay::None);

    state.last_error_seq = None;
    state.selected_seq = Some(0);
    state.open_selected_detail();
    assert_eq!(
        state.overlay(),
        &Overlay::ToolDetail {
            tool_id: "tool-1".to_string()
        }
    );
    state.open_selected_detail();
    assert_eq!(state.overlay(), &Overlay::None);

    state.selected_seq = Some(1);
    state.open_selected_detail();
    assert_eq!(
        state.overlay(),
        &Overlay::TaskDetail {
            task_id: "task-1".to_string()
        }
    );
    state.open_selected_detail();
    assert_eq!(state.overlay(), &Overlay::None);

    state.selected_seq = Some(2);
    state.open_selected_detail();
    assert_eq!(state.overlay(), &Overlay::None);
}

#[test]
fn update_tracks_timings_derived_state_and_artifacts() {
    let mut state = TuiState::new(100);
    let a1 = artifact('a');
    let a2 = artifact('b');
    let a3 = artifact('c');
    let a4 = artifact('d');
    let a5 = artifact('e');
    let a6 = artifact('f');
    let a7 = artifact('1');

    for event in [
        event(
            0,
            100,
            EventKind::SessionStarted {
                input: "hello".to_string(),
            },
        ),
        event(
            1,
            110,
            EventKind::OpenResponsesRequestStarted {
                endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
                model: Some("gpt-5".to_string()),
                request_index: 0,
                kind: "response.create".to_string(),
            },
        ),
        event(
            2,
            120,
            EventKind::OpenResponsesResponseHeaders {
                request_index: 0,
                status: 200,
                request_id: Some("req_123".to_string()),
                content_type: Some("text/event-stream".to_string()),
            },
        ),
        event(
            3,
            130,
            EventKind::OpenResponsesResponseFirstByte { request_index: 0 },
        ),
        event(
            4,
            140,
            EventKind::ProviderEvent {
                provider: "openresponses".to_string(),
                status: ProviderEventStatus::InvalidJson,
                event_name: None,
                data: None,
                raw: Some("{".to_string()),
                errors: vec!["bad json".to_string()],
                response_errors: vec!["schema".to_string()],
            },
        ),
        event(
            5,
            150,
            EventKind::OutputTextDelta {
                delta: "world".to_string(),
            },
        ),
        event(
            6,
            160,
            EventKind::ToolStarted {
                tool_id: "tool-1".to_string(),
                name: "write".to_string(),
                args: json!({"path": "notes.md"}),
                timeout_ms: Some(1000),
            },
        ),
        event(
            7,
            165,
            EventKind::ToolStdout {
                tool_id: "tool-1".to_string(),
                chunk: "stdout".to_string(),
            },
        ),
        event(
            8,
            170,
            EventKind::ToolStderr {
                tool_id: "tool-1".to_string(),
                chunk: "stderr".to_string(),
            },
        ),
        event(
            9,
            180,
            EventKind::ToolEnded {
                tool_id: "tool-1".to_string(),
                exit_code: 0,
                duration_ms: 20,
                artifacts: Some(json!({"artifact_id": a1, "nested": [a2, "ignore"]})),
            },
        ),
        event(
            10,
            185,
            EventKind::ToolStarted {
                tool_id: "tool-2".to_string(),
                name: "shell".to_string(),
                args: json!({"cmd": "sleep 1"}),
                timeout_ms: None,
            },
        ),
        event(
            11,
            190,
            EventKind::ToolFailed {
                tool_id: "tool-2".to_string(),
                error: "boom".to_string(),
            },
        ),
        event(
            12,
            200,
            EventKind::ToolTaskSpawned {
                task_id: "task-1".to_string(),
                tool_name: "shell".to_string(),
                args: json!({"cmd": "pwd"}),
                cwd: Some("/tmp".to_string()),
                title: Some("pwd".to_string()),
                execution_mode: ToolTaskExecutionMode::Pty,
                origin_session_id: Some("s1".to_string()),
                artifacts: Some(json!({"artifact": a3})),
            },
        ),
        event(
            13,
            205,
            EventKind::ToolTaskOutputDelta {
                task_id: "task-1".to_string(),
                stream: ToolTaskStream::Stdout,
                chunk: "line one".to_string(),
                artifacts: Some(json!([a4])),
            },
        ),
        event(
            14,
            206,
            EventKind::ToolTaskOutputDelta {
                task_id: "task-1".to_string(),
                stream: ToolTaskStream::Stderr,
                chunk: "warn".to_string(),
                artifacts: None,
            },
        ),
        event(
            15,
            207,
            EventKind::ToolTaskOutputDelta {
                task_id: "task-1".to_string(),
                stream: ToolTaskStream::Pty,
                chunk: "pty".to_string(),
                artifacts: None,
            },
        ),
        event(
            16,
            210,
            EventKind::ToolTaskStatus {
                task_id: "task-1".to_string(),
                status: ToolTaskStatus::Running,
                exit_code: None,
                started_at_ms: Some(205),
                ended_at_ms: None,
                artifacts: None,
                error: None,
            },
        ),
        event(
            17,
            220,
            EventKind::ToolTaskStatus {
                task_id: "task-2".to_string(),
                status: ToolTaskStatus::Failed,
                exit_code: Some(9),
                started_at_ms: Some(219),
                ended_at_ms: Some(220),
                artifacts: Some(json!({"artifact": a5})),
                error: Some("failed".to_string()),
            },
        ),
        event(
            18,
            230,
            EventKind::ContinuityJobSpawned {
                job_id: "job-1".to_string(),
                job_kind: "compaction".to_string(),
                details: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ),
        event(
            19,
            240,
            EventKind::ContinuityJobEnded {
                job_id: "job-2".to_string(),
                job_kind: "audit".to_string(),
                status: "completed".to_string(),
                result: None,
                error: Some("none".to_string()),
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ),
        event(
            20,
            250,
            EventKind::ContinuityContextSelectionDecided {
                run_session_id: "run-1".to_string(),
                message_id: "m1".to_string(),
                compiler_id: "rip.context_compiler.v1".to_string(),
                compiler_strategy: "recent_messages_v1".to_string(),
                limits: json!({"recent_messages_v1_limit": 8}),
                compaction_checkpoint: None,
                compaction_checkpoints: Vec::new(),
                resets: Vec::new(),
                reason: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ),
        event(
            21,
            255,
            EventKind::ContinuityContextCompiled {
                run_session_id: "run-1".to_string(),
                bundle_artifact_id: a6.clone(),
                compiler_id: "rip.context_compiler.v1".to_string(),
                compiler_strategy: "recent_messages_v1".to_string(),
                from_seq: 1,
                from_message_id: Some("m1".to_string()),
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ),
        event(
            22,
            260,
            EventKind::ContinuityCompactionCheckpointCreated {
                checkpoint_id: "ckpt-1".to_string(),
                cut_rule_id: "stride_messages_v1".to_string(),
                summary_kind: "cumulative_v1".to_string(),
                summary_artifact_id: a7.clone(),
                from_seq: 1,
                from_message_id: Some("m1".to_string()),
                to_seq: 5,
                to_message_id: Some("m5".to_string()),
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ),
        event(
            23,
            265,
            EventKind::OpenResponsesRequest {
                endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
                model: Some("gpt-5".to_string()),
                request_index: 0,
                kind: "response.create".to_string(),
                body_artifact_id: artifact('9'),
                body_bytes: 12,
                total_bytes: 12,
                truncated: false,
            },
        ),
    ] {
        state.update(event);
    }

    assert_eq!(state.session_id.as_deref(), Some("s1"));
    assert_eq!(state.ttft_ms(), Some(50));
    assert_eq!(state.e2e_ms(), Some(120));
    assert_eq!(state.openresponses_headers_ms(), Some(10));
    assert_eq!(state.openresponses_first_byte_ms(), Some(20));
    assert_eq!(state.openresponses_first_provider_event_ms(), Some(30));
    assert_eq!(
        state.openresponses_endpoint.as_deref(),
        Some("https://openrouter.ai/api/v1/responses")
    );
    assert_eq!(state.openresponses_model.as_deref(), Some("gpt-5"));
    assert_eq!(state.selected_seq, Some(23));
    assert!(state.has_error());
    assert_eq!(state.last_error_seq, Some(17));
    // The canvas model holds the UserTurn (materialized from
    // `SessionStarted.input` since no `begin_pending_turn` fired) and
    // agent-facing text deltas feed into the current AgentTurn via
    // the StreamCollector (B.5).
    use crate::canvas::{Block, CachedText, CanvasMessage};
    let has_user_turn = state
        .canvas
        .messages
        .iter()
        .any(|m| matches!(m, CanvasMessage::UserTurn { .. }));
    assert!(has_user_turn);
    let agent_has_world = state.canvas.messages.iter().any(|m| match m {
        CanvasMessage::AgentTurn {
            blocks,
            streaming_tail,
            ..
        } => {
            streaming_tail.contains("world")
                || blocks.iter().any(|b| match b {
                    Block::Paragraph(CachedText { text, .. }) => text
                        .lines
                        .iter()
                        .any(|l| l.spans.iter().any(|s| s.content.contains("world"))),
                    _ => false,
                })
        }
        _ => false,
    });
    assert!(agent_has_world);

    let tool1 = state.tools.get("tool-1").expect("tool-1");
    assert_eq!(tool1.stdout_preview, "stdout");
    assert_eq!(tool1.stderr_preview, "stderr");
    assert!(matches!(
        tool1.status,
        ToolStatus::Ended {
            exit_code: 0,
            duration_ms: 20
        }
    ));
    assert!(tool1.artifact_ids.contains(&artifact('a')));
    assert!(tool1.artifact_ids.contains(&artifact('b')));

    let tool2 = state.tools.get("tool-2").expect("tool-2");
    assert!(matches!(
        &tool2.status,
        ToolStatus::Failed { error } if error == "boom"
    ));

    let task1 = state.tasks.get("task-1").expect("task-1");
    assert_eq!(task1.tool_name, "shell");
    assert_eq!(task1.stdout_preview, "line one");
    assert_eq!(task1.stderr_preview, "warn");
    assert_eq!(task1.pty_preview, "pty");
    assert_eq!(task1.status, ToolTaskStatus::Running);
    assert!(task1.artifact_ids.contains(&artifact('c')));
    assert!(task1.artifact_ids.contains(&artifact('d')));

    let task2 = state.tasks.get("task-2").expect("task-2");
    assert_eq!(task2.tool_name, "unknown");
    assert_eq!(task2.status, ToolTaskStatus::Failed);
    assert_eq!(task2.exit_code, Some(9));
    assert_eq!(task2.error.as_deref(), Some("failed"));
    assert!(task2.artifact_ids.contains(&artifact('e')));

    assert_eq!(
        state.running_tool_ids().collect::<Vec<_>>(),
        Vec::<&str>::new()
    );
    assert_eq!(state.running_task_ids().collect::<Vec<_>>(), vec!["task-1"]);
    assert_eq!(state.running_job_ids().collect::<Vec<_>>(), vec!["job-1"]);

    assert!(matches!(
        state.jobs.get("job-1").expect("job-1").status,
        JobStatus::Running
    ));
    assert!(matches!(
        &state.jobs.get("job-2").expect("job-2").status,
        JobStatus::Ended { status, error }
            if status == "completed" && error.as_deref() == Some("none")
    ));
    assert!(matches!(
        &state.context,
        Some(ContextSummary {
            run_session_id,
            compiler_strategy,
            status: ContextStatus::Compiled,
            bundle_artifact_id: Some(bundle),
        }) if run_session_id == "run-1"
            && compiler_strategy == "recent_messages_v1"
            && bundle == &a6
    ));

    for artifact_id in [
        artifact('a'),
        artifact('b'),
        artifact('c'),
        artifact('d'),
        artifact('e'),
        a6,
        a7,
        artifact('9'),
    ] {
        assert!(state.artifacts.contains(&artifact_id));
    }
}

#[test]
fn helper_functions_handle_errors_artifacts_and_utf8_boundaries() {
    assert!(is_error_event(&EventKind::ToolFailed {
        tool_id: "tool-1".to_string(),
        error: "boom".to_string(),
    }));
    assert!(is_error_event(&EventKind::CheckpointFailed {
        action: CheckpointAction::Create,
        error: "bad".to_string(),
    }));
    assert!(is_error_event(&EventKind::ToolTaskStatus {
        task_id: "task-1".to_string(),
        status: ToolTaskStatus::Failed,
        exit_code: None,
        started_at_ms: None,
        ended_at_ms: None,
        artifacts: None,
        error: None,
    }));
    assert!(is_error_event(&EventKind::ProviderEvent {
        provider: "openresponses".to_string(),
        status: ProviderEventStatus::Event,
        event_name: None,
        data: None,
        raw: None,
        errors: vec!["oops".to_string()],
        response_errors: Vec::new(),
    }));
    assert!(!is_error_event(&EventKind::ProviderEvent {
        provider: "openresponses".to_string(),
        status: ProviderEventStatus::Done,
        event_name: None,
        data: None,
        raw: None,
        errors: Vec::new(),
        response_errors: vec!["warning".to_string()],
    }));
    assert!(!is_error_event(&EventKind::SessionEnded {
        reason: "ok".to_string(),
    }));

    let mut preview = String::new();
    push_preview(&mut preview, "", 6);
    push_preview(&mut preview, "ab😀cd😀ef", 6);
    assert!(preview.is_char_boundary(preview.len()));
    assert!(preview.len() <= 8);

    let ids = extract_artifact_ids(&json!({
        "one": artifact('a'),
        "nested": [artifact('b'), {"deep": artifact('c')}],
        "ignore": "short"
    }));
    assert_eq!(ids.len(), 3);
    assert!(looks_like_artifact_id(&artifact('f')));
    assert!(!looks_like_artifact_id("artifact"));
}

#[test]
fn thread_picker_state_selection_wraps_and_clamps() {
    let mut picker = ThreadPickerState::new(vec![
        ThreadPickerEntry {
            thread_id: "cont-1".into(),
            title: "one".into(),
            preview: "…".into(),
            chips: vec![],
        },
        ThreadPickerEntry {
            thread_id: "cont-2".into(),
            title: "two".into(),
            preview: "…".into(),
            chips: vec![],
        },
        ThreadPickerEntry {
            thread_id: "cont-3".into(),
            title: "three".into(),
            preview: "…".into(),
            chips: vec![],
        },
    ]);
    assert_eq!(picker.selected, 0);
    picker.move_selection(1);
    assert_eq!(picker.selected, 1);
    picker.move_selection(5);
    assert_eq!(picker.selected, 2, "move past end clamps to last");
    picker.move_selection(-10);
    assert_eq!(picker.selected, 0, "move past start clamps to first");
    assert_eq!(
        picker.selected_entry().map(|e| e.thread_id.as_str()),
        Some("cont-1")
    );

    // Empty picker is a no-op.
    let mut empty = ThreadPickerState::new(vec![]);
    empty.move_selection(3);
    assert_eq!(empty.selected, 0);
    assert!(empty.selected_entry().is_none());
}

#[test]
fn thread_picker_state_helpers_round_trip_through_overlay_stack() {
    let mut state = TuiState::default();
    assert!(!state.is_thread_picker_open());
    assert!(state.thread_picker_selected_value().is_none());

    state.open_thread_picker(vec![
        ThreadPickerEntry {
            thread_id: "cont-a".into(),
            title: "alpha".into(),
            preview: "…".into(),
            chips: vec![],
        },
        ThreadPickerEntry {
            thread_id: "cont-b".into(),
            title: "beta".into(),
            preview: "…".into(),
            chips: vec![],
        },
    ]);
    assert!(state.is_thread_picker_open());
    assert_eq!(
        state.thread_picker_selected_value().as_deref(),
        Some("cont-a")
    );

    state.thread_picker_move_selection(1);
    assert_eq!(
        state.thread_picker_selected_value().as_deref(),
        Some("cont-b")
    );

    state.close_overlay();
    assert!(!state.is_thread_picker_open());
    assert!(state.thread_picker_selected_value().is_none());
}

#[test]
fn overlay_scroll_resets_when_overlay_changes_or_conversation_resets() {
    let mut state = TuiState::default();
    state.set_overlay(Overlay::Help);
    state.overlay_scroll = 12;
    state.push_overlay(Overlay::Debug);
    assert_eq!(state.overlay_scroll, 0);

    state.scroll_overlay_down(7);
    assert_eq!(state.overlay_scroll, 7);
    state.pop_overlay();
    assert_eq!(state.overlay_scroll, 0);

    state.scroll_overlay_down(9);
    state.reset_conversation_state();
    assert_eq!(state.overlay_scroll, 0);
    assert_eq!(state.overlay(), &Overlay::None);
}

#[test]
fn overlay_classification_distinguishes_scrollable_and_non_scrollable_overlays() {
    let mut state = TuiState::default();
    assert!(!state.overlay_owns_input());
    assert!(!state.overlay_is_scrollable());

    state.set_overlay(Overlay::Help);
    assert!(state.overlay_owns_input());
    assert!(state.overlay_is_scrollable());

    state.set_overlay(Overlay::ErrorRecovery { seq: 7 });
    assert!(state.overlay_owns_input());
    assert!(!state.overlay_is_scrollable());
}

#[test]
fn palette_origin_is_none_when_no_palette_is_open() {
    let mut state = TuiState::default();
    assert!(state.palette_origin().is_none());

    state.open_palette(
        PaletteMode::Command,
        PaletteOrigin::TopCenter,
        vec![],
        "No matches".to_string(),
        false,
        String::new(),
    );
    assert_eq!(state.palette_origin(), Some(PaletteOrigin::TopCenter));

    state.close_overlay();
    assert!(state.palette_origin().is_none());
}
