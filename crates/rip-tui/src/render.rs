use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table, TableState, Tabs, Wrap};
use ratatui::Frame;
use serde_json::Value;

use crate::summary::{event_summary, event_type};
use crate::{OutputViewMode, Overlay, ThemeId, TuiState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Json,
    Decoded,
}

pub fn render(frame: &mut Frame<'_>, state: &TuiState, mode: RenderMode, input: &str) {
    let theme = ThemeStyles::for_theme(state.theme);
    match state.output_view {
        OutputViewMode::Rendered => render_canvas_screen(frame, state, &theme, input),
        OutputViewMode::Raw => render_xray_screen(frame, state, &theme, mode, input),
    }

    if state.overlay != Overlay::None {
        render_overlay(frame, state, &theme, mode);
    }
}

#[derive(Debug, Clone, Copy)]
struct ThemeStyles {
    chrome: Style,
    header: Style,
    highlight: Style,
    accent: Style,
}

impl ThemeStyles {
    fn for_theme(theme: ThemeId) -> Self {
        match theme {
            ThemeId::DefaultDark => Self {
                chrome: Style::default().fg(Color::Gray),
                header: Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                highlight: Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
                accent: Style::default().fg(Color::LightBlue),
            },
            ThemeId::DefaultLight => Self {
                chrome: Style::default().fg(Color::Black),
                header: Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
                highlight: Style::default()
                    .fg(Color::Black)
                    .bg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
                accent: Style::default().fg(Color::Blue),
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

    let view = match state.output_view {
        OutputViewMode::Rendered => "canvas",
        OutputViewMode::Raw => "xray",
    };
    let theme_name = state.theme.as_str();

    let tool_count = state.running_tool_ids().count();
    let task_count = state.running_task_ids().count();
    let job_count = state.running_job_ids().count();
    let artifact_count = state.artifacts.len();
    let stalled = state.is_stalled(5_000);
    let error = state.has_error();
    let headers = fmt_ms(state.openresponses_headers_ms());
    let first_byte = fmt_ms(state.openresponses_first_byte_ms());
    let provider_event = fmt_ms(state.openresponses_first_provider_event_ms());
    let llm = state
        .openresponses_endpoint
        .as_deref()
        .map(|endpoint| {
            if endpoint.contains("openrouter.ai") {
                "openrouter"
            } else if endpoint.contains("api.openai.com") || endpoint.contains("openai.com") {
                "openai"
            } else {
                "openresponses"
            }
        })
        .map(|provider| match state.openresponses_model.as_deref() {
            Some(model) if !model.trim().is_empty() => format!("{provider}:{model}"),
            _ => provider.to_string(),
        });

    let mut line = String::new();
    if let Some(msg) = state.status_message.as_deref() {
        line.push_str(" msg:");
        line.push_str(msg);
        line.push_str(" |");
    }
    if let Some(llm) = llm.as_deref() {
        line.push_str(" llm:");
        line.push_str(llm);
        line.push_str(" |");
    }
    line.push_str(&format!(
        " view:{view}  session:{session}  seq:{last_seq}  hdr:{headers}  fb:{first_byte}  evt:{provider_event}  TTFT:{ttft}  E2E:{e2e}  tools:{tool_count}/{}  tasks:{task_count}/{}  jobs:{job_count}/{}  arts:{artifact_count}  stalled:{stalled}  error:{error}  theme:{theme_name}",
        state.tools.len(),
        state.tasks.len(),
        state.jobs.len()
    ));
    let widget = Paragraph::new(Line::from(line)).style(theme.chrome).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Line::from("RIP").style(theme.header)),
    );
    frame.render_widget(widget, area);
}

fn render_canvas_screen(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, input: &str) {
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

fn render_canvas_body(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, area: Rect) {
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

    let story = if state.output_text.is_empty() {
        "<no output yet>".to_string()
    } else {
        state.output_text.clone()
    };
    let widget = Paragraph::new(Text::from(story))
        .wrap(Wrap { trim: false })
        .scroll(canvas_scroll_offset(state, panes[0], &state.output_text))
        .style(theme.chrome);
    frame.render_widget(widget, panes[0]);

    let chips = build_chips_line(state, panes[1].width as usize);
    let chip_widget = Paragraph::new(Line::from(chips)).style(theme.chrome);
    frame.render_widget(chip_widget, panes[1]);
}

fn render_activity_rail(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Line::from("Activity").style(theme.header))
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = build_activity_lines(state, inner.height as usize);
    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(widget, inner);
}

fn build_activity_lines(state: &TuiState, max_lines: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    if state.awaiting_response {
        lines.push(Line::from("◔ waiting for response"));
    }

    if state.has_error() {
        if let Some(seq) = state.last_error_seq {
            lines.push(Line::from(format!("⚠ error @seq {seq}")));
        } else {
            lines.push(Line::from("⚠ error"));
        }
    }

    if state.is_stalled(5_000) {
        lines.push(Line::from("⏸ stalled"));
    }

    for tool in state.tools.values() {
        if matches!(tool.status, crate::ToolStatus::Running) {
            lines.push(Line::from(format!("⟳ tool {}", tool.name)));
        }
    }

    if !state.tools.is_empty()
        && !state
            .tools
            .values()
            .any(|tool| matches!(tool.status, crate::ToolStatus::Running))
    {
        if let Some(tool) = state.tools.values().max_by_key(|tool| tool.started_seq) {
            let label = match &tool.status {
                crate::ToolStatus::Ended { .. } => "✓",
                crate::ToolStatus::Failed { .. } => "✕",
                crate::ToolStatus::Running => "⟳",
            };
            lines.push(Line::from(format!("{label} tool {}", tool.name)));
        }
    }

    for task in state.tasks.values() {
        if matches!(
            task.status,
            rip_kernel::ToolTaskStatus::Queued | rip_kernel::ToolTaskStatus::Running
        ) {
            let title = task
                .title
                .as_deref()
                .filter(|t| !t.is_empty())
                .unwrap_or(task.tool_name.as_str());
            lines.push(Line::from(format!("⟳ task {title}")));
        }
    }

    for job in state.jobs.values() {
        if matches!(job.status, crate::JobStatus::Running) {
            lines.push(Line::from(format!("◐ job {}", job.job_kind)));
        }
    }

    if let Some(ctx) = state.context.as_ref() {
        let status = match ctx.status {
            crate::ContextStatus::Selecting => "selecting",
            crate::ContextStatus::Compiled => "compiled",
        };
        lines.push(Line::from(format!("⚙ ctx {status}")));
    }

    if !state.artifacts.is_empty() {
        lines.push(Line::from(format!(
            "📄 artifacts {}",
            state.artifacts.len()
        )));
    }

    lines.truncate(max_lines.max(1));
    lines
}

fn build_chips_line(state: &TuiState, max_width: usize) -> String {
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

fn build_help_line(max_width: usize) -> String {
    truncate(
        "Enter send  Ctrl-B activity  Ctrl-R xray  PgUp/PgDn scroll",
        max_width,
    )
}

fn fmt_ms(value: Option<u64>) -> String {
    value
        .map(|ms| format!("{ms}ms"))
        .unwrap_or_else(|| "-".to_string())
}

fn canvas_scroll_offset(state: &TuiState, area: Rect, text: &str) -> (u16, u16) {
    let width = area.width.max(1) as usize;
    let height = area.height.max(1) as usize;
    let total_lines = wrapped_line_count(text, width);
    let max_scroll = total_lines.saturating_sub(height);
    let scroll = max_scroll.saturating_sub(state.canvas_scroll_from_bottom as usize);
    (scroll.min(u16::MAX as usize) as u16, 0)
}

fn wrapped_line_count(text: &str, width: usize) -> usize {
    let width = width.max(1);
    let mut total = 0usize;
    for line in text.split('\n') {
        let char_count = line.chars().count();
        total += if char_count == 0 {
            1
        } else {
            ((char_count - 1) / width) + 1
        };
    }
    total.max(1)
}

fn render_xray_screen(
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
    let (mut title, content_text) = match state.output_view {
        OutputViewMode::Rendered => ("Output".to_string(), state.output_text.clone()),
        OutputViewMode::Raw => ("Raw".to_string(), selected_event_json(state).to_string()),
    };

    if state.output_truncated && state.output_view == OutputViewMode::Rendered {
        title.push_str(" (truncated)");
    }

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

fn render_input(frame: &mut Frame<'_>, theme: &ThemeStyles, area: Rect, input: &str) {
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

fn render_overlay(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, mode: RenderMode) {
    let body = overlay_body_area(frame.area(), state.output_view);
    match &state.overlay {
        Overlay::None => {}
        Overlay::Activity => render_activity_overlay(frame, state, theme, body),
        Overlay::TaskList => render_task_list_overlay(frame, state, theme, body),
        Overlay::ToolDetail { tool_id } => {
            render_tool_detail_overlay(frame, state, theme, overlay_modal_area(body), tool_id, mode)
        }
        Overlay::TaskDetail { task_id } => {
            render_task_detail_overlay(frame, state, theme, overlay_modal_area(body), task_id)
        }
        Overlay::ErrorDetail { seq } => {
            render_error_overlay(frame, state, theme, overlay_modal_area(body), *seq)
        }
        Overlay::StallDetail => render_stall_overlay(frame, state, theme, overlay_modal_area(body)),
    }
}

fn overlay_body_area(area: Rect, view: OutputViewMode) -> Rect {
    // Keep overlays out of the status + input bars so the UI doesn't become a border salad.
    // Canvas layout: status=3, input=3.
    // X-ray layout: status=3, input=3 (output sits in body for now).
    let top = 3;
    let bottom = 3;
    let y = area.y.saturating_add(top);
    let height = area.height.saturating_sub(top + bottom).max(1);

    // In X-ray, we allow overlays to cover most of the viewport, but still keep the input visible.
    let _ = view;
    Rect {
        x: area.x,
        y,
        width: area.width.max(1),
        height,
    }
}

fn overlay_modal_area(body: Rect) -> Rect {
    let margin_x = (body.width / 10).max(2);
    let margin_y = (body.height / 10).max(1);
    Rect {
        x: body.x.saturating_add(margin_x),
        y: body.y.saturating_add(margin_y),
        width: body.width.saturating_sub(margin_x.saturating_mul(2)).max(1),
        height: body
            .height
            .saturating_sub(margin_y.saturating_mul(2))
            .max(1),
    }
}

fn render_activity_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Activity")
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(
        "tools / tasks / jobs / context / artifacts / errors",
    ));
    lines.push(Line::from(" "));

    let mut remaining = inner.height.saturating_sub(2) as usize;
    if state.openresponses_request_started_ms.is_some() {
        let headers = state
            .openresponses_headers_ms()
            .map(|ms| format!("{ms}ms"))
            .unwrap_or("-".to_string());
        let first_byte = state
            .openresponses_first_byte_ms()
            .map(|ms| format!("{ms}ms"))
            .unwrap_or("-".to_string());
        let first_event = state
            .openresponses_first_provider_event_ms()
            .map(|ms| format!("{ms}ms"))
            .unwrap_or("-".to_string());
        lines.push(Line::from(format!(
            "openresponses: headers={headers} first_byte={first_byte} first_event={first_event}"
        )));
        lines.push(Line::from(" "));
        remaining = remaining.saturating_sub(2);
    }

    lines.extend(build_activity_lines(state, remaining));
    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(widget, inner);
}

fn render_tool_detail_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
    tool_id: &str,
    mode: RenderMode,
) {
    frame.render_widget(Clear, area);
    let title = format!("Tool Detail: {tool_id}");
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(tool) = state.tools.get(tool_id) else {
        let widget = Paragraph::new(Text::from("<unknown tool>")).style(theme.chrome);
        frame.render_widget(widget, inner);
        return;
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    let status = match &tool.status {
        crate::ToolStatus::Running => "running".to_string(),
        crate::ToolStatus::Ended {
            exit_code,
            duration_ms,
        } => format!("ended exit={exit_code} ({duration_ms}ms)"),
        crate::ToolStatus::Failed { error } => format!("failed: {}", truncate(error, 64)),
    };
    lines.push(Line::from(format!("tool: {}", tool.name)));
    lines.push(Line::from(format!("status: {status}")));
    lines.push(Line::from(" "));

    lines.push(Line::from("args:"));
    match serde_json::to_string_pretty(&tool.args) {
        Ok(json) => {
            for line in json.lines().take(10) {
                lines.push(Line::from(line.to_string()));
            }
        }
        Err(_) => lines.push(Line::from("<failed to render args>")),
    }

    lines.push(Line::from(" "));
    if !tool.stdout_preview.is_empty() {
        lines.push(Line::from("stdout (preview):"));
        for line in tool.stdout_preview.lines().take(6) {
            lines.push(Line::from(line.to_string()));
        }
    }
    if !tool.stderr_preview.is_empty() {
        lines.push(Line::from("stderr (preview):"));
        for line in tool.stderr_preview.lines().take(6) {
            lines.push(Line::from(line.to_string()));
        }
    }

    if !tool.artifact_ids.is_empty() {
        lines.push(Line::from(" "));
        lines.push(Line::from(format!(
            "artifacts: {}",
            tool.artifact_ids.len()
        )));
    }

    lines.push(Line::from(" "));
    lines.push(Line::from(format!(
        "inspector_mode: {}",
        match mode {
            RenderMode::Json => "json",
            RenderMode::Decoded => "decoded",
        }
    )));

    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(widget, inner);
}

fn render_task_list_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Tasks")
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from("running / completed / failed"));
    lines.push(Line::from(" "));

