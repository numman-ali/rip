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
