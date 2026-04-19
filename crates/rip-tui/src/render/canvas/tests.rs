use super::super::theme::ThemeStyles;
use super::content::{
    artifact_chip_lines, block_as_lines, message_body_lines, message_glyph, plain_text,
    render_blocks,
};
use super::*;
use crate::canvas::{
    AgentRole, Block as CanvasBlock, CachedText, CanvasMessage, ContextLifecycle, JobLifecycle,
    NoticeLevel, PanelPlacement, StyledLine,
};
use crate::ThemeId;

#[test]
fn canvas_hit_message_id_tracks_visible_rows() {
    let mut state = TuiState::new(10);
    let first = state.canvas.push_user_turn("user", "tui", "hello", 0);
    let second = state.canvas.push_user_turn("user", "tui", "world", 1);

    assert_eq!(
        canvas_hit_message_id(&state, 40, 8, 0).as_deref(),
        Some(first.as_str())
    );
    assert_eq!(
        canvas_hit_message_id(&state, 40, 8, 2).as_deref(),
        Some(second.as_str())
    );
}

#[test]
fn format_card_top_line_fills_dashes_when_meta_and_title_fit() {
    let top = format_card_top_line("write", Some("✓ 120ms"), 40);
    assert!(top.starts_with("╭─ write "));
    assert!(top.ends_with(" ✓ 120ms ─╮"));
    assert_eq!(top.chars().count(), 40);
}

#[test]
fn format_card_top_line_degrades_gracefully_when_too_narrow() {
    let top = format_card_top_line("tool_with_long_name", Some("✓ 120ms"), 10);
    // Too narrow to fill — just concatenate.
    assert!(top.starts_with("╭─ tool_with_long_name "));
    assert!(top.ends_with("─╮"));
}

#[test]
fn format_card_bottom_line_matches_width() {
    let bot = format_card_bottom_line(10);
    assert_eq!(bot, "╰────────╯");
    assert_eq!(bot.chars().count(), 10);
}

#[test]
fn motion_thinking_glyph_cycles_every_four_hundred_ms() {
    let mut ctx = MotionCtx {
        now_ms: 0,
        ..MotionCtx::default()
    };
    assert_eq!(ctx.thinking_glyph(), "◐");
    ctx.now_ms = 400;
    assert_eq!(ctx.thinking_glyph(), "◓");
    ctx.now_ms = 800;
    assert_eq!(ctx.thinking_glyph(), "◑");
    ctx.now_ms = 1200;
    assert_eq!(ctx.thinking_glyph(), "◒");
    ctx.now_ms = 1600; // wraps back to ◐
    assert_eq!(ctx.thinking_glyph(), "◐");
}

#[test]
fn subagent_slot_is_deterministic_and_bounded_by_four() {
    // D.4: the subagent palette has 4 accent slots. Distinct parent
    // run ids must map to *some* slot in [0, 4); identical ids must
    // always map to the same slot so a subagent's color is stable
    // across frames rather than flickering as new events arrive.
    let ids = ["run-1", "run-2", "run-3", "run-4", "run-5", "run-6"];
    for id in ids {
        let slot = subagent_slot(id);
        assert!(slot < 4, "slot must land in [0, 4) for {id}");
        assert_eq!(
            slot,
            subagent_slot(id),
            "slot must be deterministic across calls for {id}",
        );
    }
}

#[test]
fn motion_streaming_is_hot_requires_recent_token() {
    // Both clocks pinned at 0 (tests): the pulse never fires so
    // goldens stay deterministic.
    let ctx = MotionCtx::default();
    assert!(!ctx.streaming_is_hot());

    let ctx = MotionCtx {
        now_ms: 1_000,
        last_event_ms: 900,
    };
    assert!(ctx.streaming_is_hot(), "100ms gap < 350ms threshold");

    let ctx = MotionCtx {
        now_ms: 1_000,
        last_event_ms: 500,
    };
    assert!(!ctx.streaming_is_hot(), "500ms gap > 350ms threshold");
}

