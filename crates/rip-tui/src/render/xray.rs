use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table, TableState, Tabs, Wrap};
use ratatui::Frame;
use serde_json::Value;

use crate::summary::{event_summary, event_type};
use crate::{OutputViewMode, TuiState};

use super::input::render_input;
use super::status_bar::render_status_bar;
use super::theme::ThemeStyles;
use super::util::canvas_scroll_offset;
use super::RenderMode;

pub(super) fn render_xray_screen(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    mode: RenderMode,
    input: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(6),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_status_bar(frame, state, theme, chunks[0]);
    render_main_panes(frame, state, theme, mode, chunks[1]);
    render_output(frame, state, theme, chunks[2]);
    render_input(frame, theme, chunks[3], input);
}

fn render_main_panes(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    mode: RenderMode,
    area: Rect,
) {
    let (left_pct, right_pct) = if area.width < 80 { (50, 50) } else { (40, 60) };
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);

    render_timeline(frame, state, theme, panes[0]);
    render_details(frame, state, theme, mode, panes[1]);
}

fn render_timeline(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, area: Rect) {
    let mut rows: Vec<Row<'static>> = Vec::new();
    for event in state.frames.iter() {
        let seq = event.seq.to_string();
        let kind = event_type(event).to_string();
        let summary = event_summary(event);
        rows.push(Row::new(vec![seq, kind, summary]));
    }

    let header = Row::new(vec!["seq", "type", "summary"]).style(theme.header);
    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(14),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(Line::from("Timeline").style(theme.header)),
    )
    .row_highlight_style(theme.highlight)
    .highlight_symbol("▸ ");

    let mut table_state = TableState::default();
    if let Some(selected_seq) = state.selected_seq {
        if let Some(idx) = state.frames.index_of_seq(selected_seq) {
            table_state.select(Some(idx));
        }
    }
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn render_details(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    mode: RenderMode,
    area: Rect,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from("Details").style(theme.header))
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let panes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    let tabs =
        Tabs::new(vec!["JSON", "Decoded"]).select(if mode == RenderMode::Decoded { 1 } else { 0 });
    frame.render_widget(tabs, panes[0]);

    let content = match mode {
        RenderMode::Json => selected_event_json(state),
        RenderMode::Decoded => selected_event_decoded(state),
    };

    let widget = Paragraph::new(content).wrap(Wrap { trim: false });
    frame.render_widget(widget, panes[1]);
}

pub(super) fn selected_event_json(state: &TuiState) -> Text<'static> {
    let Some(event) = state.selected_event() else {
        return Text::from("<no frame selected>");
    };
    match serde_json::to_string_pretty(event) {
        Ok(json) => Text::from(json),
        Err(_) => Text::from("<failed to render json>"),
    }
}

pub(super) fn selected_event_decoded(state: &TuiState) -> Text<'static> {
    let Some(event) = state.selected_event() else {
        return Text::from("");
    };
    let summary = event_summary(event);
    let kind = event_type(event);
    let mut object = serde_json::Map::<String, Value>::new();
    object.insert("seq".to_string(), Value::Number(event.seq.into()));
    object.insert("type".to_string(), Value::String(kind.to_string()));
    object.insert("summary".to_string(), Value::String(summary));
    Text::from(serde_json::to_string_pretty(&Value::Object(object)).unwrap_or_default())
}

fn render_output(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, area: Rect) {
    let (title, content_text) = match state.output_view {
        OutputViewMode::Rendered => ("Output".to_string(), state.rendered_agent_text()),
        OutputViewMode::Raw => ("Raw".to_string(), selected_event_json(state).to_string()),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from(title).style(theme.header))
        .style(theme.chrome);
    let inner = block.inner(area);
    let mut widget = Paragraph::new(Text::from(content_text.clone()))
        .block(block)
        .wrap(Wrap { trim: false });
    if state.output_view == OutputViewMode::Rendered {
        widget = widget.scroll(canvas_scroll_offset(state, inner, &content_text));
    }
    frame.render_widget(widget, area);
}
