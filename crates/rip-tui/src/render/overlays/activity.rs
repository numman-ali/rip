use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::TuiState;

use super::super::activity::build_activity_lines;
use super::super::theme::ThemeStyles;

pub(super) fn render_activity_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Activity")
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(
        "tools / tasks / jobs / context / artifacts / errors",
    ));
    lines.push(Line::from(" "));

    let mut remaining = inner.height.saturating_sub(2) as usize;
    if state.openresponses_request_started_ms.is_some() {
        let headers = state
            .openresponses_headers_ms()
            .map(|ms| format!("{ms}ms"))
            .unwrap_or("-".to_string());
        let first_byte = state
            .openresponses_first_byte_ms()
            .map(|ms| format!("{ms}ms"))
            .unwrap_or("-".to_string());
        let first_event = state
            .openresponses_first_provider_event_ms()
            .map(|ms| format!("{ms}ms"))
            .unwrap_or("-".to_string());
        lines.push(Line::from(format!(
            "openresponses: headers={headers} first_byte={first_byte} first_event={first_event}"
        )));
        lines.push(Line::from(" "));
        remaining = remaining.saturating_sub(2);
    }

    lines.extend(build_activity_lines(state, remaining));
    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(widget, inner);
}
