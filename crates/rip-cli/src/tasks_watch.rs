use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Context;
use crossterm::event::{Event as TermEvent, EventStream, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table, TableState, Wrap};
use ratatui::Frame;
use ratatui::Terminal;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::{interval, Instant};

const OUTPUT_CHUNK_BYTES: usize = 4096;
const OUTPUT_BUFFER_MAX_BYTES: usize = 64 * 1024;
const OUTPUT_POLL_MS: u64 = 250;

#[derive(Debug, Clone, Deserialize, Serialize)]
struct TaskStatus {
    task_id: String,
    status: String,
    tool: String,
    title: Option<String>,
    execution_mode: String,
    exit_code: Option<i32>,
    started_at_ms: Option<u64>,
    ended_at_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct TaskOutputResponse {
    content: String,
    offset_bytes: u64,
    bytes: usize,
    total_bytes: u64,
    truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskStream {
    Stdout,
    Stderr,
    Pty,
}

impl TaskStream {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
            Self::Pty => "pty",
        }
    }
}

struct TaskWatchState {
    tasks: Vec<TaskStatus>,
    selected_task_id: Option<String>,
    output: String,
    output_offset: u64,
    output_total: u64,
    output_truncated: bool,
    output_stream: TaskStream,
    status_message: Option<String>,
}

impl Default for TaskWatchState {
    fn default() -> Self {
        Self {
            tasks: Vec::new(),
            selected_task_id: None,
            output: String::new(),
            output_offset: 0,
            output_total: 0,
            output_truncated: false,
            output_stream: TaskStream::Stdout,
            status_message: None,
        }
    }
}

impl TaskWatchState {
    fn update_tasks(&mut self, mut tasks: Vec<TaskStatus>) -> bool {
        tasks.sort_by(|a, b| {
            let rank_a = status_rank(&a.status);
            let rank_b = status_rank(&b.status);
            rank_a
                .cmp(&rank_b)
                .then_with(|| {
                    a.started_at_ms
                        .unwrap_or(0)
                        .cmp(&b.started_at_ms.unwrap_or(0))
                })
                .then_with(|| a.task_id.cmp(&b.task_id))
        });

        let previous_selection = self.selected_task_id.clone();
        self.tasks = tasks;

        let selection = match previous_selection.as_ref() {
            Some(id) if self.tasks.iter().any(|task| task.task_id == *id) => Some(id.clone()),
            _ => self.tasks.first().map(|task| task.task_id.clone()),
        };

        if selection != previous_selection {
            self.selected_task_id = selection;
            self.reset_output_for_selection();
        } else if selection.is_none() {
            self.selected_task_id = None;
            self.output_stream = TaskStream::Stdout;
            self.reset_output();
        }

        true
    }

    fn move_selection(&mut self, delta: i64) -> bool {
        if self.tasks.is_empty() {
            return false;
        }
        let current_idx = self
            .selected_task_id
            .as_ref()
            .and_then(|id| self.tasks.iter().position(|task| task.task_id == *id))
            .unwrap_or(0);
        let next_idx = if delta.is_negative() {
            current_idx.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            current_idx.saturating_add(delta as usize)
        };
        let clamped = next_idx.min(self.tasks.len().saturating_sub(1));
        let next_id = self.tasks.get(clamped).map(|task| task.task_id.clone());
        if next_id != self.selected_task_id {
            self.selected_task_id = next_id;
            self.reset_output_for_selection();
            return true;
        }
        false
    }

    fn selected_task(&self) -> Option<&TaskStatus> {
        let id = self.selected_task_id.as_ref()?;
        self.tasks.iter().find(|task| &task.task_id == id)
    }

    fn reset_output(&mut self) {
        self.output.clear();
        self.output_offset = 0;
        self.output_total = 0;
        self.output_truncated = false;
    }

