use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::{Overlay, TuiState};

use super::super::theme::ThemeStyles;
use super::super::util::truncate;

pub(super) fn render_thread_picker_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    let Overlay::ThreadPicker(picker) = state.overlay() else {
        return;
    };

    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Threads")
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(inner);

    let header = Paragraph::new(Text::from(vec![
        Line::from("Pick the continuity to target on the next run."),
        Line::from("Any metadata the runtime does not expose yet renders as —."),
    ]))
    .style(theme.muted)
    .wrap(Wrap { trim: false });
    frame.render_widget(header, sections[0]);

    let visible_rows = sections[1].height.max(1) as usize;
    let start = picker
        .selected
        .saturating_sub(visible_rows.saturating_sub(1) / 2)
        .min(picker.entries.len().saturating_sub(visible_rows));
    let end = (start + visible_rows).min(picker.entries.len());

    let mut lines: Vec<Line<'static>> = Vec::new();
    if picker.entries.is_empty() {
        lines.push(Line::from(Span::styled("No threads yet.", theme.muted)));
    } else {
        for (visible_idx, entry) in picker.entries[start..end].iter().enumerate() {
            let selected = start + visible_idx == picker.selected;
            let marker = if selected { "›" } else { " " };
            let title_style = if selected {
                theme.highlight
            } else {
                theme.header
            };
            let body_style = if selected { theme.chrome } else { theme.muted };

            lines.push(Line::from(vec![
                Span::styled(format!("{marker} "), title_style),
                Span::styled(
                    truncate(&entry.title, sections[1].width.saturating_sub(4) as usize),
                    title_style,
                ),
            ]));

            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    truncate(&entry.preview, sections[1].width.saturating_sub(4) as usize),
                    body_style,
                ),
            ]));

            let chip_text = if entry.chips.is_empty() {
                "size —  actors —".to_string()
            } else {
                entry.chips.join("  ")
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    truncate(&chip_text, sections[1].width.saturating_sub(4) as usize),
                    if selected { theme.accent } else { theme.quiet },
                ),
            ]));

            if start + visible_idx + 1 < end {
                lines.push(Line::default());
            }
        }
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .style(theme.chrome)
            .wrap(Wrap { trim: false }),
        sections[1],
    );

    let footer = Paragraph::new(Text::from(vec![
        Line::from("Enter target next run  Esc close"),
        Line::from("Up/Down move  PageUp/PageDown scroll"),
    ]))
    .style(theme.chrome)
    .wrap(Wrap { trim: false });
    frame.render_widget(footer, sections[2]);
}
