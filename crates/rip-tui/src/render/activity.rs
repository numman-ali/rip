use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::TuiState;

use super::theme::ThemeStyles;

pub(super) fn render_activity_rail(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from("Activity").style(theme.header))
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = build_activity_lines(state, inner.height as usize);
    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(widget, inner);
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
