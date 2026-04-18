use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::TuiState;

use super::super::theme::ThemeStyles;
use super::super::util::truncate;

pub(super) fn render_task_detail_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
    task_id: &str,
) {
    frame.render_widget(Clear, area);
    let title = format!("Task Detail: {task_id}");
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(task) = state.tasks.get(task_id) else {
        frame.render_widget(
            Paragraph::new(Text::from("<unknown task>")).style(theme.chrome),
            inner,
        );
        return;
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(format!("tool: {}", task.tool_name)));
    if let Some(title) = task.title.as_deref().filter(|t| !t.is_empty()) {
        lines.push(Line::from(format!("title: {title}")));
    }
    lines.push(Line::from(format!("status: {:?}", task.status)));
    if let Some(code) = task.exit_code {
        lines.push(Line::from(format!("exit: {code}")));
    }
    if let Some(err) = task.error.as_deref() {
        lines.push(Line::from(format!("error: {}", truncate(err, 80))));
    }
    lines.push(Line::from(" "));
    if !task.stdout_preview.is_empty() {
        lines.push(Line::from("stdout (preview):"));
        for line in task.stdout_preview.lines().take(6) {
            lines.push(Line::from(line.to_string()));
        }
    }
    if !task.stderr_preview.is_empty() {
        lines.push(Line::from("stderr (preview):"));
        for line in task.stderr_preview.lines().take(6) {
            lines.push(Line::from(line.to_string()));
        }
    }
    if !task.pty_preview.is_empty() {
        lines.push(Line::from("pty (preview):"));
        for line in task.pty_preview.lines().take(6) {
            lines.push(Line::from(line.to_string()));
        }
    }
    if !task.artifact_ids.is_empty() {
        lines.push(Line::from(" "));
        lines.push(Line::from(format!(
            "artifacts: {}",
            task.artifact_ids.len()
        )));
    }

    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(widget, inner);
}
