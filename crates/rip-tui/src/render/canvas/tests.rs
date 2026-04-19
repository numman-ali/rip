use super::*;

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
