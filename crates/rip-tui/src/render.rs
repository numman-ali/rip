use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table, TableState, Tabs, Wrap};
use ratatui::Frame;
use serde_json::Value;

use crate::summary::{event_summary, event_type};
use crate::TuiState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Json,
    Decoded,
}

pub fn render(frame: &mut Frame<'_>, state: &TuiState, mode: RenderMode) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(6),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_status_bar(frame, state, chunks[0]);
    render_main_panes(frame, state, mode, chunks[1]);
    render_output(frame, state, chunks[2]);
    render_input(frame, chunks[3]);
}

fn render_status_bar(frame: &mut Frame<'_>, state: &TuiState, area: Rect) {
    let session = state.session_id.as_deref().unwrap_or("-");
    let last_seq = state
        .frames
        .last_seq()
        .map(|seq| seq.to_string())
        .unwrap_or("-".to_string());
    let ttft = state
        .ttft_ms()
        .map(|ms| format!("{ms}ms"))
        .unwrap_or("-".to_string());
    let e2e = state
        .e2e_ms()
        .map(|ms| format!("{ms}ms"))
        .unwrap_or("-".to_string());

    let line = Line::from(format!(
        " session:{session}  seq:{last_seq}  TTFT:{ttft}  E2E:{e2e} "
    ));
    let widget = Paragraph::new(line).block(Block::default().borders(Borders::ALL).title("RIP"));
    frame.render_widget(widget, area);
}

fn render_main_panes(frame: &mut Frame<'_>, state: &TuiState, mode: RenderMode, area: Rect) {
    let (left_pct, right_pct) = if area.width < 80 { (50, 50) } else { (40, 60) };
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);

    render_timeline(frame, state, panes[0]);
    render_details(frame, state, mode, panes[1]);
}

fn render_timeline(frame: &mut Frame<'_>, state: &TuiState, area: Rect) {
    let mut rows: Vec<Row<'static>> = Vec::new();
    for event in state.frames.iter() {
        let seq = event.seq.to_string();
        let kind = event_type(event).to_string();
        let summary = event_summary(event);
        rows.push(Row::new(vec![seq, kind, summary]));
    }

    let header = Row::new(vec!["seq", "type", "summary"])
        .style(Style::default().add_modifier(Modifier::BOLD));
    let table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Length(14),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title("Timeline"))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol("▸ ");

    let mut table_state = TableState::default();
    if let Some(selected_seq) = state.selected_seq {
        if let Some(idx) = state.frames.index_of_seq(selected_seq) {
            table_state.select(Some(idx));
        }
    }
    frame.render_stateful_widget(table, area, &mut table_state);
}

fn render_details(frame: &mut Frame<'_>, state: &TuiState, mode: RenderMode, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title("Details");
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

fn selected_event_json(state: &TuiState) -> Text<'static> {
    let Some(event) = state.selected_event() else {
        return Text::from("");
    };
    match serde_json::to_string_pretty(event) {
        Ok(json) => Text::from(json),
        Err(_) => Text::from("<failed to render json>"),
    }
}

fn selected_event_decoded(state: &TuiState) -> Text<'static> {
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

fn render_output(frame: &mut Frame<'_>, state: &TuiState, area: Rect) {
    let mut title = "Output".to_string();
    if state.output_truncated {
        title.push_str(" (truncated)");
    }
    let widget = Paragraph::new(state.output_text.as_str())
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_input(frame: &mut Frame<'_>, area: Rect) {
    let widget = Paragraph::new("> ").block(Block::default().borders(Borders::ALL).title("Input"));
    frame.render_widget(widget, area);
}