    let mut tasks: Vec<&crate::TaskSummary> = state.tasks.values().collect();
    tasks.sort_by_key(|t| {
        (
            matches!(
                t.status,
                rip_kernel::ToolTaskStatus::Exited
                    | rip_kernel::ToolTaskStatus::Cancelled
                    | rip_kernel::ToolTaskStatus::Failed
            ),
            t.task_id.as_str(),
        )
    });

    for task in tasks
        .into_iter()
        .take(inner.height.saturating_sub(2) as usize)
    {
        let icon = match task.status {
            rip_kernel::ToolTaskStatus::Queued => "◯",
            rip_kernel::ToolTaskStatus::Running => "⟳",
            rip_kernel::ToolTaskStatus::Exited => "✓",
            rip_kernel::ToolTaskStatus::Cancelled => "⊘",
            rip_kernel::ToolTaskStatus::Failed => "✗",
        };
        let title = task
            .title
            .as_deref()
            .filter(|t| !t.is_empty())
            .unwrap_or(task.tool_name.as_str());
        lines.push(Line::from(format!("{icon} {title}  ({:?})", task.status)));
    }

    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(widget, inner);
}

fn render_task_detail_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
    task_id: &str,
) {
    frame.render_widget(Clear, area);
    let title = format!("Task Detail: {task_id}");
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(task) = state.tasks.get(task_id) else {
        frame.render_widget(
            Paragraph::new(Text::from("<unknown task>")).style(theme.chrome),
            inner,
        );
        return;
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(format!("tool: {}", task.tool_name)));
    if let Some(title) = task.title.as_deref().filter(|t| !t.is_empty()) {
        lines.push(Line::from(format!("title: {title}")));
    }
    lines.push(Line::from(format!("status: {:?}", task.status)));
    if let Some(code) = task.exit_code {
        lines.push(Line::from(format!("exit: {code}")));
    }
    if let Some(err) = task.error.as_deref() {
        lines.push(Line::from(format!("error: {}", truncate(err, 80))));
    }
    lines.push(Line::from(" "));
    if !task.stdout_preview.is_empty() {
        lines.push(Line::from("stdout (preview):"));
        for line in task.stdout_preview.lines().take(6) {
            lines.push(Line::from(line.to_string()));
        }
    }
    if !task.stderr_preview.is_empty() {
        lines.push(Line::from("stderr (preview):"));
        for line in task.stderr_preview.lines().take(6) {
            lines.push(Line::from(line.to_string()));
        }
    }
    if !task.pty_preview.is_empty() {
        lines.push(Line::from("pty (preview):"));
        for line in task.pty_preview.lines().take(6) {
            lines.push(Line::from(line.to_string()));
        }
    }
    if !task.artifact_ids.is_empty() {
        lines.push(Line::from(" "));
        lines.push(Line::from(format!(
            "artifacts: {}",
            task.artifact_ids.len()
        )));
    }

    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(widget, inner);
}

