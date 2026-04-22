use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::TuiState;

use super::super::theme::ThemeStyles;

pub(super) fn render_stall_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Stalled")
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let last_seq = state.frames.last_seq().unwrap_or(0);
    let last_ms = state.last_event_ms.unwrap_or(0);
    let now_ms = state.now_ms.unwrap_or(0);
    let delta_ms = now_ms.saturating_sub(last_ms);

    let lines = vec![
        Line::from("No new frames recently."),
        Line::from(format!("last_seq: {last_seq}")),
        Line::from(format!("idle_ms: {delta_ms}")),
        Line::from(" "),
        Line::from("Safe actions: cancel run, retry, or inspect last error."),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .scroll((state.overlay_scroll, 0))
            .wrap(Wrap { trim: false })
            .style(theme.chrome),
        inner,
    );
}
