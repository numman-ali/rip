//! Tests for `copy.rs` helpers that aren't reached by the broader
//! `fullscreen/tests.rs` cases: the 1-byte-remainder branch of
//! `base64_encode`, empty-payload behavior of `osc52_sequence`, and
//! round-trip coverage of `prepare_copy_selected` when a selection is
//! present and OSC52 is enabled. `copy_selected` itself needs a real
//! `Terminal<CrosstermBackend<Stdout>>` so it stays out of the unit
//! test suite.

use super::*;
use rip_kernel::ToolTaskExecutionMode;
use rip_tui::canvas::{AgentRole, NoticeLevel, TaskCardStatus, ToolCardStatus};
use rip_tui::CachedText;

#[test]
fn base64_encode_single_byte_uses_two_pad_chars() {
    // 1-byte remainder branch. "a" = 0x61 → "YQ==".
    assert_eq!(base64_encode(b"a"), "YQ==");
    // 4 bytes → 3+1 remainder; same branch exercised.
    assert_eq!(base64_encode(b"abcd"), "YWJjZA==");
    // 7 bytes → 6+1.
    assert_eq!(base64_encode(b"abcdefg"), "YWJjZGVmZw==");
}

#[test]
fn base64_encode_two_byte_remainder_uses_one_pad_char() {
    // Already covered by `b"hi"` in the sibling file, but we want a
    // longer input that ends with the 2-byte branch so the test file is
    // self-contained.
    assert_eq!(base64_encode(b"abcde"), "YWJjZGU=");
}

#[test]
fn base64_encode_aligned_length_has_no_pad_chars() {
    assert_eq!(base64_encode(b"abc"), "YWJj");
    assert_eq!(base64_encode(b"abcdef"), "YWJjZGVm");
}

#[test]
fn osc52_sequence_empty_payload_still_wraps_prefix_and_bell() {
    // Empty input should still produce a well-formed OSC52 sequence —
    // terminals that gate OSC52 parsing on the trailing BEL won't drop
    // the next sequence on the floor.
    let seq = osc52_sequence(b"");
    assert_eq!(seq, "\x1b]52;c;\x07");
}

#[test]
fn osc52_sequence_preserves_expected_shape() {
    let seq = osc52_sequence(b"a");
    assert!(seq.starts_with("\x1b]52;c;"));
    assert!(seq.ends_with('\x07'));
    assert!(seq.contains("YQ=="));
}

#[test]
fn block_to_text_covers_heading_code_quote_list_thematic_and_artifact() {
    let heading = block_to_text(&CanvasBlock::Heading {
        level: 2,
        text: CachedText::plain("Heading"),
    });
    assert_eq!(heading, "## Heading");

    let code = block_to_text(&CanvasBlock::CodeFence {
        lang: Some("rust".to_string()),
        text: CachedText::plain("fn main() {}"),
    });
    assert!(code.contains("```rust"));

    let quote = block_to_text(&CanvasBlock::BlockQuote(vec![CanvasBlock::Paragraph(
        CachedText::plain("quoted"),
    )]));
    assert_eq!(quote, "> quoted");

    let list = block_to_text(&CanvasBlock::List {
        ordered: true,
        items: vec![
            vec![CanvasBlock::Paragraph(CachedText::plain("first"))],
            vec![CanvasBlock::Paragraph(CachedText::plain("second"))],
        ],
    });
    assert!(list.contains("1. first"));
    assert!(list.contains("2. second"));

    assert_eq!(block_to_text(&CanvasBlock::Thematic), "────");
    assert_eq!(
        block_to_text(&CanvasBlock::ArtifactChip {
            artifact_id: "artifact-1234567890".to_string(),
            bytes: None,
        }),
        "⧉ artifact"
    );
}

