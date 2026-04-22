use ratatui::Frame;
use ratatui_textarea::TextArea;

use crate::{OutputViewMode, Overlay, TuiState};

mod activity;
mod canvas;
mod input;
mod overlays;
mod status_bar;
mod syntax;
mod theme;
mod util;
mod xray;

pub use self::canvas::canvas_hit_message_id;
pub use self::status_bar::{hero_click_target, HeroClickTarget};
use self::theme::ThemeStyles;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Json,
    Decoded,
}

pub fn render(
    frame: &mut Frame<'_>,
    state: &TuiState,
    mode: RenderMode,
    input: &TextArea<'static>,
) {
    let theme = ThemeStyles::for_theme(state.theme);
    match state.output_view {
        OutputViewMode::Rendered => self::canvas::render_canvas_screen(frame, state, &theme, input),
        OutputViewMode::Raw => self::xray::render_xray_screen(frame, state, &theme, mode, input),
    }

    if state.overlay() != &Overlay::None {
        self::overlays::render_overlay(frame, state, &theme, mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ThemeId;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::Terminal;
    use rip_kernel::{
        Event, EventKind, ProviderEventStatus, ToolTaskExecutionMode, ToolTaskStatus,
        ToolTaskStream,
    };
    use serde_json::json;

    use self::activity::{build_activity_lines, build_strip_line};
    use self::canvas::build_canvas_text;
    use self::input::build_help_line;
    use self::overlays::{overlay_body_area, overlay_modal_area};
    use self::util::wrapped_line_count;
    use self::xray::{selected_event_decoded, selected_event_json};

    fn event(seq: u64, kind: EventKind) -> Event {
        Event {
            id: format!("e{seq}"),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq,
            kind,
        }
    }

    fn render_once(state: &TuiState, mode: RenderMode, width: u16) {
        let mut terminal = Terminal::new(TestBackend::new(width, 20)).expect("terminal");
        let input = TextArea::default();
        terminal
            .draw(|f| render(f, state, mode, &input))
            .expect("draw");
    }

    fn buffer_to_string(buffer: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                let symbol = buffer.cell((x, y)).map(|cell| cell.symbol()).unwrap_or(" ");
                line.push_str(symbol);
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    fn render_to_string(state: &TuiState, mode: RenderMode, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        let input = TextArea::default();
        terminal
            .draw(|f| render(f, state, mode, &input))
            .expect("draw");
        buffer_to_string(terminal.backend().buffer())
    }

    #[test]
    fn render_handles_empty_state_small_width() {
        let state = TuiState::new(100);
        render_once(&state, RenderMode::Json, 60);
    }

    #[test]
    fn render_handles_decoded_mode_and_streaming_output() {
        let mut state = TuiState::new(100);
        state.update(event(
            0,
            EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        ));
        state.update(event(
            1,
            EventKind::OutputTextDelta {
                delta: "hello".to_string(),
            },
        ));
        render_once(&state, RenderMode::Decoded, 100);
    }

    fn seed_overlay_state() -> TuiState {
        let mut state = TuiState::new(100);
        state.update(event(
            0,
            EventKind::SessionStarted {
                input: "hello".to_string(),
            },
        ));
        state.update(event(
            1,
            EventKind::OpenResponsesRequestStarted {
                endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
                model: Some("gpt-5".to_string()),
                request_index: 0,
                kind: "response.create".to_string(),
            },
        ));
        state.update(event(
            2,
            EventKind::OpenResponsesResponseHeaders {
                request_index: 0,
                status: 200,
                request_id: Some("req_123".to_string()),
                content_type: Some("text/event-stream".to_string()),
            },
        ));
        state.update(event(
            3,
            EventKind::OpenResponsesResponseFirstByte { request_index: 0 },
        ));
        state.update(event(
            4,
            EventKind::ProviderEvent {
                provider: "openresponses".to_string(),
                status: ProviderEventStatus::InvalidJson,
                event_name: None,
                data: None,
                raw: Some("{".to_string()),
                errors: vec!["bad json".to_string()],
                response_errors: vec!["schema".to_string()],
            },
        ));
        state.update(event(
            5,
            EventKind::OutputTextDelta {
                delta: "output".to_string(),
            },
        ));
        state.update(event(
            6,
            EventKind::ToolStarted {
                tool_id: "tool-1".to_string(),
                name: "write".to_string(),
                args: json!({"path": "notes.md"}),
                timeout_ms: None,
            },
        ));
        state.update(event(
            7,
            EventKind::ToolStdout {
                tool_id: "tool-1".to_string(),
                chunk: "stdout line".to_string(),
            },
        ));
        state.update(event(
            8,
            EventKind::ToolStderr {
                tool_id: "tool-1".to_string(),
                chunk: "stderr line".to_string(),
            },
        ));
        state.update(event(
            9,
            EventKind::ToolTaskSpawned {
                task_id: "task-1".to_string(),
                tool_name: "shell".to_string(),
                args: json!({"cmd": "pwd"}),
                cwd: Some("/tmp".to_string()),
                title: Some("pwd".to_string()),
                execution_mode: ToolTaskExecutionMode::Pty,
                origin_session_id: None,
                artifacts: Some(json!({"artifact": "a".repeat(64)})),
            },
        ));
        state.update(event(
            10,
            EventKind::ToolTaskOutputDelta {
                task_id: "task-1".to_string(),
                stream: ToolTaskStream::Pty,
                chunk: "pty line".to_string(),
                artifacts: None,
            },
        ));
        state.update(event(
            11,
            EventKind::ToolTaskStatus {
                task_id: "task-1".to_string(),
                status: ToolTaskStatus::Running,
                exit_code: None,
                started_at_ms: Some(9),
                ended_at_ms: None,
                artifacts: None,
                error: None,
            },
        ));
        state.update(event(
            12,
            EventKind::ContinuityJobSpawned {
                job_id: "job-1".to_string(),
                job_kind: "compaction".to_string(),
                details: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ));
        state.update(event(
            13,
            EventKind::ContinuityContextCompiled {
                run_session_id: "run-1".to_string(),
                bundle_artifact_id: "b".repeat(64),
                compiler_id: "rip.context_compiler.v1".to_string(),
                compiler_strategy: "recent_messages_v1".to_string(),
                from_seq: 1,
                from_message_id: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ));
        state.activity_pinned = true;
        state.set_status_message("watching");
        state.set_now_ms(10_000);
        state
    }

    #[test]
    fn helper_builders_reflect_errors_stalls_and_running_work() {
        let mut state = seed_overlay_state();
        state.last_event_ms = Some(0);

        let activity = build_activity_lines(&state, 10)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(activity.iter().any(|line| line.contains("error")));
        assert!(activity.iter().any(|line| line.contains("stalled")));
        assert!(activity.iter().any(|line| line.contains("tool write")));
        assert!(activity.iter().any(|line| line.contains("task pwd")));
        assert!(activity.iter().any(|line| line.contains("job compaction")));
        assert!(activity.iter().any(|line| line.contains("ctx compiled")));
        assert!(activity.iter().any(|line| line.contains("artifacts")));

        let theme = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let strip = build_strip_line(&state, &theme, 120).expect("strip populated");
        let strip_str = strip.to_string();
        assert!(strip_str.contains("▲ error"), "strip: {strip_str}");
        assert!(strip_str.contains("stalled"));
        assert!(strip_str.contains("⟡ write"));
        assert!(strip_str.contains("⧉ pwd"));
        assert!(strip_str.contains("◐ compaction"));
        assert!(strip_str.contains("ctx compiled"));

        let truncated = build_strip_line(&state, &theme, 12)
            .expect("strip populated")
            .to_string();
        assert!(truncated.ends_with('…'), "expected ellipsis: {truncated}");
    }

    #[test]
    fn strip_auto_hides_when_idle_at_bottom() {
        let theme = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let state = TuiState::new(100);
        assert!(build_strip_line(&state, &theme, 80).is_none());
    }

    #[test]
    fn strip_shows_scrolled_back_hint_when_not_at_bottom() {
        let theme = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let mut state = TuiState::new(100);
        state.canvas_scroll_from_bottom = 3;
        let strip = build_strip_line(&state, &theme, 80)
            .expect("strip populated")
            .to_string();
        assert!(strip.contains("scrolled back"), "strip: {strip}");
    }

    #[test]
    fn build_canvas_text_styles_user_turns() {
        let mut state = TuiState::default();
        state.begin_pending_turn("hello\nsecond line");
        let theme = ThemeStyles::for_theme(ThemeId::DefaultDark);
        let text = build_canvas_text(&state, &theme, 80);

        // UserTurn renders as a 3-column gutter (glyph in accent + focus
        // rule + spacer) followed by the body. Second and subsequent body
        // lines keep the body style but the gutter columns are spacers.
        assert_eq!(text.lines[0].spans[0].style, theme.prompt_label);
        assert_eq!(text.lines[0].spans[3].style, theme.prompt);
        assert_eq!(text.lines[1].spans[3].style, theme.prompt);
    }

    #[test]
    fn wrapped_line_count_and_help_line_have_small_screen_fallbacks() {
        assert_eq!(wrapped_line_count("", 10), 1);
        assert_eq!(wrapped_line_count("hello", 10), 1);
        assert_eq!(wrapped_line_count("hello world", 5), 3);
        // Idle-state keylight starts with "? help" as the headline
        // shortcut; truncation at 8 chars leaves room for an ellipsis.
        assert_eq!(build_help_line(8), "? help …");
    }

    #[test]
    fn selected_event_renderers_and_overlay_geometry_have_fallbacks() {
        let state = TuiState::default();
        assert_eq!(
            selected_event_json(&state).to_string(),
            "<no frame selected>"
        );
        assert_eq!(selected_event_decoded(&state).to_string(), "");

        let body = overlay_body_area(
            Rect {
                x: 0,
                y: 0,
                width: 120,
                height: 40,
            },
            OutputViewMode::Raw,
        );
        // Hero (1 row) on top, input block (2 rows) on bottom.
        assert_eq!(body.y, 1);
        assert_eq!(body.height, 37);

        let modal = overlay_modal_area(body);
        assert!(modal.width < body.width);
        assert!(modal.height < body.height);
    }

    #[test]
    fn render_helpers_cover_overlay_variants_and_decoded_views() {
        let mut state = seed_overlay_state();
        state.selected_seq = Some(0);
        let json = selected_event_json(&state).to_string();
        assert!(json.contains("\"type\": \"session_started\""));
        let decoded = selected_event_decoded(&state).to_string();
        assert!(decoded.contains("\"summary\": \"\\\"hello\\\"\""));

        render_once(&state, RenderMode::Json, 120);

        state.output_view = OutputViewMode::Raw;
        state.theme = ThemeId::DefaultLight;
        render_once(&state, RenderMode::Decoded, 120);

        for overlay in [
            Overlay::Activity,
            Overlay::Palette(crate::PaletteState::new(
                crate::PaletteMode::Model,
                crate::PaletteOrigin::TopCenter,
                vec![
                    crate::PaletteEntry {
                        value: "openrouter/openai/gpt-oss-20b".to_string(),
                        title: "openrouter/openai/gpt-oss-20b".to_string(),
                        subtitle: Some("OpenRouter".to_string()),
                        chips: vec!["current".to_string()],
                    },
                    crate::PaletteEntry {
                        value: "openai/gpt-5-nano-2025-08-07".to_string(),
                        title: "openai/gpt-5-nano-2025-08-07".to_string(),
                        subtitle: None,
                        chips: vec![],
                    },
                ],
                "No models".to_string(),
                true,
                "Use typed route".to_string(),
            )),
            Overlay::TaskList,
            Overlay::ToolDetail {
                tool_id: "tool-1".to_string(),
            },
            Overlay::TaskDetail {
                task_id: "task-1".to_string(),
            },
            Overlay::ErrorDetail { seq: 4 },
            Overlay::StallDetail,
        ] {
            state.set_overlay(overlay);
            render_once(&state, RenderMode::Decoded, 120);
        }

        state.set_overlay(Overlay::ToolDetail {
            tool_id: "missing".to_string(),
        });
        render_once(&state, RenderMode::Json, 120);

        state.set_overlay(Overlay::TaskDetail {
            task_id: "missing".to_string(),
        });
        render_once(&state, RenderMode::Json, 120);

        state.set_overlay(Overlay::ErrorDetail { seq: 999 });
        render_once(&state, RenderMode::Json, 120);
    }

    #[test]
    fn overlay_renderers_expose_palette_tool_and_task_details() {
        let mut state = seed_overlay_state();

        state.set_overlay(Overlay::Palette(crate::PaletteState::new(
            crate::PaletteMode::Model,
            crate::PaletteOrigin::TopCenter,
            vec![
                crate::PaletteEntry {
                    value: "openrouter/openai/gpt-oss-20b".to_string(),
                    title: "openrouter/openai/gpt-oss-20b".to_string(),
                    subtitle: Some("OpenRouter".to_string()),
                    chips: vec!["active".to_string(), "128k".to_string()],
                },
                crate::PaletteEntry {
                    value: "openai/gpt-5-nano".to_string(),
                    title: "openai/gpt-5-nano".to_string(),
                    subtitle: None,
                    chips: vec![],
                },
            ],
            "No models".to_string(),
            true,
            "Use typed route".to_string(),
        )));
        let palette = render_to_string(&state, RenderMode::Json, 120, 30);
        assert!(palette.contains("Model Picker"), "{palette}");
        assert!(palette.contains("Filter"), "{palette}");
        assert!(
            palette.contains("openrouter/openai/gpt-oss-20b"),
            "{palette}"
        );
        assert!(palette.contains("OpenRouter"), "{palette}");
        assert!(palette.contains("[active] [128k]"), "{palette}");

        state.set_overlay(Overlay::ToolDetail {
            tool_id: "tool-1".to_string(),
        });
        let tool = render_to_string(&state, RenderMode::Json, 120, 30);
        assert!(tool.contains("Tool Detail: tool-1"), "{tool}");
        assert!(tool.contains("tool: write"), "{tool}");
        assert!(tool.contains("stdout (preview):"), "{tool}");
        assert!(tool.contains("stderr (preview):"), "{tool}");
        assert!(tool.contains("inspector_mode: json"), "{tool}");

        state.set_overlay(Overlay::TaskDetail {
            task_id: "task-1".to_string(),
        });
        let task = render_to_string(&state, RenderMode::Decoded, 120, 30);
        assert!(task.contains("Task Detail: task-1"), "{task}");
        assert!(task.contains("tool: shell"), "{task}");
        assert!(task.contains("title: pwd"), "{task}");
        assert!(task.contains("pty (preview):"), "{task}");
        assert!(task.contains("artifacts: 1"), "{task}");

        state.set_overlay(Overlay::ToolDetail {
            tool_id: "missing".to_string(),
        });
        let missing_tool = render_to_string(&state, RenderMode::Json, 120, 20);
        assert!(missing_tool.contains("<unknown tool>"), "{missing_tool}");

        state.set_overlay(Overlay::TaskDetail {
            task_id: "missing".to_string(),
        });
        let missing_task = render_to_string(&state, RenderMode::Json, 120, 20);
        assert!(missing_task.contains("<unknown task>"), "{missing_task}");
    }
}
