use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Wrap};
use ratatui::Frame;

use crate::{Overlay, TuiState};

use super::super::theme::ThemeStyles;
use super::super::util::truncate;

pub(super) fn render_palette_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    let Overlay::Palette(palette) = &state.overlay else {
        return;
    };

    frame.render_widget(Clear, area);
    let title = format!("Palette · {}", palette.mode.label());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(inner);

    let tabs = Tabs::new(vec![Line::from(palette.mode.label())])
        .select(0)
        .highlight_style(theme.highlight)
        .style(theme.chrome);
    frame.render_widget(tabs, sections[0]);

    let query_text = if palette.query.trim().is_empty() {
        "> type to filter".to_string()
    } else {
        format!("> {}", palette.query)
    };
    let query = Paragraph::new(query_text).style(theme.accent).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Filter")
            .style(theme.chrome),
    );
    frame.render_widget(query, sections[1]);

    let filtered = palette.filtered_indices();
    let visible_rows = sections[2].height.max(1) as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    if filtered.is_empty() {
        if let Some(custom) = palette.custom_candidate() {
            lines.push(Line::from(vec![
                Span::styled("› ", theme.highlight),
                Span::styled(
                    truncate(
                        &format!("{}: {}", palette.custom_prompt, custom),
                        sections[2].width.saturating_sub(4) as usize,
                    ),
                    theme.highlight,
                ),
            ]));
        } else {
            lines.push(Line::from(truncate(
                &palette.empty_message,
                sections[2].width.saturating_sub(2) as usize,
            )));
        }
    } else {
        let start = palette
            .selected
            .saturating_sub(visible_rows.saturating_sub(1) / 2)
            .min(filtered.len().saturating_sub(visible_rows));
        let end = (start + visible_rows).min(filtered.len());
        for (visible_idx, entry_idx) in filtered[start..end].iter().enumerate() {
            let entry = &palette.entries[*entry_idx];
            let selected = start + visible_idx == palette.selected;
            let mut line = entry.title.clone();
            if let Some(subtitle) = entry.subtitle.as_deref().filter(|value| !value.is_empty()) {
                line.push_str("  ");
                line.push_str(subtitle);
            }
            if !entry.chips.is_empty() {
                line.push_str("  ");
                for (idx, chip) in entry.chips.iter().enumerate() {
                    if idx > 0 {
                        line.push(' ');
                    }
                    line.push('[');
                    line.push_str(chip);
                    line.push(']');
                }
            }
            let style = if selected {
                theme.highlight
            } else {
                theme.chrome
            };
            let prefix = if selected { "› " } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(
                    truncate(&line, sections[2].width.saturating_sub(4) as usize),
                    style,
                ),
            ]));
        }
    }

    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .style(theme.chrome),
        sections[2],
    );

    let footer = Paragraph::new(Text::from(vec![
        Line::from("Enter apply  Esc close  Type to filter"),
        Line::from("Ctrl-K reopens model switching for future palette modes"),
    ]))
    .style(theme.chrome)
    .wrap(Wrap { trim: false });
    frame.render_widget(footer, sections[3]);
}
