use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table, TableState, Tabs, Wrap};
use ratatui::Frame;
use serde_json::Value;

use crate::summary::{event_summary, event_type};
use crate::{OutputViewMode, ThemeId, TuiState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Json,
    Decoded,
}

pub fn render(frame: &mut Frame<'_>, state: &TuiState, mode: RenderMode, input: &str) {
    let theme = ThemeStyles::for_theme(state.theme);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(6),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_status_bar(frame, state, &theme, chunks[0]);
    render_main_panes(frame, state, &theme, mode, chunks[1]);
    render_output(frame, state, &theme, chunks[2]);
    render_input(frame, &theme, chunks[3], input);
}

#[derive(Debug, Clone, Copy)]
struct ThemeStyles {
    chrome: Style,
    header: Style,
    highlight: Style,
}

impl ThemeStyles {
    fn for_theme(theme: ThemeId) -> Self {
        match theme {
            ThemeId::DefaultDark => Self {
                chrome: Style::default().fg(Color::White),
                header: Style::default().add_modifier(Modifier::BOLD),
                highlight: Style::default().add_modifier(Modifier::REVERSED),
            },
            ThemeId::DefaultLight => Self {
                chrome: Style::default().fg(Color::Black),
                header: Style::default()
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
                highlight: Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD),
            },
        }
    }
}

fn render_status_bar(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, area: Rect) {
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

    let view = state.output_view.as_str();
    let theme_name = state.theme.as_str();

    let mut line = String::new();
    if let Some(msg) = state.status_message.as_deref() {
        line.push_str(" msg:");
        line.push_str(msg);
        line.push_str(" |");
    }
    line.push_str(&format!(
        " session:{session}  seq:{last_seq}  TTFT:{ttft}  E2E:{e2e}  view:{view}  theme:{theme_name}"
    ));
    let widget = Paragraph::new(Line::from(line))
        .style(theme.chrome)
        .block(Block::default().borders(Borders::ALL).title("RIP"));
    frame.render_widget(widget, area);
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
    .block(Block::default().borders(Borders::ALL).title("Timeline"))
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
        .title("Details")
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

fn selected_event_json(state: &TuiState) -> Text<'static> {
    let Some(event) = state.selected_event() else {
        return Text::from("<no frame selected>");
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

fn render_output(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, area: Rect) {
    let (mut title, content) = match state.output_view {
        OutputViewMode::Rendered => ("Output".to_string(), Text::from(state.output_text.as_str())),
        OutputViewMode::Raw => ("Raw".to_string(), selected_event_json(state)),
    };

    if state.output_truncated && state.output_view == OutputViewMode::Rendered {
        title.push_str(" (truncated)");
    }

    let widget = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(theme.chrome),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_input(frame: &mut Frame<'_>, theme: &ThemeStyles, area: Rect, input: &str) {
    let widget = Paragraph::new(format!("> {input}")).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Input")
            .style(theme.chrome),
    );
    frame.render_widget(widget, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use rip_kernel::{Event, EventKind};

    fn event(seq: u64, kind: EventKind) -> Event {
        Event {
            id: format!("e{seq}"),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq,
            kind,
        }
    }

    fn render_once(state: &TuiState, mode: RenderMode, width: u16) {
        let mut terminal = Terminal::new(TestBackend::new(width, 20)).expect("terminal");
        terminal.draw(|f| render(f, state, mode, "")).expect("draw");
    }

    #[test]
    fn render_handles_empty_state_small_width() {
        let state = TuiState::new(100, 1024);
        render_once(&state, RenderMode::Json, 60);
    }

    #[test]
    fn render_handles_decoded_mode_and_truncated_output() {
        let mut state = TuiState::new(100, 16);
        state.update(event(
            0,
            EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        ));
        state.update(event(
            1,
            EventKind::OutputTextDelta {
                delta: "hello".to_string(),
            },
        ));
        state.output_truncated = true;
        state.output_text = "partial".to_string();
        render_once(&state, RenderMode::Decoded, 100);
    }
}
