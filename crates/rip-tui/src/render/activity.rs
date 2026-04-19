use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::TuiState;

use super::theme::ThemeStyles;

pub(super) fn render_activity_rail(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    let header = Paragraph::new(Line::from(Span::styled("Activity", theme.header)));
    frame.render_widget(header, chunks[0]);

    let lines = build_activity_lines(state, chunks[1].height as usize);
    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.muted);
    frame.render_widget(widget, chunks[1]);
}

/// The strip (C.2 footer) is an ordered summary of ambient work —
/// error first, stall next, then running tools / tasks / jobs, then
/// context + artifact counts. Each item has a glyph + short label.
///
/// Returns `None` when there is nothing worth showing *and* the
/// transcript is at its natural bottom (the user hasn't scrolled up
/// to see history). When scrolled up, we still show the strip even
/// when empty — it flips to a subtle `· scrolled back` breadcrumb so
/// the user knows the view isn't live.
pub(super) fn build_strip_line(
    state: &TuiState,
    theme: &ThemeStyles,
    width: usize,
) -> Option<Line<'static>> {
    let items = collect_strip_items(state);
    let scrolled = state.canvas_scroll_from_bottom > 0;
    if items.is_empty() && !scrolled {
        return None;
    }

    if items.is_empty() {
        // Scrolled-back hint: one muted dot + label, no chrome.
        let text = " · scrolled back — press End to follow";
        let truncated = truncate(text, width);
        return Some(Line::from(vec![Span::styled(truncated, theme.quiet)]));
    }

    let style = strip_worst_style(state, theme);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used: usize = 0;
    for (idx, chunk) in items.iter().enumerate() {
        let sep = if idx == 0 { "" } else { "  ·  " };
        let combined = format!("{sep}{chunk}");
        let combined_len = combined.chars().count();
        if used + combined_len > width {
            let remaining = width.saturating_sub(used);
            if remaining > 1 {
                let mut trimmed: String = combined.chars().take(remaining - 1).collect();
                trimmed.push('…');
                spans.push(Span::styled(trimmed, style));
            } else if remaining == 1 {
                spans.push(Span::styled("…".to_string(), style));
            }
            return Some(Line::from(spans));
        }
        used += combined_len;
        // Use chunk style: first item gets the worst-state style
        // (so errors/stalls land in the right tint). Later items
        // read as muted so the strip scans left-to-right with the
        // most severe item first.
        let seg_style = if idx == 0 { style } else { theme.muted };
        if !sep.is_empty() {
            spans.push(Span::styled(sep.to_string(), theme.quiet));
        }
        spans.push(Span::styled(chunk.clone(), seg_style));
    }
    Some(Line::from(spans))
}

fn collect_strip_items(state: &TuiState) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();

    if state.has_error() {
        items.push(if let Some(seq) = state.last_error_seq {
            format!("▲ error @seq {seq}")
        } else {
            "▲ error".to_string()
        });
    }

    if state.is_stalled(5_000) {
        items.push("· stalled".to_string());
    }

    let running_tools: Vec<&str> = state.running_tool_ids().collect();
    if !running_tools.is_empty() {
        let name = state
            .tools
            .get(running_tools[0])
            .map(|t| t.name.as_str())
            .unwrap_or("tool");
        let label = if running_tools.len() == 1 {
            format!("⟡ {name}")
        } else {
            format!("⟡ {name} +{}", running_tools.len() - 1)
        };
        items.push(label);
    }

    let running_tasks: Vec<&str> = state.running_task_ids().collect();
    if !running_tasks.is_empty() {
        let title = running_tasks
            .first()
            .and_then(|id| state.tasks.get(*id))
            .map(|task| {
                task.title
                    .as_deref()
                    .filter(|t| !t.is_empty())
                    .unwrap_or(task.tool_name.as_str())
                    .to_string()
            })
            .unwrap_or_else(|| "task".to_string());
        let label = if running_tasks.len() == 1 {
            format!("⧉ {title}")
        } else {
            format!("⧉ {title} +{}", running_tasks.len() - 1)
        };
        items.push(label);
    }

    let running_jobs: Vec<&str> = state.running_job_ids().collect();
    if !running_jobs.is_empty() {
        let kind = running_jobs
            .first()
            .and_then(|id| state.jobs.get(*id))
            .map(|job| job.job_kind.clone())
            .unwrap_or_else(|| "job".to_string());
        let label = if running_jobs.len() == 1 {
            format!("◐ {kind}")
        } else {
            format!("◐ {kind} +{}", running_jobs.len() - 1)
        };
        items.push(label);
    }

    if let Some(ctx) = state.context.as_ref() {
        match ctx.status {
            crate::ContextStatus::Selecting => items.push("⌖ ctx selecting".to_string()),
            crate::ContextStatus::Compiled => items.push("⌖ ctx compiled".to_string()),
        }
    }

    items
}