#[test]
fn render_blocks_handles_heading_lists_quotes_code_and_artifacts() {
    let styles = ThemeStyles::for_theme(ThemeId::DefaultDark);
    let ctx = RenderCtx {
        theme_id: ThemeId::DefaultDark,
        styles: &styles,
        motion: MotionCtx::default(),
    };
    let blocks = vec![
        CanvasBlock::Heading {
            level: 2,
            text: CachedText::plain("Title"),
        },
        CanvasBlock::Paragraph(CachedText::plain("para")),
        CanvasBlock::List {
            ordered: false,
            items: vec![vec![CanvasBlock::Paragraph(CachedText::plain("item"))]],
        },
        CanvasBlock::BlockQuote(vec![CanvasBlock::Paragraph(CachedText::plain("quoted"))]),
        CanvasBlock::CodeFence {
            lang: Some("rust".to_string()),
            text: CachedText::plain("let x = 1;"),
        },
        CanvasBlock::Thematic,
        CanvasBlock::ArtifactChip {
            artifact_id: "abcdef1234567890".to_string(),
            bytes: Some(42),
        },
    ];

    let rendered = render_blocks(&blocks, &ctx);
    let text = plain_text(&ratatui::text::Text::from(rendered));
    assert!(text.contains("## Title"), "{text}");
    assert!(text.contains("para"), "{text}");
    assert!(text.contains("• item"), "{text}");
    assert!(text.contains("│ quoted"), "{text}");
    assert!(text.contains("```rust"), "{text}");
    assert!(text.contains("let x = 1;"), "{text}");
    assert!(text.contains("────"), "{text}");
    assert!(text.contains("⧉ abcdef12"), "{text}");

    let artifact_lines = artifact_chip_lines(&["abcdef1234567890".to_string()]);
    assert_eq!(artifact_lines[0].to_string(), "⧉ abcdef12");

    let block_lines = block_as_lines(&CanvasBlock::ToolStdout(CachedText::plain("stdout")));
    assert_eq!(block_lines[0].to_string(), "stdout");
}

#[test]
fn message_body_lines_cover_notice_and_summary_variants() {
    let styles = ThemeStyles::for_theme(ThemeId::DefaultDark);
    let ctx = RenderCtx {
        theme_id: ThemeId::DefaultDark,
        styles: &styles,
        motion: MotionCtx::default(),
    };

    let job = CanvasMessage::JobNotice {
        message_id: "m1".to_string(),
        job_id: "job-1".to_string(),
        job_kind: "compaction".to_string(),
        details: None,
        status: JobLifecycle::Failed {
            error: Some("boom".to_string()),
        },
        actor_id: "user".to_string(),
        origin: "cli".to_string(),
        started_at_ms: Some(1),
        ended_at_ms: Some(2),
    };
    assert_eq!(
        message_body_lines(&job, &ctx)[0].to_string(),
        "compaction · failed"
    );

    let notice = CanvasMessage::SystemNotice {
        message_id: "m2".to_string(),
        level: NoticeLevel::Warn,
        text: "watch out".to_string(),
        origin_event_kind: "provider_event".to_string(),
        seq: 7,
    };
    assert_eq!(
        message_body_lines(&notice, &ctx)[0].to_string(),
        "watch out"
    );

    let context = CanvasMessage::ContextNotice {
        message_id: "m3".to_string(),
        run_session_id: "run-1".to_string(),
        strategy: "recent_messages_v1".to_string(),
        status: ContextLifecycle::Compiled,
        bundle_artifact_id: Some("bundle".to_string()),
        contributed_artifact_ids: vec!["artifact".to_string()],
    };
    assert_eq!(
        message_body_lines(&context, &ctx)[0].to_string(),
        "context recent_messages_v1 · compiled"
    );

    let checkpoint = CanvasMessage::CompactionCheckpoint {
        message_id: "m4".to_string(),
        checkpoint_id: "ckpt-1".to_string(),
        from_seq: 4,
        to_seq: 9,
        summary_artifact_id: "artifact".to_string(),
    };
    assert_eq!(
        message_body_lines(&checkpoint, &ctx)[0].to_string(),
        "compaction checkpoint · seq 4…9"
    );

    let panel = CanvasMessage::ExtensionPanel {
        message_id: "m5".to_string(),
        panel_id: "panel-1".to_string(),
        extension_id: "ext".to_string(),
        title: "Inspector".to_string(),
        placement: PanelPlacement::Inline,
        lines: vec![StyledLine {
            text: "line".to_string(),
            accent: None,
        }],
        keys: vec![],
        artifact_ids: vec![],
    };
    assert_eq!(
        message_body_lines(&panel, &ctx)[0].to_string(),
        "extension: Inspector"
    );

    let reviewer = CanvasMessage::AgentTurn {
        message_id: "m6".to_string(),
        run_session_id: "run-2".to_string(),
        agent_id: Some("reviewer".to_string()),
        role: AgentRole::Reviewer {
            target_message_id: "m1".to_string(),
        },
        actor_id: "reviewer".to_string(),
        model: Some("gpt".to_string()),
        blocks: vec![CanvasBlock::Paragraph(CachedText::plain("review"))],
        streaming_tail: String::new(),
        streaming: false,
        started_at_ms: 0,
        ended_at_ms: Some(1),
    };
    assert_eq!(
        message_glyph(&reviewer, &styles, MotionCtx::default()).0,
        "◌"
    );
}
