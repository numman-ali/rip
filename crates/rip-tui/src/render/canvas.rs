use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::canvas::{
    Block as CanvasBlock, CachedText, CanvasMessage, ContextLifecycle, JobLifecycle, NoticeLevel,
    TaskCardStatus, ToolCardStatus,
};
use crate::TuiState;

use super::activity::render_activity_rail;
use super::input::render_input;
use super::status_bar::render_status_bar;
use super::theme::ThemeStyles;
use super::util::{canvas_scroll_offset, truncate};

const GUTTER_WIDTH: usize = 3;

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

    let text = build_canvas_text(state, theme);
    let scroll_text = plain_text(&text);
    let widget = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll(canvas_scroll_offset(state, panes[0], &scroll_text))
        .style(theme.chrome);
    frame.render_widget(widget, panes[0]);

    let chips = build_chips_line(state, panes[1].width as usize);
    let chip_widget = Paragraph::new(Line::from(chips)).style(theme.chrome);
    frame.render_widget(chip_widget, panes[1]);
}

/// Walk `state.canvas.messages` and render each as a gutter + body pair.
/// Replaces the old `output_text` + `prompt_ranges` renderer.
pub(super) fn build_canvas_text(state: &TuiState, theme: &ThemeStyles) -> Text<'static> {
    if state.canvas.messages.is_empty() {
        return Text::from("<no output yet>");
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (idx, message) in state.canvas.messages.iter().enumerate() {
        if idx > 0 {
            lines.push(Line::default());
        }
        append_message(&mut lines, message, theme);
    }
    Text::from(lines)
}

fn append_message(lines: &mut Vec<Line<'static>>, message: &CanvasMessage, theme: &ThemeStyles) {
    let (glyph, accent) = message_glyph(message, theme);
    let body_lines = message_body_lines(message, theme);

    for (row, body_line) in body_lines.into_iter().enumerate() {
        let mut line = Line::default();
        if row == 0 {
            line.spans.push(Span::styled(
                format!("{:<width$}", glyph, width = GUTTER_WIDTH),
                accent,
            ));
        } else {
            line.spans.push(Span::raw(" ".repeat(GUTTER_WIDTH)));
        }
        line.spans.extend(body_line.spans.into_iter());
        lines.push(line);
    }
}

/// Colors here are minimal — Phase C expands `ThemeStyles` with semantic
/// muted/error/warn/accent fields driven by the Graphite/Ink token sets.
/// For B.2 we pick restrained defaults that preserve readability across
/// existing terminals.
fn muted_style() -> Style {
    Style::default().fg(Color::DarkGray)
}

fn error_style() -> Style {
    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
}

fn warn_style() -> Style {
    Style::default().fg(Color::Yellow)
}

fn message_glyph(message: &CanvasMessage, theme: &ThemeStyles) -> (&'static str, Style) {
    match message {
        CanvasMessage::UserTurn { .. } => ("›", theme.prompt_label),
        CanvasMessage::AgentTurn { streaming, .. } => {
            let base = theme.chrome.add_modifier(Modifier::BOLD);
            if *streaming {
                ("◎", base)
            } else {
                ("◉", base)
            }
        }
        CanvasMessage::ToolCard { .. } => ("⟡", theme.chrome),
        CanvasMessage::TaskCard { .. } => ("⧉", theme.chrome),
        CanvasMessage::JobNotice { .. } => ("⧉", muted_style()),
        CanvasMessage::SystemNotice { level, .. } => match level {
            NoticeLevel::Danger => ("▲", error_style()),
            NoticeLevel::Warn => ("▲", warn_style()),
            _ => ("·", muted_style()),
        },
        CanvasMessage::ContextNotice { .. } => ("⌖", muted_style()),
        CanvasMessage::CompactionCheckpoint { .. } => ("·", muted_style()),
        CanvasMessage::ExtensionPanel { .. } => ("◈", theme.chrome),
    }
}