fn strip_worst_style(state: &TuiState, theme: &ThemeStyles) -> Style {
    if state.has_error() {
        return theme.danger;
    }
    if state.is_stalled(5_000) {
        return theme.warn;
    }
    theme.muted
}

fn truncate(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = input.chars().take(keep).collect();
    out.push('…');
    out
}

pub(super) fn build_activity_lines(state: &TuiState, max_lines: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    if state.awaiting_response {
        lines.push(Line::from("◔ waiting for response"));
    }

    if state.has_error() {
        if let Some(seq) = state.last_error_seq {
            lines.push(Line::from(format!("⚠ error @seq {seq}")));
        } else {
            lines.push(Line::from("⚠ error"));
        }
    }

    if state.is_stalled(5_000) {
        lines.push(Line::from("⏸ stalled"));
    }

    for tool in state.tools.values() {
        if matches!(tool.status, crate::ToolStatus::Running) {
            lines.push(Line::from(format!("⟳ tool {}", tool.name)));
        }
    }

    if !state.tools.is_empty()
        && !state
            .tools
            .values()
            .any(|tool| matches!(tool.status, crate::ToolStatus::Running))
    {
        if let Some(tool) = state.tools.values().max_by_key(|tool| tool.started_seq) {
            let label = match &tool.status {
                crate::ToolStatus::Ended { .. } => "✓",
                crate::ToolStatus::Failed { .. } => "✕",
                crate::ToolStatus::Running => "⟳",
            };
            lines.push(Line::from(format!("{label} tool {}", tool.name)));
        }
    }

    for task in state.tasks.values() {
        if matches!(
            task.status,
            rip_kernel::ToolTaskStatus::Queued | rip_kernel::ToolTaskStatus::Running
        ) {
            let title = task
                .title
                .as_deref()
                .filter(|t| !t.is_empty())
                .unwrap_or(task.tool_name.as_str());
            lines.push(Line::from(format!("⟳ task {title}")));
        }
    }

    for job in state.jobs.values() {
        if matches!(job.status, crate::JobStatus::Running) {
            lines.push(Line::from(format!("◐ job {}", job.job_kind)));
        }
    }

    if let Some(ctx) = state.context.as_ref() {
        let status = match ctx.status {
            crate::ContextStatus::Selecting => "selecting",
            crate::ContextStatus::Compiled => "compiled",
        };
        lines.push(Line::from(format!("⚙ ctx {status}")));
    }

    if !state.artifacts.is_empty() {
        lines.push(Line::from(format!(
            "📄 artifacts {}",
            state.artifacts.len()
        )));
    }

    lines.truncate(max_lines.max(1));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{JobStatus, JobSummary, TaskSummary, ToolStatus, ToolSummary};
    use std::collections::BTreeSet;

    fn seed() -> TuiState {
        let mut state = TuiState::new(10);
        state.now_ms = Some(1_000);
        state.continuity_id = Some("cont-1".to_string());
        state
    }

    fn tool(id: &str, name: &str, status: ToolStatus) -> ToolSummary {
        ToolSummary {
            tool_id: id.to_string(),
            name: name.to_string(),
            args: serde_json::Value::Null,
            started_seq: 0,
            started_at_ms: 1_000,
            status,
            stdout_preview: String::new(),
            stderr_preview: String::new(),
            artifact_ids: BTreeSet::new(),
        }
    }

    fn task(title: &str, status: rip_kernel::ToolTaskStatus) -> TaskSummary {
        TaskSummary {
            task_id: "task-1".to_string(),
            tool_name: "bash".to_string(),
            args: serde_json::Value::Null,
            cwd: None,
            title: Some(title.to_string()),
            execution_mode: rip_kernel::ToolTaskExecutionMode::Pipes,
            status,
            exit_code: None,
            started_at_ms: Some(900),
            ended_at_ms: None,
            error: None,
            stdout_preview: String::new(),
            stderr_preview: String::new(),
            pty_preview: String::new(),
            artifact_ids: BTreeSet::new(),
        }
    }

    fn running_job(kind: &str) -> JobSummary {
        JobSummary {
            job_id: "job-1".to_string(),
            job_kind: kind.to_string(),
            status: JobStatus::Running,
        }
    }

    #[test]
    fn collect_strip_items_mentions_every_ambient_state_source() {
        let mut state = seed();
        state.tools.insert(
            "tool-1".to_string(),
            tool("tool-1", "bash", ToolStatus::Running),
        );
        state.tools.insert(
            "tool-2".to_string(),
            tool("tool-2", "write", ToolStatus::Running),
        );
        state.tasks.insert(
            "task-1".to_string(),
            task("long running", rip_kernel::ToolTaskStatus::Running),
        );
        state
            .jobs
            .insert("job-1".to_string(), running_job("indexer"));
        state.context = Some(crate::state::ContextSummary {
            run_session_id: "r".to_string(),
            compiler_strategy: "default".to_string(),
            status: crate::ContextStatus::Compiled,
            bundle_artifact_id: None,
        });

        let items = collect_strip_items(&state);
        assert!(items.iter().any(|s| s.contains("bash +1")));
        assert!(items.iter().any(|s| s.contains("long running")));
        assert!(items.iter().any(|s| s.contains("indexer")));
        assert!(items.iter().any(|s| s.contains("ctx compiled")));
    }

    #[test]
    fn truncate_respects_char_limit_and_handles_zero() {
        assert_eq!(truncate("hello", 0), "");
        assert_eq!(truncate("hello", 10), "hello");
        let out = truncate("abcdefgh", 5);
        assert_eq!(out.chars().count(), 5);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn strip_worst_style_escalates_to_danger_then_warn_then_muted() {
        let theme = ThemeStyles::for_theme(crate::ThemeId::DefaultDark);
        let mut state = seed();
        assert_eq!(strip_worst_style(&state, &theme), theme.muted);

        state.last_error_seq = Some(1);
        assert_eq!(strip_worst_style(&state, &theme), theme.danger);

        state.last_error_seq = None;
        // is_stalled needs a last_event_ms far enough in the past.
        state.last_event_ms = Some(0);
        state.now_ms = Some(10_000);
        assert_eq!(strip_worst_style(&state, &theme), theme.warn);
    }

    #[test]
    fn build_strip_line_collapses_empty_running_unless_scrolled() {
        let theme = ThemeStyles::for_theme(crate::ThemeId::DefaultDark);
        let mut state = seed();
        // Nothing to say, at-bottom: no line.
        assert!(build_strip_line(&state, &theme, 80).is_none());

        // Scrolled up with no items: surface the breadcrumb.
        state.canvas_scroll_from_bottom = 3;
        let line = build_strip_line(&state, &theme, 80).expect("breadcrumb");
        let text: String = line
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect::<String>();
        assert!(text.contains("scrolled back"));
    }

    #[test]
    fn build_strip_line_truncates_on_narrow_width() {
        let theme = ThemeStyles::for_theme(crate::ThemeId::DefaultDark);
        let mut state = seed();
        for i in 0..3 {
            state.tools.insert(
                format!("tool-{i}"),
                tool(
                    &format!("tool-{i}"),
                    &format!("tool{i}"),
                    ToolStatus::Running,
                ),
            );
        }
        // Width of 5 forces a mid-chunk ellipsis: first item "⟡ tool0 +2"
        // is ~10 chars so it can't fit even alone, and the truncation
        // branch emits an ellipsis-capped prefix.
        let line = build_strip_line(&state, &theme, 5).expect("truncated line");
        let text: String = line
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect::<String>();
        assert!(
            text.contains('…'),
            "expected ellipsis in truncated strip: {text:?}",
        );
    }

    #[test]
    fn build_activity_lines_respects_max_lines_cap() {
        let mut state = seed();
        for i in 0..5 {
            state.tools.insert(
                format!("tool-{i}"),
                tool(&format!("tool-{i}"), &format!("t{i}"), ToolStatus::Running),
            );
        }
        let lines = build_activity_lines(&state, 2);
        assert_eq!(lines.len(), 2);
        assert_eq!(build_activity_lines(&state, 0).len(), 1);
    }
}