fn render_error_overlay(
    frame: &mut Frame<'_>,
    state: &TuiState,
    theme: &ThemeStyles,
    area: Rect,
    seq: u64,
) {
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Error Detail")
        .style(theme.chrome);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(event) = state.frames.get_by_seq(seq) else {
        frame.render_widget(
            Paragraph::new(Text::from("<missing error frame>")).style(theme.chrome),
            inner,
        );
        return;
    };

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(format!("seq: {}", event.seq)));
    lines.push(Line::from(format!("type: {}", event_type(event))));
    lines.push(Line::from(format!("summary: {}", event_summary(event))));

    match &event.kind {
        rip_kernel::EventKind::ToolFailed { error, .. } => {
            lines.push(Line::from(" "));
            lines.push(Line::from(format!("error: {}", truncate(error, 200))));
        }
        rip_kernel::EventKind::ProviderEvent {
            status,
            errors,
            response_errors,
            raw,
            ..
        } => {
            lines.push(Line::from(" "));
            lines.push(Line::from(format!("provider_status: {status:?}")));
            if !errors.is_empty() {
                lines.push(Line::from(format!("errors: {}", errors.len())));
                for e in errors.iter().take(4) {
                    lines.push(Line::from(format!("- {}", truncate(e, 120))));
                }
            }
            if !response_errors.is_empty() {
                lines.push(Line::from(format!(
                    "response_errors: {}",
                    response_errors.len()
                )));
                for e in response_errors.iter().take(4) {
                    lines.push(Line::from(format!("- {}", truncate(e, 120))));
                }
            }
            if let Some(raw) = raw.as_deref() {
                lines.push(Line::from("raw (preview):"));
                for line in raw.lines().take(6) {
                    lines.push(Line::from(truncate(line, 120)));
                }
            }
        }
        _ => {}
    }

    let widget = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .style(theme.chrome);
    frame.render_widget(widget, inner);
}

