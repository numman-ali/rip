use httpmock::Method::{GET, POST};
use httpmock::MockServer;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;

use super::*;

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
