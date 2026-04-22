use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::{Overlay, ThreadPickerState, TuiState};

use super::super::theme::ThemeStyles;
use super::super::util::truncate;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ThreadPickerMouseTarget {
    Outside,
    Inside,
    Entry(usize),
}

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
    let sections = thread_picker_sections(inner);

    let header = Paragraph::new(Text::from(vec![
        Line::from("Pick the continuity to target on the next run."),
        Line::from("Any metadata the runtime does not expose yet renders as —."),
    ]))
    .style(theme.muted)
    .wrap(Wrap { trim: false });
    frame.render_widget(header, sections[0]);

    let visible_rows = sections[1].height.max(1) as usize;
    let start = thread_picker_visible_start(picker, visible_rows);
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
        Line::from("Enter target next run  click row apply  Esc close"),
        Line::from("Up/Down move  PageUp/PageDown scroll  click outside close"),
    ]))
    .style(theme.chrome)
    .wrap(Wrap { trim: false });
    frame.render_widget(footer, sections[2]);
}

pub(super) fn thread_picker_mouse_target(
    picker: &ThreadPickerState,
    area: Rect,
    column: u16,
    row: u16,
) -> ThreadPickerMouseTarget {
    if !point_in_rect(area, column, row) {
        return ThreadPickerMouseTarget::Outside;
    }

    let inner = Block::default().borders(Borders::ALL).inner(area);
    let sections = thread_picker_sections(inner);
    if !point_in_rect(sections[1], column, row) {
        return ThreadPickerMouseTarget::Inside;
    }

    let visible_rows = sections[1].height.max(1) as usize;
    let start = thread_picker_visible_start(picker, visible_rows);
    let end = (start + visible_rows).min(picker.entries.len());
    let mut current_row = sections[1].y;

    for idx in start..end {
        let last = idx + 1 == end;
        let entry_height = if last { 3 } else { 4 };
        if row >= current_row && row < current_row.saturating_add(entry_height) {
            let offset = row.saturating_sub(current_row);
            return if offset < 3 {
                ThreadPickerMouseTarget::Entry(idx)
            } else {
                ThreadPickerMouseTarget::Inside
            };
        }
        current_row = current_row.saturating_add(entry_height);
    }

    ThreadPickerMouseTarget::Inside
}

fn point_in_rect(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

fn thread_picker_sections(inner: Rect) -> [Rect; 3] {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(inner);
    [sections[0], sections[1], sections[2]]
}

fn thread_picker_visible_start(picker: &ThreadPickerState, visible_rows: usize) -> usize {
    picker
        .selected
        .saturating_sub(visible_rows.saturating_sub(1) / 2)
        .min(picker.entries.len().saturating_sub(visible_rows))
}