fn render_stall_overlay(frame: &mut Frame<'_>, state: &TuiState, theme: &ThemeStyles, area: Rect) {
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
            .wrap(Wrap { trim: false })
            .style(theme.chrome),
        inner,
    );
}

fn truncate(input: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    if input.chars().count() <= max_len {
        return input.to_string();
    }
    input
        .chars()
        .take(max_len.saturating_sub(1))
        .collect::<String>()
        + "…"
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use rip_kernel::{
        Event, EventKind, ProviderEventStatus, ToolTaskExecutionMode, ToolTaskStatus,
        ToolTaskStream,
    };
    use serde_json::json;

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

    fn seed_overlay_state() -> TuiState {
        let mut state = TuiState::new(100, 1024);
        state.update(event(
            0,
            EventKind::SessionStarted {
                input: "hello".to_string(),
            },
        ));
        state.update(event(
            1,
            EventKind::OpenResponsesRequestStarted {
                endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
                model: Some("gpt-5".to_string()),
                request_index: 0,
                kind: "response.create".to_string(),
            },
        ));
        state.update(event(
            2,
            EventKind::OpenResponsesResponseHeaders {
                request_index: 0,
                status: 200,
                request_id: Some("req_123".to_string()),
                content_type: Some("text/event-stream".to_string()),
            },
        ));
        state.update(event(
            3,
            EventKind::OpenResponsesResponseFirstByte { request_index: 0 },
        ));
        state.update(event(
            4,
            EventKind::ProviderEvent {
                provider: "openresponses".to_string(),
                status: ProviderEventStatus::InvalidJson,
                event_name: None,
                data: None,
                raw: Some("{".to_string()),
                errors: vec!["bad json".to_string()],
                response_errors: vec!["schema".to_string()],
            },
        ));
        state.update(event(
            5,
            EventKind::OutputTextDelta {
                delta: "output".to_string(),
            },
        ));
        state.update(event(
            6,
            EventKind::ToolStarted {
                tool_id: "tool-1".to_string(),
                name: "write".to_string(),
                args: json!({"path": "notes.md"}),
                timeout_ms: None,
            },
        ));
        state.update(event(
            7,
            EventKind::ToolStdout {
                tool_id: "tool-1".to_string(),
                chunk: "stdout line".to_string(),
            },
        ));
        state.update(event(
            8,
            EventKind::ToolStderr {
                tool_id: "tool-1".to_string(),
                chunk: "stderr line".to_string(),
            },
        ));
        state.update(event(
            9,
            EventKind::ToolTaskSpawned {
                task_id: "task-1".to_string(),
                tool_name: "shell".to_string(),
                args: json!({"cmd": "pwd"}),
                cwd: Some("/tmp".to_string()),
                title: Some("pwd".to_string()),
                execution_mode: ToolTaskExecutionMode::Pty,
                origin_session_id: None,
                artifacts: Some(json!({"artifact": "a".repeat(64)})),
            },
        ));
        state.update(event(
            10,
            EventKind::ToolTaskOutputDelta {
                task_id: "task-1".to_string(),
                stream: ToolTaskStream::Pty,
                chunk: "pty line".to_string(),
                artifacts: None,
            },
        ));
        state.update(event(
            11,
            EventKind::ToolTaskStatus {
                task_id: "task-1".to_string(),
                status: ToolTaskStatus::Running,
                exit_code: None,
                started_at_ms: Some(9),
                ended_at_ms: None,
                artifacts: None,
                error: None,
            },
        ));
        state.update(event(
            12,
            EventKind::ContinuityJobSpawned {
                job_id: "job-1".to_string(),
                job_kind: "compaction".to_string(),
                details: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ));
        state.update(event(
            13,
            EventKind::ContinuityContextCompiled {
                run_session_id: "run-1".to_string(),
                bundle_artifact_id: "b".repeat(64),
                compiler_id: "rip.context_compiler.v1".to_string(),
                compiler_strategy: "recent_messages_v1".to_string(),
                from_seq: 1,
                from_message_id: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ));
        state.activity_pinned = true;
        state.set_status_message("watching");
        state.set_now_ms(10_000);
        state
    }

    #[test]
    fn helper_builders_reflect_errors_stalls_and_running_work() {
        let mut state = seed_overlay_state();
        state.last_event_ms = Some(0);

        let activity = build_activity_lines(&state, 10)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(activity.iter().any(|line| line.contains("error")));
        assert!(activity.iter().any(|line| line.contains("stalled")));
        assert!(activity.iter().any(|line| line.contains("tool write")));
        assert!(activity.iter().any(|line| line.contains("task pwd")));
        assert!(activity.iter().any(|line| line.contains("job compaction")));
        assert!(activity.iter().any(|line| line.contains("ctx compiled")));
        assert!(activity.iter().any(|line| line.contains("artifacts")));

        let chips = build_chips_line(&state, 120);
        assert!(chips.contains("[⟳ write]"));
        assert!(chips.contains("[tasks:1/1]"));
        assert!(chips.contains("[jobs:1/1]"));
        assert!(chips.contains("[⚙ ctx:compiled]"));
        assert!(chips.contains("[⚠ error]"));

        let truncated = build_chips_line(&state, 12);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn wrapped_line_count_and_help_line_have_small_screen_fallbacks() {
        assert_eq!(wrapped_line_count("", 10), 1);
        assert_eq!(wrapped_line_count("hello", 10), 1);
        assert_eq!(wrapped_line_count("hello world", 5), 3);
        assert_eq!(build_help_line(8), "Enter s…");
    }

    #[test]
    fn selected_event_renderers_and_overlay_geometry_have_fallbacks() {
        let state = TuiState::default();
        assert_eq!(
            selected_event_json(&state).to_string(),
            "<no frame selected>"
        );
        assert_eq!(selected_event_decoded(&state).to_string(), "");

        let body = overlay_body_area(
            Rect {
                x: 0,
                y: 0,
                width: 120,
                height: 40,
            },
            OutputViewMode::Raw,
        );
        assert_eq!(body.y, 3);
        assert_eq!(body.height, 34);

        let modal = overlay_modal_area(body);
        assert!(modal.width < body.width);
        assert!(modal.height < body.height);
    }

    #[test]
    fn render_helpers_cover_overlay_variants_and_decoded_views() {
        let mut state = seed_overlay_state();
        state.selected_seq = Some(0);
        let json = selected_event_json(&state).to_string();
        assert!(json.contains("\"type\": \"session_started\""));
        let decoded = selected_event_decoded(&state).to_string();
        assert!(decoded.contains("\"summary\": \"\\\"hello\\\"\""));

        render_once(&state, RenderMode::Json, 120);

        state.output_view = OutputViewMode::Raw;
        state.theme = ThemeId::DefaultLight;
        render_once(&state, RenderMode::Decoded, 120);

        for overlay in [
            Overlay::Activity,
            Overlay::TaskList,
            Overlay::ToolDetail {
                tool_id: "tool-1".to_string(),
            },
            Overlay::TaskDetail {
                task_id: "task-1".to_string(),
            },
            Overlay::ErrorDetail { seq: 4 },
            Overlay::StallDetail,
        ] {
            state.overlay = overlay;
            render_once(&state, RenderMode::Decoded, 120);
        }

        state.overlay = Overlay::ToolDetail {
            tool_id: "missing".to_string(),
        };
        render_once(&state, RenderMode::Json, 120);

        state.overlay = Overlay::TaskDetail {
            task_id: "missing".to_string(),
        };
        render_once(&state, RenderMode::Json, 120);

        state.overlay = Overlay::ErrorDetail { seq: 999 };
        render_once(&state, RenderMode::Json, 120);
    }
}
