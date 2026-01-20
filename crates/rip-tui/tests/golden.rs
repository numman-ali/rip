use std::path::PathBuf;

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::Terminal;
use rip_kernel::{Event, EventKind, ProviderEventStatus};
use rip_tui::{render, OutputViewMode, RenderMode, ThemeId, TuiState};

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
