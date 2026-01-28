use std::path::PathBuf;

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;
use rip_kernel::{Event, EventKind, ProviderEventStatus};
use rip_tui::{render, OutputViewMode, Overlay, RenderMode, ThemeId, TuiState};

fn event(seq: u64, timestamp_ms: u64, kind: EventKind) -> Event {
    Event {
        id: format!("e{seq}"),
        session_id: "s1".to_string(),
        timestamp_ms,
        seq,
        kind,
    }
}

fn basic_state() -> TuiState {
    let mut state = TuiState::new(10_000, 1_000_000);
    state.update(event(
        0,
        1000,
        EventKind::SessionStarted {
            input: "hi".to_string(),
        },
    ));
    state.update(event(
        1,
        1200,
        EventKind::OutputTextDelta {
            delta: "hello".to_string(),
        },
    ));
    state.update(event(
        2,
        1300,
        EventKind::ToolStarted {
            tool_id: "t1".to_string(),
            name: "bash".to_string(),
            args: serde_json::json!({"command":"echo ok"}),
            timeout_ms: None,
        },
    ));
    state.update(event(
        3,
        1350,
        EventKind::ToolStdout {
            tool_id: "t1".to_string(),
            chunk: "ok\n".to_string(),
        },
    ));
    state.update(event(
        4,
        1400,
        EventKind::ToolEnded {
            tool_id: "t1".to_string(),
            exit_code: 0,
            duration_ms: 50,
            artifacts: None,
        },
    ));
    state.update(event(
        5,
        1450,
        EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Done,
            event_name: None,
            data: None,
            raw: None,
            errors: Vec::new(),
            response_errors: Vec::new(),
        },
    ));
    state.update(event(
        6,
        1500,
        EventKind::SessionEnded {
            reason: "completed".to_string(),
        },
    ));
    state
}

const ART1: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn follow_run_state_mid_tool() -> TuiState {
    let mut state = TuiState::new(10_000, 1_000_000);
    state.update(event(
        0,
        1000,
        EventKind::SessionStarted {
            input: "Add a slide outline for a product launch.".to_string(),
        },
    ));
    state.update(event(
        1,
        1200,
        EventKind::OutputTextDelta {
            delta: "Got it. I'll draft a 5-slide outline, then refine it.\n".to_string(),
        },
    ));
    state.update(event(
        2,
        1300,
        EventKind::ToolStarted {
            tool_id: "t1".to_string(),
            name: "bash".to_string(),
            args: serde_json::json!({"command":"ls"}),
            timeout_ms: None,
        },
    ));
    state.update(event(
        3,
        1350,
        EventKind::ToolStdout {
            tool_id: "t1".to_string(),
            chunk: "README.md\nslides.md\n".to_string(),
        },
    ));
    state
}

fn follow_run_state_tool_detail() -> TuiState {
    let mut state = follow_run_state_mid_tool();
    state.update(event(
        4,
        1400,
        EventKind::ToolEnded {
            tool_id: "t1".to_string(),
            exit_code: 0,
            duration_ms: 100,
            artifacts: Some(serde_json::json!({"stdout_artifact_id": ART1})),
        },
    ));
    state.overlay = Overlay::ToolDetail {
        tool_id: "t1".to_string(),
    };
    state
}

