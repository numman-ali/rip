use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::summary::{event_summary, event_type};
use crate::TuiState;

use super::super::theme::ThemeStyles;
use super::super::util::truncate;

pub(super) fn render_error_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
    seq: u64,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Error Detail")
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(event) = state.frames.get_by_seq(seq) else {
        frame.render_widget(
            Paragraph::new(Text::from("<missing error frame>")).style(theme.chrome),
            inner,
        );
        return;
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(format!("seq: {}", event.seq)));
    lines.push(Line::from(format!("type: {}", event_type(event))));
    lines.push(Line::from(format!("summary: {}", event_summary(event))));

    match &event.kind {
        rip_kernel::EventKind::ToolFailed { error, .. } => {
            lines.push(Line::from(" "));
            lines.push(Line::from(format!("error: {}", truncate(error, 200))));
        }
        rip_kernel::EventKind::ProviderEvent {
            status,
            errors,
            response_errors,
            raw,
            ..
        } => {
            lines.push(Line::from(" "));
            lines.push(Line::from(format!("provider_status: {status:?}")));
            if !errors.is_empty() {
                lines.push(Line::from(format!("errors: {}", errors.len())));
                for e in errors.iter().take(4) {
                    lines.push(Line::from(format!("- {}", truncate(e, 120))));
                }
            }
            if !response_errors.is_empty() {
                lines.push(Line::from(format!(
                    "response_errors: {}",
                    response_errors.len()
                )));
                for e in response_errors.iter().take(4) {
                    lines.push(Line::from(format!("- {}", truncate(e, 120))));
                }
            }
            if let Some(raw) = raw.as_deref() {
                lines.push(Line::from("raw (preview):"));
                for line in raw.lines().take(6) {
                    lines.push(Line::from(truncate(line, 120)));
                }
            }
            lines.push(Line::from(" "));
            if let Some(session_id) = state.session_id.as_deref() {
                lines.push(Line::from(format!("session: {}", truncate(session_id, 40))));
            }
        }
        _ => {}
    }

    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(widget, inner);
}