    fn reset_output_for_selection(&mut self) {
        self.output_stream = match self.selected_task() {
            Some(task) if task.execution_mode.eq_ignore_ascii_case("pty") => TaskStream::Pty,
            _ => TaskStream::Stdout,
        };
        self.reset_output();
    }

    fn toggle_stream(&mut self) -> bool {
        let Some(task) = self.selected_task() else {
            return false;
        };
        if task.execution_mode.eq_ignore_ascii_case("pty") {
            return false;
        }
        self.output_stream = match self.output_stream {
            TaskStream::Stdout => TaskStream::Stderr,
            TaskStream::Stderr => TaskStream::Stdout,
            TaskStream::Pty => TaskStream::Stdout,
        };
        self.reset_output();
        true
    }

    fn append_output(&mut self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }
        self.output.push_str(chunk);
        if self.output.len() <= OUTPUT_BUFFER_MAX_BYTES {
            return;
        }

        let keep = OUTPUT_BUFFER_MAX_BYTES / 2;
        let start = self.output.len().saturating_sub(keep);
        let safe_start = self
            .output
            .char_indices()
            .find(|(idx, _)| *idx >= start)
            .map(|(idx, _)| idx)
            .unwrap_or(0);
        self.output = self.output[safe_start..].to_string();
    }

    fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }
}