fn background_tasks_state() -> TuiState {
    let mut state = TuiState::new(10_000, 1_000_000);
    state.update(event(
        0,
        1000,
        EventKind::SessionStarted {
            input: "Run tests in the background.".to_string(),
        },
    ));

    // Running task
    state.update(event(
        1,
        1100,
        EventKind::ToolTaskSpawned {
            task_id: "tsk_a".to_string(),
            tool_name: "bash".to_string(),
            args: serde_json::json!({"command":"npm test"}),
            cwd: Some("/repo".to_string()),
            title: Some("tests".to_string()),
            execution_mode: rip_kernel::ToolTaskExecutionMode::Pipes,
            origin_session_id: Some("s1".to_string()),
            artifacts: None,
        },
    ));
    state.update(event(
        2,
        1150,
        EventKind::ToolTaskStatus {
            task_id: "tsk_a".to_string(),
            status: rip_kernel::ToolTaskStatus::Running,
            exit_code: None,
            started_at_ms: Some(1150),
            ended_at_ms: None,
            artifacts: None,
            error: None,
        },
    ));
    state.update(event(
        3,
        1200,
        EventKind::ToolTaskOutputDelta {
            task_id: "tsk_a".to_string(),
            stream: rip_kernel::ToolTaskStream::Stdout,
            chunk: "PASS src/app.test.ts\n".to_string(),
            artifacts: None,
        },
    ));

    // Failed task
    state.update(event(
        4,
        1300,
        EventKind::ToolTaskSpawned {
            task_id: "tsk_b".to_string(),
            tool_name: "bash".to_string(),
            args: serde_json::json!({"command":"cargo test"}),
            cwd: Some("/repo".to_string()),
            title: Some("rust tests".to_string()),
            execution_mode: rip_kernel::ToolTaskExecutionMode::Pipes,
            origin_session_id: Some("s1".to_string()),
            artifacts: None,
        },
    ));
    state.update(event(
        5,
        1350,
        EventKind::ToolTaskStatus {
            task_id: "tsk_b".to_string(),
            status: rip_kernel::ToolTaskStatus::Failed,
            exit_code: Some(101),
            started_at_ms: Some(1310),
            ended_at_ms: Some(1350),
            artifacts: Some(serde_json::json!({"log_artifact_id": ART1})),
            error: Some("exit status 101".to_string()),
        },
    ));
    state.update(event(
        6,
        1360,
        EventKind::ToolTaskOutputDelta {
            task_id: "tsk_b".to_string(),
            stream: rip_kernel::ToolTaskStream::Stderr,
            chunk: "error: test failed\n".to_string(),
            artifacts: None,
        },
    ));

    // Completed task
    state.update(event(
        7,
        1400,
        EventKind::ToolTaskSpawned {
            task_id: "tsk_c".to_string(),
            tool_name: "bash".to_string(),
            args: serde_json::json!({"command":"npm run lint"}),
            cwd: Some("/repo".to_string()),
            title: Some("lint".to_string()),
            execution_mode: rip_kernel::ToolTaskExecutionMode::Pipes,
            origin_session_id: Some("s1".to_string()),
            artifacts: None,
        },
    ));
    state.update(event(
        8,
        1450,
        EventKind::ToolTaskStatus {
            task_id: "tsk_c".to_string(),
            status: rip_kernel::ToolTaskStatus::Exited,
            exit_code: Some(0),
            started_at_ms: Some(1410),
            ended_at_ms: Some(1450),
            artifacts: None,
            error: None,
        },
    ));

    state
}

fn recover_error_provider_state() -> TuiState {
    let mut state = TuiState::new(10_000, 1_000_000);
    state.update(event(
        0,
        1000,
        EventKind::SessionStarted {
            input: "Continue.".to_string(),
        },
    ));
    state.update(event(
        1,
        1100,
        EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::InvalidJson,
            event_name: None,
            data: None,
            raw: Some("data: {not json}".to_string()),
            errors: vec!["invalid json".to_string()],
            response_errors: vec![],
        },
    ));
    state.overlay = Overlay::ErrorDetail { seq: 1 };
    state
}

fn recover_error_tool_failed_state() -> TuiState {
    let mut state = recover_error_provider_state();
    state.update(event(
        2,
        1200,
        EventKind::ToolFailed {
            tool_id: "t9".to_string(),
            error: "permission denied".to_string(),
        },
    ));
    state.overlay = Overlay::ErrorDetail { seq: 2 };
    state
}

fn recover_stalled_state() -> TuiState {
    let mut state = recover_error_provider_state();
    state.overlay = Overlay::StallDetail;
    state.set_now_ms(20_000);
    state
}

fn render_to_string(width: u16, height: u16, state: &TuiState, mode: RenderMode) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal.draw(|f| render(f, state, mode, "")).expect("draw");
    buffer_to_string(terminal.backend().buffer())
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

fn snapshot_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
        .join(name)
}

fn assert_snapshot(name: &str, rendered: String) {
    let path = snapshot_path(name);
    if std::env::var("RIPTUI_UPDATE_SNAPSHOTS").is_ok() {
        std::fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        std::fs::write(&path, rendered).expect("write snapshot");
        return;
    }

    let expected = std::fs::read_to_string(&path).expect("snapshot missing");
    assert_eq!(expected, rendered);
}

