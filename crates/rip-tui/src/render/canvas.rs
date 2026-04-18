use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::TuiState;

use super::activity::render_activity_rail;
use super::input::render_input;
use super::status_bar::render_status_bar;
use super::theme::ThemeStyles;
use super::util::{canvas_scroll_offset, truncate};

pub(super) fn render_canvas_screen(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    input: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(frame.area());

    render_status_bar(frame, state, theme, chunks[0]);
    render_canvas_body(frame, state, theme, chunks[1]);
    render_input(frame, theme, chunks[2], input);
}

pub(super) fn render_canvas_body(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    if state.activity_pinned && area.width >= 100 {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(40), Constraint::Length(32)])
            .split(area);
        render_canvas(frame, state, theme, panes[0]);
        render_activity_rail(frame, state, theme, panes[1]);
    } else {
        render_canvas(frame, state, theme, area);
    }
}

fn render_canvas(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from("Canvas").style(theme.header))
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let panes = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);

    let widget = Paragraph::new(build_canvas_text(state, theme))
        .wrap(Wrap { trim: false })
        .scroll(canvas_scroll_offset(state, panes[0], &state.output_text))
        .style(theme.chrome);
    frame.render_widget(widget, panes[0]);

    let chips = build_chips_line(state, panes[1].width as usize);
    let chip_widget = Paragraph::new(Line::from(chips)).style(theme.chrome);
    frame.render_widget(chip_widget, panes[1]);
}

pub(super) fn build_canvas_text(state: &TuiState, theme: &ThemeStyles) -> Text<'static> {
    if state.output_text.is_empty() {
        return Text::from("<no output yet>");
    }

    build_styled_canvas_text(&state.output_text, state.prompt_ranges(), theme)
}

fn build_styled_canvas_text(
    text: &str,
    prompt_ranges: &[(usize, usize)],
    theme: &ThemeStyles,
) -> Text<'static> {
    let mut lines = vec![Line::default()];
    let mut cursor = 0usize;

    for &(start, end) in prompt_ranges {
        if start > cursor {
            push_text_segment(&mut lines, &text[cursor..start], theme.chrome);
        }
        if end > start {
            push_prompt_segment(&mut lines, &text[start..end], theme);
        }
        cursor = end;
    }

    if cursor < text.len() {
        push_text_segment(&mut lines, &text[cursor..], theme.chrome);
    }

    Text::from(lines)
}

fn push_prompt_segment(lines: &mut Vec<Line<'static>>, segment: &str, theme: &ThemeStyles) {
    for piece in segment.split_inclusive('\n') {
        let trailing_newline = piece.ends_with('\n');
        let content = piece.strip_suffix('\n').unwrap_or(piece);
        if let Some(rest) = content.strip_prefix("You: ") {
            push_span(lines, "You: ".to_string(), theme.prompt_label);
            if !rest.is_empty() {
                push_span(lines, rest.to_string(), theme.prompt);
            }
        } else if !content.is_empty() {
            push_span(lines, content.to_string(), theme.prompt);
        }

        if trailing_newline {
            lines.push(Line::default());
        }
    }
}

fn push_text_segment(lines: &mut Vec<Line<'static>>, segment: &str, style: Style) {
    for piece in segment.split_inclusive('\n') {
        let trailing_newline = piece.ends_with('\n');
        let content = piece.strip_suffix('\n').unwrap_or(piece);
        if !content.is_empty() {
            push_span(lines, content.to_string(), style);
        }

        if trailing_newline {
            lines.push(Line::default());
        }
    }
}

fn push_span(lines: &mut Vec<Line<'static>>, content: String, style: Style) {
    if lines.is_empty() {
        lines.push(Line::default());
    }
    lines
        .last_mut()
        .expect("line")
        .spans
        .push(Span::styled(content, style));
}

pub(super) fn build_chips_line(state: &TuiState, max_width: usize) -> String {
    let mut chips: Vec<String> = Vec::new();

    if state.awaiting_response {
        let waiting = if state.openresponses_first_provider_event_ms.is_some() {
            "working"
        } else if state.openresponses_request_started_ms.is_some() {
            "waiting"
        } else {
            "sending"
        };
        chips.push(format!("[◔ {waiting}]"));
    }

    let running_tools: Vec<&str> = state.running_tool_ids().collect();
    if !running_tools.is_empty() {
        let name = state
            .tools
            .get(running_tools[0])
            .map(|t| t.name.as_str())
            .unwrap_or("tool");
        chips.push(format!("[⟳ {name}]"));
        if running_tools.len() > 1 {
            chips.push(format!("[+{}]", running_tools.len() - 1));
        }
    } else if let Some(tool) = state.tools.values().max_by_key(|tool| tool.started_seq) {
        let chip = match &tool.status {
            crate::ToolStatus::Ended { .. } => format!("[✓ {}]", tool.name),
            crate::ToolStatus::Failed { .. } => format!("[✕ {}]", tool.name),
            crate::ToolStatus::Running => format!("[⟳ {}]", tool.name),
        };
        chips.push(chip);
    }

    let running_tasks = state.running_task_ids().count();
    if running_tasks > 0 {
        chips.push(format!("[tasks:{running_tasks}/{}]", state.tasks.len()));
    } else if !state.tasks.is_empty() {
        chips.push(format!("[tasks:{}]", state.tasks.len()));
    }

    let running_jobs = state.running_job_ids().count();
    if running_jobs > 0 {
        chips.push(format!("[jobs:{running_jobs}/{}]", state.jobs.len()));
    } else if !state.jobs.is_empty() {
        chips.push(format!("[jobs:{}]", state.jobs.len()));
    }

    if let Some(ctx) = state.context.as_ref() {
        let status = match ctx.status {
            crate::ContextStatus::Selecting => "ctx:selecting",
            crate::ContextStatus::Compiled => "ctx:compiled",
        };
        chips.push(format!("[⚙ {status}]"));
    }

    if !state.artifacts.is_empty() {
        chips.push(format!("[📄{}]", state.artifacts.len()));
    }

    if state.is_stalled(5_000) {
        chips.push("[⏸ stalled]".to_string());
    }

    if state.has_error() {
        chips.push("[⚠ error]".to_string());
    }

    let mut out = String::from("chips: ");
    out.push_str(&chips.join(" "));
    if out.chars().count() > max_width {
        out = truncate(&out, max_width);
    }
    out
}