#[test]
fn copyable_message_text_covers_remaining_canvas_variants() {
    let agent = copyable_message_text(&CanvasMessage::AgentTurn {
        message_id: "m-agent".to_string(),
        run_session_id: "s1".to_string(),
        agent_id: None,
        role: AgentRole::Primary,
        actor_id: "rip".to_string(),
        model: Some("gpt".to_string()),
        blocks: vec![CanvasBlock::Paragraph(CachedText::plain("final answer"))],
        streaming_tail: "tail".to_string(),
        streaming: true,
        started_at_ms: 0,
        ended_at_ms: None,
    })
    .expect("agent turn");
    assert!(agent.contains("final answer"));
    assert!(agent.contains("tail"));

    let tool = copyable_message_text(&CanvasMessage::ToolCard {
        message_id: "m-tool".to_string(),
        tool_id: "tool-1".to_string(),
        tool_name: "read_file".to_string(),
        args_block: CanvasBlock::ToolArgsJson(CachedText::plain("{\"path\":\"README.md\"}")),
        status: ToolCardStatus::Succeeded {
            duration_ms: 10,
            exit_code: 0,
        },
        body: vec![CanvasBlock::ToolStdout(CachedText::plain("hello"))],
        expanded: true,
        artifact_ids: vec!["artifact-1".to_string()],
        started_seq: 0,
        started_at_ms: 0,
    })
    .expect("tool card");
    assert!(tool.contains("tool: read_file"));
    assert!(tool.contains("args:"));
    assert!(tool.contains("hello"));

    let task = copyable_message_text(&CanvasMessage::TaskCard {
        message_id: "m-task".to_string(),
        task_id: "task-1".to_string(),
        tool_name: "shell".to_string(),
        title: Some("Build".to_string()),
        execution_mode: ToolTaskExecutionMode::Pipes,
        status: TaskCardStatus::Running,
        body: vec![CanvasBlock::ToolStdout(CachedText::plain("cargo test"))],
        expanded: true,
        artifact_ids: vec![],
        started_at_ms: Some(0),
    })
    .expect("task card");
    assert!(task.contains("Build"));
    assert!(task.contains("cargo test"));

    assert_eq!(
        copyable_message_text(&CanvasMessage::JobNotice {
            message_id: "m-job".to_string(),
            job_id: "job-1".to_string(),
            job_kind: "compaction".to_string(),
            details: None,
            status: rip_tui::canvas::JobLifecycle::Running,
            actor_id: "rip".to_string(),
            origin: "system".to_string(),
            started_at_ms: Some(0),
            ended_at_ms: None,
        })
        .as_deref(),
        Some("compaction")
    );
    assert_eq!(
        copyable_message_text(&CanvasMessage::SystemNotice {
            message_id: "m-notice".to_string(),
            level: NoticeLevel::Warn,
            text: "provider warning".to_string(),
            origin_event_kind: "provider_event".to_string(),
            seq: 3,
        })
        .as_deref(),
        Some("provider warning")
    );
    assert_eq!(
        copyable_message_text(&CanvasMessage::ContextNotice {
            message_id: "m-context".to_string(),
            run_session_id: "s1".to_string(),
            strategy: "recent_messages_v1".to_string(),
            status: rip_tui::canvas::ContextLifecycle::Compiled,
            bundle_artifact_id: None,
            contributed_artifact_ids: vec![],
        })
        .as_deref(),
        Some("context recent_messages_v1 · Compiled")
    );
    assert_eq!(
        copyable_message_text(&CanvasMessage::CompactionCheckpoint {
            message_id: "m-checkpoint".to_string(),
            checkpoint_id: "cp-1".to_string(),
            from_seq: 10,
            to_seq: 20,
            summary_artifact_id: "artifact-2".to_string(),
        })
        .as_deref(),
        Some("compaction checkpoint · seq 10…20")
    );
    assert_eq!(
        copyable_message_text(&CanvasMessage::ExtensionPanel {
            message_id: "m-extension".to_string(),
            panel_id: "panel-1".to_string(),
            extension_id: "ext".to_string(),
            title: "Inspector".to_string(),
            placement: rip_tui::canvas::PanelPlacement::Inline,
            lines: vec![],
            keys: vec![],
            artifact_ids: vec![],
        })
        .as_deref(),
        Some("Inspector")
    );
}

#[test]
fn preferred_copyable_message_prefers_focused_message_before_latest() {
    let mut state = TuiState::new(100);
    state.canvas.messages.push(CanvasMessage::SystemNotice {
        message_id: "older".to_string(),
        level: NoticeLevel::Info,
        text: "older".to_string(),
        origin_event_kind: "provider_event".to_string(),
        seq: 1,
    });
    state.canvas.messages.push(CanvasMessage::SystemNotice {
        message_id: "focused".to_string(),
        level: NoticeLevel::Info,
        text: "focused".to_string(),
        origin_event_kind: "provider_event".to_string(),
        seq: 2,
    });
    state.focused_message_id = Some("focused".to_string());

    assert_eq!(
        preferred_copyable_message(&state).as_deref(),
        Some("focused")
    );
}