#[test]
fn golden_basic_80x24() {
    let state = basic_state();
    let rendered = render_to_string(80, 24, &state, RenderMode::Json);
    assert_snapshot("basic_80x24.txt", rendered);
}

#[test]
fn golden_basic_60x20() {
    let state = basic_state();
    let rendered = render_to_string(60, 20, &state, RenderMode::Json);
    assert_snapshot("basic_60x20.txt", rendered);
}

#[test]
fn golden_raw_80x24() {
    let mut state = basic_state();
    state.output_view = OutputViewMode::Raw;
    let rendered = render_to_string(80, 24, &state, RenderMode::Json);
    assert_snapshot("raw_80x24.txt", rendered);
}

#[test]
fn golden_theme_light_80x24() {
    let mut state = basic_state();
    state.theme = ThemeId::DefaultLight;
    let rendered = render_to_string(80, 24, &state, RenderMode::Json);
    assert_snapshot("theme_light_80x24.txt", rendered);
}

#[test]
fn golden_clipboard_fallback_80x24() {
    let mut state = basic_state();
    state.status_message = Some("clipboard: stored (OSC52 disabled)".to_string());
    let rendered = render_to_string(80, 24, &state, RenderMode::Json);
    assert_snapshot("clipboard_fallback_80x24.txt", rendered);
}

#[test]
fn journey_follow_a_run_xs_60x20() {
    let state = follow_run_state_mid_tool();
    let rendered = render_to_string(60, 20, &state, RenderMode::Json);
    assert_snapshot("journey_follow_a_run_xs_60x20.txt", rendered);
}

#[test]
fn journey_follow_a_run_s_80x24_activity() {
    let mut state = follow_run_state_mid_tool();
    state.overlay = Overlay::Activity;
    let rendered = render_to_string(80, 24, &state, RenderMode::Json);
    assert_snapshot("journey_follow_a_run_s_80x24_activity.txt", rendered);
}

#[test]
fn journey_follow_a_run_m_120x40_tool_detail() {
    let mut state = follow_run_state_tool_detail();
    state.activity_pinned = true;
    let rendered = render_to_string(120, 40, &state, RenderMode::Decoded);
    assert_snapshot("journey_follow_a_run_m_120x40_tool_detail.txt", rendered);
}

#[test]
fn journey_background_tasks_xs_60x20_tasks() {
    let mut state = background_tasks_state();
    state.overlay = Overlay::TaskList;
    let rendered = render_to_string(60, 20, &state, RenderMode::Json);
    assert_snapshot("journey_background_tasks_xs_60x20_tasks.txt", rendered);
}

#[test]
fn journey_background_tasks_s_80x24_task_detail() {
    let mut state = background_tasks_state();
    state.overlay = Overlay::TaskDetail {
        task_id: "tsk_a".to_string(),
    };
    let rendered = render_to_string(80, 24, &state, RenderMode::Json);
    assert_snapshot("journey_background_tasks_s_80x24_task_detail.txt", rendered);
}

#[test]
fn journey_background_tasks_m_120x40_task_detail() {
    let mut state = background_tasks_state();
    state.activity_pinned = true;
    state.overlay = Overlay::TaskDetail {
        task_id: "tsk_b".to_string(),
    };
    let rendered = render_to_string(120, 40, &state, RenderMode::Json);
    assert_snapshot(
        "journey_background_tasks_m_120x40_task_detail.txt",
        rendered,
    );
}

#[test]
fn journey_recover_error_xs_60x20_provider() {
    let state = recover_error_provider_state();
    let rendered = render_to_string(60, 20, &state, RenderMode::Json);
    assert_snapshot("journey_recover_error_xs_60x20_provider.txt", rendered);
}

#[test]
fn journey_recover_error_s_80x24_tool_failed() {
    let state = recover_error_tool_failed_state();
    let rendered = render_to_string(80, 24, &state, RenderMode::Json);
    assert_snapshot("journey_recover_error_s_80x24_tool_failed.txt", rendered);
}

#[test]
fn journey_recover_error_m_120x40_stalled() {
    let state = recover_stalled_state();
    let rendered = render_to_string(120, 40, &state, RenderMode::Json);
    assert_snapshot("journey_recover_error_m_120x40_stalled.txt", rendered);
}
