use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::TuiState;

use super::super::theme::ThemeStyles;

pub(super) fn render_task_list_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Tasks")
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from("running / completed / failed"));
    lines.push(Line::from(" "));

    let mut tasks: Vec<&crate::TaskSummary> = state.tasks.values().collect();
    tasks.sort_by_key(|t| {
        (
            matches!(
                t.status,
                rip_kernel::ToolTaskStatus::Exited
                    | rip_kernel::ToolTaskStatus::Cancelled
                    | rip_kernel::ToolTaskStatus::Failed
            ),
            t.task_id.as_str(),
        )
    });

    for task in tasks {
        let icon = match task.status {
            rip_kernel::ToolTaskStatus::Queued => "◯",
            rip_kernel::ToolTaskStatus::Running => "⟳",
            rip_kernel::ToolTaskStatus::Exited => "✓",
            rip_kernel::ToolTaskStatus::Cancelled => "⊘",
            rip_kernel::ToolTaskStatus::Failed => "✗",
        };
        let title = task
            .title
            .as_deref()
            .filter(|t| !t.is_empty())
            .unwrap_or(task.tool_name.as_str());
        lines.push(Line::from(format!("{icon} {title}  ({:?})", task.status)));
    }

    let widget = Paragraph::new(Text::from(lines))
        .scroll((state.overlay_scroll, 0))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(widget, inner);
}