fn message_body_lines(message: &CanvasMessage, theme: &ThemeStyles) -> Vec<Line<'static>> {
    match message {
        CanvasMessage::UserTurn { blocks, .. } => {
            let text = paragraph_source(blocks);
            style_block_lines(&text, theme.prompt)
        }
        CanvasMessage::AgentTurn { blocks, .. } => {
            let text = paragraph_source(blocks);
            if text.is_empty() {
                Vec::new()
            } else {
                style_block_lines(&text, theme.chrome)
            }
        }
        CanvasMessage::ToolCard {
            tool_name, status, ..
        } => {
            let summary = format!("{tool_name} · {}", tool_card_status_label(status));
            style_block_lines(&summary, muted_style())
        }
        CanvasMessage::TaskCard {
            tool_name,
            title,
            status,
            ..
        } => {
            let label = title.as_deref().unwrap_or(tool_name);
            let summary = format!("{label} · {}", task_card_status_label(status));
            style_block_lines(&summary, muted_style())
        }
        CanvasMessage::JobNotice {
            job_kind, status, ..
        } => {
            let summary = format!("{job_kind} · {}", job_status_label(status));
            style_block_lines(&summary, muted_style())
        }
        CanvasMessage::SystemNotice { text, level, .. } => {
            let style = match level {
                NoticeLevel::Danger => error_style(),
                NoticeLevel::Warn => warn_style(),
                _ => muted_style(),
            };
            style_block_lines(text, style)
        }
        CanvasMessage::ContextNotice {
            strategy, status, ..
        } => {
            let label = match status {
                ContextLifecycle::Selecting => "selecting",
                ContextLifecycle::Compiled => "compiled",
            };
            style_block_lines(&format!("context {strategy} · {label}"), muted_style())
        }
        CanvasMessage::CompactionCheckpoint {
            from_seq, to_seq, ..
        } => style_block_lines(
            &format!("compaction checkpoint · seq {from_seq}…{to_seq}"),
            muted_style(),
        ),
        CanvasMessage::ExtensionPanel { title, .. } => {
            style_block_lines(&format!("extension: {title}"), theme.chrome)
        }
    }
}

fn paragraph_source(blocks: &[CanvasBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        let piece = match block {
            CanvasBlock::Paragraph(text) | CanvasBlock::Markdown(text) => cached_source(text),
            CanvasBlock::Heading { text, .. } => cached_source(text),
            CanvasBlock::CodeFence { text, .. } => cached_source(text),
            CanvasBlock::ToolArgsJson(text) => cached_source(text),
            CanvasBlock::ToolStdout(text) | CanvasBlock::ToolStderr(text) => cached_source(text),
            CanvasBlock::ArtifactChip { artifact_id, .. } => {
                let short: String = artifact_id.chars().take(8).collect();
                format!("⧉ {short}")
            }
            CanvasBlock::Thematic => "────".to_string(),
            CanvasBlock::BlockQuote(_) | CanvasBlock::List { .. } => String::new(),
        };
        if piece.is_empty() {
            continue;
        }
        out.push_str(&piece);
    }
    out
}

fn cached_source(text: &CachedText) -> String {
    let mut out = String::new();
    for (i, line) in text.text.lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        for span in &line.spans {
            out.push_str(span.content.as_ref());
        }
    }
    out
}

fn style_block_lines(source: &str, style: Style) -> Vec<Line<'static>> {
    if source.is_empty() {
        return vec![Line::default()];
    }
    source
        .split('\n')
        .map(|line| Line::from(Span::styled(line.to_string(), style)))
        .collect()
}

fn tool_card_status_label(status: &ToolCardStatus) -> String {
    match status {
        ToolCardStatus::Running => "running".to_string(),
        ToolCardStatus::Succeeded { duration_ms, .. } => format!("✓ {duration_ms}ms"),
        ToolCardStatus::Failed { error } => format!("✕ {error}"),
    }
}

fn task_card_status_label(status: &TaskCardStatus) -> String {
    match status {
        TaskCardStatus::Queued => "queued".to_string(),
        TaskCardStatus::Running => "running".to_string(),
        TaskCardStatus::Exited { exit_code } => match exit_code {
            Some(code) => format!("exited {code}"),
            None => "exited".to_string(),
        },
        TaskCardStatus::Cancelled => "cancelled".to_string(),
        TaskCardStatus::Failed { error } => match error.as_deref() {
            Some(err) => format!("failed · {err}"),
            None => "failed".to_string(),
        },
    }
}

fn job_status_label(status: &JobLifecycle) -> &'static str {
    match status {
        JobLifecycle::Running => "running",
        JobLifecycle::Succeeded { .. } => "succeeded",
        JobLifecycle::Failed { .. } => "failed",
        JobLifecycle::Cancelled => "cancelled",
    }
}

/// Flatten a styled `Text` back to a newline-joined string so
/// `canvas_scroll_offset` can compute wrapped-line counts without needing
/// to know about styles.
fn plain_text(text: &Text<'_>) -> String {
    let mut out = String::new();
    for (i, line) in text.lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        for span in &line.spans {
            out.push_str(span.content.as_ref());
        }
    }
    out
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
