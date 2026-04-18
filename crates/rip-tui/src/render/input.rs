use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use super::theme::ThemeStyles;
use super::util::truncate;

pub(super) fn render_input(frame: &mut Frame<'_>, theme: &ThemeStyles, area: Rect, input: &str) {
    let title = format!(
        "Input  {}",
        build_help_line(area.width.saturating_sub(12) as usize)
    );
    let widget = Paragraph::new(format!("> {input}"))
        .style(theme.accent)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Line::from(title).style(theme.header))
                .style(theme.chrome),
        );
    frame.render_widget(widget, area);
}

pub(super) fn build_help_line(max_width: usize) -> String {
    truncate(
        "Enter send  Ctrl-K palette  Wheel/Pg scroll  Ctrl-B activity  Ctrl-R xray",
        max_width,
    )
}
