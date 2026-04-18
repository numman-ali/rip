use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::TuiState;

use super::super::theme::ThemeStyles;
use super::super::util::truncate;
use super::super::RenderMode;

pub(super) fn render_tool_detail_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
    tool_id: &str,
    mode: RenderMode,
) {
    frame.render_widget(Clear, area);
    let title = format!("Tool Detail: {tool_id}");
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(tool) = state.tools.get(tool_id) else {
        let widget = Paragraph::new(Text::from("<unknown tool>")).style(theme.chrome);
        frame.render_widget(widget, inner);
        return;
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    let status = match &tool.status {
        crate::ToolStatus::Running => "running".to_string(),
        crate::ToolStatus::Ended {
            exit_code,
            duration_ms,
        } => format!("ended exit={exit_code} ({duration_ms}ms)"),
        crate::ToolStatus::Failed { error } => format!("failed: {}", truncate(error, 64)),
    };
    lines.push(Line::from(format!("tool: {}", tool.name)));
    lines.push(Line::from(format!("status: {status}")));
    lines.push(Line::from(" "));

    lines.push(Line::from("args:"));
    match serde_json::to_string_pretty(&tool.args) {
        Ok(json) => {
            for line in json.lines().take(10) {
                lines.push(Line::from(line.to_string()));
            }
        }
        Err(_) => lines.push(Line::from("<failed to render args>")),
    }

    lines.push(Line::from(" "));
    if !tool.stdout_preview.is_empty() {
        lines.push(Line::from("stdout (preview):"));
        for line in tool.stdout_preview.lines().take(6) {
            lines.push(Line::from(line.to_string()));
        }
    }
    if !tool.stderr_preview.is_empty() {
        lines.push(Line::from("stderr (preview):"));
        for line in tool.stderr_preview.lines().take(6) {
            lines.push(Line::from(line.to_string()));
        }
    }

    if !tool.artifact_ids.is_empty() {
        lines.push(Line::from(" "));
        lines.push(Line::from(format!(
            "artifacts: {}",
            tool.artifact_ids.len()
        )));
    }

    lines.push(Line::from(" "));
    lines.push(Line::from(format!(
        "inspector_mode: {}",
        match mode {
            RenderMode::Json => "json",
            RenderMode::Decoded => "decoded",
        }
    )));

    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(widget, inner);
}