pub async fn run_tasks_watch(server: String, refresh_ms: u64) -> anyhow::Result<()> {
    let client = Client::new();

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let mut guard = TerminalGuard::active();
    terminal.clear()?;

    let mut state = TaskWatchState::default();
    let mut term_events = EventStream::new();
    let mut tick = interval(Duration::from_millis(50));
    let list_interval = Duration::from_millis(refresh_ms.max(100));
    let output_interval = Duration::from_millis(OUTPUT_POLL_MS);
    let mut last_list_refresh = Instant::now() - list_interval;
    let mut last_output_refresh = Instant::now() - output_interval;
    let mut dirty = true;

    loop {
        if dirty {
            terminal.draw(|f| render(f, &state))?;
            dirty = false;
        }

        tokio::select! {
            _ = tick.tick() => {
                let mut changed = false;
                if last_list_refresh.elapsed() >= list_interval {
                    match fetch_tasks(&client, &server).await {
                        Ok(tasks) => {
                            changed |= state.update_tasks(tasks);
                        }
                        Err(err) => {
                            state.set_status_message(format!("tasks: {err}"));
                            changed = true;
                        }
                    }
                    last_list_refresh = Instant::now();
                }

                if last_output_refresh.elapsed() >= output_interval {
                    if let Some(task) = state.selected_task() {
                        if task_output_ready(&task.status) {
                            match fetch_output(&client, &server, &task.task_id, state.output_stream, state.output_offset).await {
                                Ok(output) => {
                                    if output.bytes > 0 {
                                        state.append_output(&output.content);
                                        changed = true;
                                    }
                                    let next_offset = output.offset_bytes.saturating_add(output.bytes as u64);
                                    if next_offset != state.output_offset
                                        || output.total_bytes != state.output_total
                                        || output.truncated != state.output_truncated
                                    {
                                        state.output_offset = next_offset;
                                        state.output_total = output.total_bytes;
                                        state.output_truncated = output.truncated;
                                        changed = true;
                                    }
                                }
                                Err(err) => {
                                    state.set_status_message(format!("output: {err}"));
                                    changed = true;
                                }
                            }
                        }
                    }
                    last_output_refresh = Instant::now();
                }

                if changed {
                    dirty = true;
                }
            }
            maybe_event = term_events.next() => {
                let Some(Ok(event)) = maybe_event else {
                    continue;
                };
                if let TermEvent::Key(key) = event {
                    if let Some(action) = handle_key_event(key) {
                        match action {
                            UiAction::Quit => break,
                            UiAction::Move(delta) => {
                                if state.move_selection(delta) {
                                    dirty = true;
                                }
                            }
                            UiAction::Cancel => {
                                if let Some(task) = state.selected_task() {
                                    match cancel_task(&client, &server, &task.task_id).await {
                                        Ok(()) => state.set_status_message("cancel requested"),
                                        Err(err) => state.set_status_message(format!("cancel: {err}")),
                                    }
                                    dirty = true;
                                }
                            }
                            UiAction::ToggleStream => {
                                if state.toggle_stream() {
                                    dirty = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    guard.deactivate(&mut terminal)?;
    Ok(())
}

async fn fetch_tasks(client: &Client, server: &str) -> anyhow::Result<Vec<TaskStatus>> {
    let url = format!("{server}/tasks");
    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        anyhow::bail!("status {}", response.status());
    }
    let tasks = response.json::<Vec<TaskStatus>>().await?;
    Ok(tasks)
}

async fn fetch_output(
    client: &Client,
    server: &str,
    task_id: &str,
    stream: TaskStream,
    offset_bytes: u64,
) -> anyhow::Result<TaskOutputResponse> {
    let url = format!(
        "{server}/tasks/{task_id}/output?stream={}&offset_bytes={offset_bytes}&max_bytes={OUTPUT_CHUNK_BYTES}",
        stream.as_str()
    );
    let response = client.get(url).send().await?;
    if !response.status().is_success() {
        anyhow::bail!("status {}", response.status());
    }
    response
        .json::<TaskOutputResponse>()
        .await
        .context("decode output")
}

async fn cancel_task(client: &Client, server: &str, task_id: &str) -> anyhow::Result<()> {
    let url = format!("{server}/tasks/{task_id}/cancel");
    let response = client
        .post(url)
        .json(&serde_json::json!({"reason": "cancel"}))
        .send()
        .await?;
    if !response.status().is_success() {
        anyhow::bail!("status {}", response.status());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum UiAction {
    Quit,
    Move(i64),
    Cancel,
    ToggleStream,
}

fn handle_key_event(key: KeyEvent) -> Option<UiAction> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        return Some(UiAction::Quit);
    }

    match key.code {
        KeyCode::Esc => Some(UiAction::Quit),
        KeyCode::Char('q') => Some(UiAction::Quit),
        KeyCode::Up => Some(UiAction::Move(-1)),
        KeyCode::Down => Some(UiAction::Move(1)),
        KeyCode::Char('k') => Some(UiAction::Move(-1)),
        KeyCode::Char('j') => Some(UiAction::Move(1)),
        KeyCode::Char('c') => Some(UiAction::Cancel),
        KeyCode::Char('s') => Some(UiAction::ToggleStream),
        _ => None,
    }
}

fn render(frame: &mut Frame<'_>, state: &TaskWatchState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Min(6),
        ])
        .split(frame.area());

    render_status_bar(frame, state, chunks[0]);
    render_tasks_table(frame, state, chunks[1]);
    render_output(frame, state, chunks[2]);
}

fn render_status_bar(frame: &mut Frame<'_>, state: &TaskWatchState, area: Rect) {
    let selected = state
        .selected_task()
        .map(|task| short_id(&task.task_id))
        .unwrap_or_else(|| "-".to_string());
    let stream = state.output_stream.as_str();

    let mut line = String::new();
    if let Some(msg) = state.status_message.as_deref() {
        line.push_str("msg:");
        line.push_str(msg);
        line.push_str(" | ");
    }
    line.push_str(&format!(
        "tasks:{}  selected:{}  stream:{}  [q] quit  [c] cancel  [s] stream  [up/down] select",
        state.tasks.len(),
        selected,
        stream
    ));

    let widget = Paragraph::new(Line::from(line))
        .block(Block::default().borders(Borders::ALL).title("Tasks"));
    frame.render_widget(widget, area);
}

fn render_tasks_table(frame: &mut Frame<'_>, state: &TaskWatchState, area: Rect) {
    let now_ms = now_ms();
    let mut rows: Vec<Row<'static>> = Vec::new();
    for task in &state.tasks {
        let id = short_id(&task.task_id);
        let status = task.status.clone();
        let tool = format_tool(task);
        let mode = task.execution_mode.clone();
        let elapsed = format_elapsed(task, now_ms);
        let exit = task
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "-".to_string());
        rows.push(Row::new(vec![id, status, tool, mode, elapsed, exit]));
    }

    let header = Row::new(vec!["id", "status", "tool", "mode", "elapsed", "exit"])
        .style(Style::default().add_modifier(Modifier::BOLD));

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Min(16),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title("Task list"))
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol("> ");

    let mut table_state = TableState::default();
    if let Some(selected_id) = state.selected_task_id.as_ref() {
        if let Some(index) = state
            .tasks
            .iter()
            .position(|task| &task.task_id == selected_id)
        {
            table_state.select(Some(index));
        }
    }

    frame.render_stateful_widget(table, area, &mut table_state);
}

fn render_output(frame: &mut Frame<'_>, state: &TaskWatchState, area: Rect) {
    let title = match state.selected_task() {
        Some(task) => {
            let mut title = format!(
                "Output {} ({})",
                short_id(&task.task_id),
                state.output_stream.as_str()
            );
            if state.output_truncated {
                title.push_str(" truncated");
            }
            title
        }
        None => "Output".to_string(),
    };

    let content = if state.output.is_empty() {
        "<no output>"
    } else {
        state.output.as_str()
    };

    let widget = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn format_tool(task: &TaskStatus) -> String {
    match task.title.as_deref() {
        Some(title) if !title.trim().is_empty() => format!("{}: {}", task.tool, title.trim()),
        _ => task.tool.clone(),
    }
}

fn format_elapsed(task: &TaskStatus, now_ms: u64) -> String {
    let start = match task.started_at_ms {
        Some(value) => value,
        None => return "-".to_string(),
    };
    let end = task.ended_at_ms.unwrap_or(now_ms);
    if end < start {
        return "-".to_string();
    }
    format_duration(end - start)
}

fn format_duration(duration_ms: u64) -> String {
    let total_seconds = duration_ms / 1000;
    let seconds = total_seconds % 60;
    let minutes = (total_seconds / 60) % 60;
    let hours = total_seconds / 3600;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

fn short_id(task_id: &str) -> String {
    const SHORT_LEN: usize = 8;
    if task_id.len() <= SHORT_LEN {
        task_id.to_string()
    } else {
        task_id[..SHORT_LEN].to_string()
    }
}

fn status_rank(status: &str) -> u8 {
    match status.trim().to_ascii_lowercase().as_str() {
        "running" => 0,
        "queued" => 1,
        "exited" => 2,
        "cancelled" => 3,
        "failed" => 4,
        _ => 5,
    }
}

fn task_output_ready(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_lowercase().as_str(),
        "running" | "exited" | "cancelled" | "failed"
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn active() -> Self {
        Self { active: true }
    }

    fn deactivate(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> anyhow::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        disable_raw_mode()?;
        terminal.backend_mut().execute(LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = stdout.execute(LeaveAlternateScreen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    #[test]
    fn short_id_truncates() {
        assert_eq!(short_id("abcd"), "abcd");
        assert_eq!(short_id("abcdefghijk"), "abcdefgh");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(9_000), "0:09");
        assert_eq!(format_duration(61_000), "1:01");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(3_660_000), "1:01:00");
    }

    fn make_task(
        task_id: &str,
        status: &str,
        execution_mode: &str,
        started_at_ms: Option<u64>,
        ended_at_ms: Option<u64>,
        title: Option<&str>,
    ) -> TaskStatus {
        TaskStatus {
            task_id: task_id.to_string(),
            status: status.to_string(),
            tool: "bash".to_string(),
            title: title.map(|value| value.to_string()),
            execution_mode: execution_mode.to_string(),
            exit_code: None,
            started_at_ms,
            ended_at_ms,
        }
    }

    #[test]
    fn update_tasks_sorts_and_preserves_selection() {
        let mut state = TaskWatchState::default();
        let tasks = vec![
            make_task("b", "queued", "pipes", Some(2), None, None),
            make_task("a", "running", "pipes", Some(1), None, None),
        ];
        state.update_tasks(tasks);
        assert_eq!(state.tasks[0].task_id, "a");
        assert_eq!(state.selected_task_id.as_deref(), Some("a"));

        state.selected_task_id = Some("b".to_string());
        state.output = "keep".to_string();
        let tasks = vec![
            make_task("b", "queued", "pipes", Some(2), None, None),
            make_task("a", "running", "pipes", Some(1), None, None),
        ];
        state.update_tasks(tasks);
        assert_eq!(state.selected_task_id.as_deref(), Some("b"));
        assert_eq!(state.output, "keep");

        state.update_tasks(Vec::new());
        assert!(state.selected_task_id.is_none());
        assert_eq!(state.output_stream, TaskStream::Stdout);
        assert!(state.output.is_empty());
    }

    #[test]
    fn update_tasks_resets_output_and_sets_pty_stream() {
        let mut state = TaskWatchState {
            output: "data".to_string(),
            output_offset: 99,
            ..Default::default()
        };
        let tasks = vec![make_task("a", "running", "pty", Some(1), None, None)];
        state.update_tasks(tasks);
        assert_eq!(state.selected_task_id.as_deref(), Some("a"));
        assert_eq!(state.output_stream, TaskStream::Pty);
        assert!(state.output.is_empty());
        assert_eq!(state.output_offset, 0);
    }

    #[test]
    fn move_selection_clamps_and_resets_output() {
        let mut state = TaskWatchState {
            tasks: vec![
                make_task("a", "running", "pipes", Some(1), None, None),
                make_task("b", "queued", "pipes", Some(2), None, None),
            ],
            selected_task_id: Some("a".to_string()),
            output: "data".to_string(),
            output_offset: 10,
            ..Default::default()
        };

        assert!(state.move_selection(1));
        assert_eq!(state.selected_task_id.as_deref(), Some("b"));
        assert!(state.output.is_empty());
        assert_eq!(state.output_offset, 0);

        assert!(!state.move_selection(10));
        assert_eq!(state.selected_task_id.as_deref(), Some("b"));
    }

    #[test]
    fn toggle_stream_resets_and_skips_pty() {
        let mut state = TaskWatchState {
            tasks: vec![make_task("a", "running", "pipes", Some(1), None, None)],
            selected_task_id: Some("a".to_string()),
            output: "data".to_string(),
            output_offset: 10,
            ..Default::default()
        };

        assert!(state.toggle_stream());
        assert_eq!(state.output_stream, TaskStream::Stderr);
        assert!(state.output.is_empty());

        state.tasks = vec![make_task("a", "running", "pty", Some(1), None, None)];
        state.selected_task_id = Some("a".to_string());
        assert!(!state.toggle_stream());
        assert_eq!(state.output_stream, TaskStream::Stderr);
    }

    #[test]
    fn toggle_stream_without_selection_is_noop() {
        let mut state = TaskWatchState::default();
        assert!(!state.toggle_stream());
        assert_eq!(state.output_stream, TaskStream::Stdout);
    }

    #[test]
    fn move_selection_empty_returns_false() {
        let mut state = TaskWatchState::default();
        assert!(!state.move_selection(1));
    }

    #[test]
    fn append_output_truncates_buffer() {
        let mut state = TaskWatchState::default();
        let chunk = "a".repeat(OUTPUT_BUFFER_MAX_BYTES);
        state.append_output(&chunk);
        assert_eq!(state.output.len(), OUTPUT_BUFFER_MAX_BYTES);

        state.append_output(&"b".repeat(OUTPUT_BUFFER_MAX_BYTES));
        assert!(state.output.len() <= OUTPUT_BUFFER_MAX_BYTES);
        assert!(state.output.contains('b'));
    }

    #[test]
    fn append_output_ignores_empty_chunk() {
        let mut state = TaskWatchState::default();
        state.append_output("");
        assert!(state.output.is_empty());
    }

    #[test]
    fn handle_key_event_maps_actions() {
        assert!(matches!(
            handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(UiAction::Quit)
        ));
        assert!(matches!(
            handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::empty())),
            Some(UiAction::Quit)
        ));
        assert!(matches!(
            handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty())),
            Some(UiAction::Quit)
        ));
        assert!(matches!(
            handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::empty())),
            Some(UiAction::Move(-1))
        ));
        assert!(matches!(
            handle_key_event(KeyEvent::new(KeyCode::Down, KeyModifiers::empty())),
            Some(UiAction::Move(1))
        ));
        assert!(matches!(
            handle_key_event(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::empty())),
            Some(UiAction::Move(1))
        ));
        assert!(matches!(
            handle_key_event(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::empty())),
            Some(UiAction::Move(-1))
        ));
        assert!(matches!(
            handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::empty())),
            Some(UiAction::Cancel)
        ));
        assert!(matches!(
            handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::empty())),
            Some(UiAction::ToggleStream)
        ));
    }

    fn buffer_to_string(buffer: &Buffer) -> String {
        let mut out = String::new();
        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                let symbol = buffer.cell((x, y)).map(|cell| cell.symbol()).unwrap_or(" ");
                line.push_str(symbol);
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    #[test]
    fn render_populates_sections() {
        let mut state = TaskWatchState {
            tasks: vec![make_task(
                "task123456",
                "running",
                "pipes",
                Some(1_000),
                None,
                Some(" build "),
            )],
            selected_task_id: Some("task123456".to_string()),
            output: "hello".to_string(),
            output_truncated: true,
            ..Default::default()
        };
        state.set_status_message("ok");

        let mut terminal = Terminal::new(TestBackend::new(80, 20)).expect("terminal");
        terminal.draw(|f| render(f, &state)).expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());
        assert!(rendered.contains("Task list"));
        assert!(rendered.contains("Output"));
        assert!(rendered.contains("hello"));
        assert!(rendered.contains("task1234"));
    }

    #[test]
    fn render_empty_state_shows_no_output() {
        let state = TaskWatchState::default();
        let mut terminal = Terminal::new(TestBackend::new(60, 10)).expect("terminal");
        terminal.draw(|f| render(f, &state)).expect("draw");
        let rendered = buffer_to_string(terminal.backend().buffer());
        assert!(rendered.contains("<no output>"));
    }

    #[test]
    fn format_helpers_cover_branches() {
        let task_status = make_task("a", "running", "pipes", None, None, Some(" title "));
        assert_eq!(format_tool(&task_status), "bash: title");
        assert_eq!(format_elapsed(&task_status, 100), "-");

        let task_status = make_task("a", "running", "pipes", Some(200), Some(100), None);
        assert_eq!(format_elapsed(&task_status, 150), "-");

        let task_status = make_task("a", "running", "pipes", Some(0), Some(3_000), None);
        assert_eq!(format_elapsed(&task_status, 3_000), "0:03");

        assert_eq!(status_rank("running"), 0);
        assert_eq!(status_rank("unknown"), 5);
        assert!(task_output_ready("running"));
        assert!(!task_output_ready("queued"));

        let now = now_ms();
        assert!(now > 0);
    }

    #[test]
    fn format_tool_without_title_uses_tool_name() {
        let task_status = make_task("a", "running", "pipes", Some(0), None, None);
        assert_eq!(format_tool(&task_status), "bash");
    }

    #[test]
    fn selected_task_returns_none_when_missing() {
        let state = TaskWatchState {
            tasks: vec![make_task("a", "running", "pipes", None, None, None)],
            selected_task_id: Some("missing".to_string()),
            ..Default::default()
        };
        assert!(state.selected_task().is_none());
    }

    #[test]
    fn selected_task_returns_some_when_present() {
        let state = TaskWatchState {
            tasks: vec![make_task("a", "running", "pipes", None, None, None)],
            selected_task_id: Some("a".to_string()),
            ..Default::default()
        };
        assert!(state.selected_task().is_some());
    }

    #[test]
    fn reset_output_clears_fields() {
        let mut state = TaskWatchState {
            output: "data".to_string(),
            output_offset: 10,
            output_total: 20,
            output_truncated: true,
            ..Default::default()
        };
        state.reset_output();
        assert!(state.output.is_empty());
        assert_eq!(state.output_offset, 0);
        assert_eq!(state.output_total, 0);
        assert!(!state.output_truncated);
    }

    #[test]
    fn reset_output_for_selection_sets_stream() {
        let mut state = TaskWatchState {
            tasks: vec![make_task("a", "running", "pty", None, None, None)],
            selected_task_id: Some("a".to_string()),
            output_stream: TaskStream::Stdout,
            ..Default::default()
        };
        state.reset_output_for_selection();
        assert_eq!(state.output_stream, TaskStream::Pty);
    }

    #[test]
    fn stream_and_status_helpers_cover_variants() {
        assert_eq!(TaskStream::Stdout.as_str(), "stdout");
        assert_eq!(TaskStream::Stderr.as_str(), "stderr");
        assert_eq!(TaskStream::Pty.as_str(), "pty");
        assert_eq!(status_rank("queued"), 1);
        assert_eq!(status_rank("exited"), 2);
        assert_eq!(status_rank("cancelled"), 3);
        assert_eq!(status_rank("failed"), 4);
        assert!(task_output_ready("failed"));
        assert!(task_output_ready("cancelled"));
    }

    #[tokio::test]
    async fn fetch_tasks_output_and_cancel() {
        let server = MockServer::start();
        let tasks_body = serde_json::to_string(&vec![make_task(
            "task1",
            "running",
            "pipes",
            Some(1),
            None,
            None,
        )])
        .expect("json");
        let _tasks = server.mock(|when, then| {
            when.method(GET).path("/tasks");
            then.status(200)
                .header("content-type", "application/json")
                .body(tasks_body);
        });

        let output_body = serde_json::to_string(&TaskOutputResponse {
            content: "out".to_string(),
            offset_bytes: 0,
            bytes: 3,
            total_bytes: 3,
            truncated: false,
        })
        .expect("json");
        let _output = server.mock(|when, then| {
            when.method(GET)
                .path("/tasks/task1/output")
                .query_param("stream", "stdout");
            then.status(200)
                .header("content-type", "application/json")
                .body(output_body);
        });

        let _cancel = server.mock(|when, then| {
            when.method(POST).path("/tasks/task1/cancel");
            then.status(200);
        });

        let client = Client::new();
        let base = server.base_url();

        let tasks = fetch_tasks(&client, &base).await.expect("tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].task_id, "task1");

        let output = fetch_output(&client, &base, "task1", TaskStream::Stdout, 0)
            .await
            .expect("output");
        assert_eq!(output.content, "out");

        cancel_task(&client, &base, "task1").await.expect("cancel");
    }

    #[tokio::test]
    async fn fetch_errors_return_err() {
        let server = MockServer::start();
        let _tasks = server.mock(|when, then| {
            when.method(GET).path("/tasks");
            then.status(500);
        });
        let _output = server.mock(|when, then| {
            when.method(GET).path("/tasks/task1/output");
            then.status(500);
        });
        let _cancel = server.mock(|when, then| {
            when.method(POST).path("/tasks/task1/cancel");
            then.status(500);
        });

        let client = Client::new();
        let base = server.base_url();

        assert!(fetch_tasks(&client, &base).await.is_err());
        assert!(fetch_output(&client, &base, "task1", TaskStream::Stdout, 0)
            .await
            .is_err());
        assert!(cancel_task(&client, &base, "task1").await.is_err());
    }
}
