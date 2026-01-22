use std::future;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{Event as TermEvent, EventStream, KeyCode, KeyEvent};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, ExecutableCommand};
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use reqwest::Client;
use reqwest_eventsource::{
    Error as EventSourceError, Event as SseEvent, EventSource, RequestBuilderExt,
};
use rip_kernel::Event as FrameEvent;
use rip_tui::{render, RenderMode, TuiState};
use tokio::sync::broadcast;

mod keymap;

use keymap::{Command as KeyCommand, Keymap};

pub async fn run_fullscreen_tui(initial_prompt: Option<String>) -> anyhow::Result<()> {
    let engine =
        ripd::SessionEngine::new_default().map_err(|err| anyhow::anyhow!("engine init: {err}"))?;

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut guard = TerminalGuard::active();

    terminal.clear()?;

    let InitState {
        mut state,
        mut mode,
        mut input,
        keymap,
    } = init_fullscreen_state(initial_prompt);

    let mut receiver: Option<broadcast::Receiver<FrameEvent>> = None;
    if !input.trim().is_empty() {
        match start_local_session(&engine, std::mem::take(&mut input)) {
            Ok(next) => receiver = Some(next),
            Err(err) => state.set_status_message(format!("start failed: {err}")),
        }
    }

    let mut term_events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(33));
    let mut dirty = true;

    loop {
        if dirty {
            terminal.draw(|f| render(f, &state, mode, &input))?;
            dirty = false;
        }

        tokio::select! {
            _ = tick.tick() => {
                dirty = true;
            }
            maybe_event = term_events.next() => {
                let Some(Ok(event)) = maybe_event else {
                    continue;
                };
                match handle_term_event(event, &mut state, &mut mode, &mut input, receiver.is_some(), &keymap) {
                    UiAction::None => {}
                    UiAction::Quit => break,
                    UiAction::Submit => {
                        if receiver.is_none() {
                            let prompt = input.trim().to_string();
                            if !prompt.is_empty() {
                                input.clear();
                                let theme = state.theme;
                                let status_message = state.status_message.clone();
                                state = TuiState::default();
                                state.theme = theme;
                                state.status_message = status_message;
                                match start_local_session(&engine, prompt) {
                                    Ok(next) => receiver = Some(next),
                                    Err(err) => state.set_status_message(format!("start failed: {err}")),
                                }
                            }
                        }
                    }
                    UiAction::CopySelected => {
                        copy_selected(&mut terminal, &mut state)?;
                    }
                };

                dirty = true;
            }
            maybe_frame = next_frame(&mut receiver) => {
                let Some(frame) = maybe_frame else {
                    dirty = true;
                    continue;
                };
                state.update(frame);
                dirty = true;
            }
        }
    }

    guard.deactivate(&mut terminal)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SseUiMode {
    Interactive,
    Attach,
}

async fn run_fullscreen_tui_sse(
    client: &Client,
    server: String,
    initial_prompt: Option<String>,
    mut stream: Option<EventSource>,
    ui_mode: SseUiMode,
) -> anyhow::Result<()> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut guard = TerminalGuard::active();

    terminal.clear()?;

    let InitState {
        mut state,
        mut mode,
        mut input,
        keymap,
    } = init_fullscreen_state(initial_prompt);

    if ui_mode == SseUiMode::Interactive && stream.is_none() && !input.trim().is_empty() {
        match start_remote_session(client, &server, std::mem::take(&mut input)).await {
            Ok(next) => stream = Some(next),
            Err(err) => state.set_status_message(format!("start failed: {err}")),
        }
    }

    let mut term_events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(33));
    let mut dirty = true;

    loop {
        if dirty {
            terminal.draw(|f| render(f, &state, mode, &input))?;
            dirty = false;
        }

        let session_running = match ui_mode {
            SseUiMode::Attach => true,
            SseUiMode::Interactive => stream.is_some(),
        };

        tokio::select! {
            _ = tick.tick() => {
                dirty = true;
            }
            maybe_event = term_events.next() => {
                let Some(Ok(event)) = maybe_event else {
                    continue;
                };
                match handle_term_event(event, &mut state, &mut mode, &mut input, session_running, &keymap) {
                    UiAction::None => {}
                    UiAction::Quit => break,
                    UiAction::Submit => {
                        if ui_mode == SseUiMode::Interactive && stream.is_none() {
                            let prompt = input.trim().to_string();
                            if !prompt.is_empty() {
                                input.clear();
                                let theme = state.theme;
                                let status_message = state.status_message.clone();
                                state = TuiState::default();
                                state.theme = theme;
                                state.status_message = status_message;
                                match start_remote_session(client, &server, prompt).await {
                                    Ok(next) => stream = Some(next),
                                    Err(err) => state.set_status_message(format!("start failed: {err}")),
                                }
                            }
                        }
                    }
                    UiAction::CopySelected => {
                        copy_selected(&mut terminal, &mut state)?;
                    }
                };

                dirty = true;
            }
            maybe_sse = next_sse_event(&mut stream) => {
                let Some(next) = maybe_sse else {
                    dirty = true;
                    continue;
                };
                match next {
                    Ok(SseEvent::Open) => {}
                    Ok(SseEvent::Message(msg)) => {
                        if let Ok(frame) = serde_json::from_str::<FrameEvent>(&msg.data) {
                            let ended = matches!(frame.kind, rip_kernel::EventKind::SessionEnded { .. });
                            state.update(frame);
                            if ui_mode == SseUiMode::Interactive && ended {
                                stream.take();
                            }
                        }
                    }
                    Err(EventSourceError::StreamEnded) => {
                        stream.take();
                    }
                    Err(_) => {
                        stream.take();
                    }
                }
                dirty = true;
            }
        }
    }

    guard.deactivate(&mut terminal)?;
    Ok(())
}

pub async fn run_fullscreen_tui_remote(
    server: String,
    initial_prompt: Option<String>,
) -> anyhow::Result<()> {
    let client = Client::new();
    run_fullscreen_tui_sse(
        &client,
        server,
        initial_prompt,
        None,
        SseUiMode::Interactive,
    )
    .await
}

pub async fn run_fullscreen_tui_attach(server: String, session_id: String) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{server}/sessions/{session_id}/events");
    run_fullscreen_tui_sse(
        &client,
        server,
        None,
        Some(client.get(url).eventsource()?),
        SseUiMode::Attach,
    )
    .await
}

pub async fn run_fullscreen_tui_attach_task(server: String, task_id: String) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{server}/tasks/{task_id}/events");
    run_fullscreen_tui_sse(
        &client,
        server,
        None,
        Some(client.get(url).eventsource()?),
        SseUiMode::Attach,
    )
    .await
}

fn start_local_session(
    engine: &ripd::SessionEngine,
    prompt: String,
) -> Result<broadcast::Receiver<FrameEvent>, String> {
    let continuities = engine.continuities();
    let continuity_id = continuities.ensure_default()?;
    let message_id = continuities.append_message(
        &continuity_id,
        "user".to_string(),
        "tui".to_string(),
        prompt.clone(),
    )?;

    let handle = engine.create_session();
    continuities.append_run_spawned(&continuity_id, &message_id, &handle.session_id)?;
    let receiver = handle.subscribe();
    engine.spawn_session(handle, prompt);
    Ok(receiver)
}

async fn start_remote_session(
    client: &Client,
    server: &str,
    prompt: String,
) -> anyhow::Result<EventSource> {
    let thread_id = crate::ensure_thread(client, server).await?;
    let response =
        crate::post_thread_message(client, server, &thread_id, &prompt, "user", "tui").await?;
    let url = format!("{server}/sessions/{}/events", response.session_id);
    Ok(client.get(url).eventsource()?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiAction {
    None,
    Quit,
    Submit,
    CopySelected,
}

struct InitState {
    state: TuiState,
    mode: RenderMode,
    input: String,
    keymap: Keymap,
}

fn init_fullscreen_state(initial_prompt: Option<String>) -> InitState {
    let mut state = TuiState::default();
    let mode = RenderMode::Json;
    let input = initial_prompt.unwrap_or_default();

    let (keymap, keymap_warning) = Keymap::load();
    let mut warnings = Vec::new();
    match load_theme() {
        Ok(Some(theme)) => state.theme = theme,
        Ok(None) => {}
        Err(err) => warnings.push(format!("theme: {err}")),
    }
    if let Some(warn) = keymap_warning {
        warnings.push(warn);
    }
    if !warnings.is_empty() {
        state.set_status_message(warnings.join("; "));
    }

    InitState {
        state,
        mode,
        input,
        keymap,
    }
}

fn handle_term_event(
    event: TermEvent,
    state: &mut TuiState,
    mode: &mut RenderMode,
    input: &mut String,
    session_running: bool,
    keymap: &Keymap,
) -> UiAction {
    match event {
        TermEvent::Key(key) => handle_key_event(key, state, mode, input, session_running, keymap),
        TermEvent::Resize(_, _) => UiAction::None,
        _ => UiAction::None,
    }
}

fn handle_key_event(
    key: KeyEvent,
    state: &mut TuiState,
    mode: &mut RenderMode,
    input: &mut String,
    session_running: bool,
    keymap: &Keymap,
) -> UiAction {
    if let Some(cmd) = keymap.command_for(key) {
        return match cmd {
            KeyCommand::Quit => UiAction::Quit,
            KeyCommand::Submit => {
                if session_running {
                    UiAction::None
                } else {
                    UiAction::Submit
                }
            }
            KeyCommand::ToggleDetailsMode => {
                *mode = match mode {
                    RenderMode::Json => RenderMode::Decoded,
                    RenderMode::Decoded => RenderMode::Json,
                };
                UiAction::None
            }
            KeyCommand::ToggleFollow => {
                state.auto_follow = !state.auto_follow;
                UiAction::None
            }
            KeyCommand::ToggleOutputView => {
                state.toggle_output_view();
                UiAction::None
            }
            KeyCommand::ToggleTheme => {
                state.toggle_theme();
                UiAction::None
            }
            KeyCommand::CopySelected => UiAction::CopySelected,
            KeyCommand::SelectPrev => {
                state.auto_follow = false;
                move_selected(state, -1);
                UiAction::None
            }
            KeyCommand::SelectNext => {
                state.auto_follow = false;
                move_selected(state, 1);
                UiAction::None
            }
        };
    }

    if session_running {
        return UiAction::None;
    }

    match key.code {
        KeyCode::Backspace => {
            input.pop();
            UiAction::None
        }
        KeyCode::Char(ch) => {
            input.push(ch);
            UiAction::None
        }
        _ => UiAction::None,
    }
}

fn move_selected(state: &mut TuiState, delta: i64) {
    let Some(selected) = state.selected_seq else {
        state.selected_seq = state.frames.last_seq();
        return;
    };
    let next = if delta.is_negative() {
        selected.saturating_sub(delta.unsigned_abs())
    } else {
        selected.saturating_add(delta as u64)
    };
    let clamped = next
        .max(state.frames.first_seq().unwrap_or(next))
        .min(state.frames.last_seq().unwrap_or(next));
    state.selected_seq = Some(clamped);
}

async fn next_frame(receiver: &mut Option<broadcast::Receiver<FrameEvent>>) -> Option<FrameEvent> {
    let Some(rx) = receiver.as_mut() else {
        return future::pending::<Option<FrameEvent>>().await;
    };

    loop {
        match rx.recv().await {
            Ok(frame) => return Some(frame),
            Err(broadcast::error::RecvError::Closed) => {
                receiver.take();
                return None;
            }
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
        }
    }
}

async fn next_sse_event(
    source: &mut Option<EventSource>,
) -> Option<Result<SseEvent, EventSourceError>> {
    let Some(stream) = source.as_mut() else {
        return future::pending::<Option<Result<SseEvent, EventSourceError>>>().await;
    };
    stream.next().await
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
        let _ = execute!(stdout, LeaveAlternateScreen);
    }
}

const OSC52_MAX_BYTES: usize = 10_000;

fn load_theme() -> anyhow::Result<Option<rip_tui::ThemeId>> {
    if let Some(raw) = std::env::var_os("RIP_TUI_THEME") {
        return parse_theme(&raw.to_string_lossy());
    }

    let path = theme_path().ok_or_else(|| anyhow::anyhow!("missing $HOME for theme.json"))?;
    let Ok(contents) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };

    let value: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|err| anyhow::anyhow!("theme.json invalid json at {}: {err}", path.display()))?;

    match value {
        serde_json::Value::String(s) => parse_theme(&s),
        serde_json::Value::Object(map) => map
            .get("theme")
            .and_then(|v| v.as_str())
            .map(parse_theme)
            .transpose()
            .map(|theme| theme.flatten()),
        _ => Ok(None),
    }
}

fn parse_theme(raw: &str) -> anyhow::Result<Option<rip_tui::ThemeId>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }

    match raw.to_ascii_lowercase().as_str() {
        "default-dark" | "dark" => Ok(Some(rip_tui::ThemeId::DefaultDark)),
        "default-light" | "light" => Ok(Some(rip_tui::ThemeId::DefaultLight)),
        _ => Err(anyhow::anyhow!("unknown theme '{raw}'")),
    }
}

fn theme_path() -> Option<PathBuf> {
    Some(config_dir()?.join("theme.json"))
}

fn config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("RIP_CONFIG_HOME") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".rip"))
}

fn copy_selected(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut TuiState,
) -> anyhow::Result<()> {
    let action = prepare_copy_selected(state);
    let CopySelectedAction::Osc52(payload) = action else {
        return Ok(());
    };

    let seq = osc52_sequence(payload.as_bytes());
    terminal.backend_mut().write_all(seq.as_bytes())?;
    terminal.backend_mut().flush()?;

    state.clipboard_buffer = None;
    state.set_status_message("clipboard: osc52");
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CopySelectedAction {
    None,
    Store,
    Osc52(String),
}

fn prepare_copy_selected(state: &mut TuiState) -> CopySelectedAction {
    let Some(event) = state.selected_event() else {
        state.set_status_message("clipboard: no frame selected");
        return CopySelectedAction::None;
    };

    let payload = match serde_json::to_string_pretty(event) {
        Ok(json) => json,
        Err(_) => {
            state.set_status_message("clipboard: failed to serialize frame");
            return CopySelectedAction::None;
        }
    };

    let osc52_disabled = std::env::var_os("RIP_TUI_DISABLE_OSC52").is_some();
    if osc52_disabled || payload.len() > OSC52_MAX_BYTES {
        state.clipboard_buffer = Some(payload);
        if osc52_disabled {
            state.set_status_message("clipboard: stored (OSC52 disabled)");
        } else {
            state.set_status_message("clipboard: stored (too large for OSC52)");
        }
        return CopySelectedAction::Store;
    }

    CopySelectedAction::Osc52(payload)
}

fn osc52_sequence(bytes: &[u8]) -> String {
    let encoded = base64_encode(bytes);
    format!("\x1b]52;c;{encoded}\x07")
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity((bytes.len().saturating_add(2) / 3) * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push(TABLE[(n & 0x3f) as usize] as char);
        i += 3;
    }

    match bytes.len().saturating_sub(i) {
        0 => {}
        1 => {
            let n = (bytes[i] as u32) << 16;
            out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
            out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => unreachable!("len mod 3 is always 0..=2"),
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use rip_kernel::{EventKind, ProviderEventStatus};
    use std::ffi::OsString;
    use tokio::time::timeout;

    fn seed_state() -> TuiState {
        let mut state = TuiState::new(100, 1024);
        state.update(FrameEvent {
            id: "e0".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        });
        state.update(FrameEvent {
            id: "e1".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 1,
            kind: EventKind::ProviderEvent {
                provider: "openresponses".to_string(),
                status: ProviderEventStatus::Done,
                event_name: None,
                data: None,
                raw: None,
                errors: Vec::new(),
                response_errors: Vec::new(),
            },
        });
        state.update(FrameEvent {
            id: "e2".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 2,
            kind: EventKind::SessionEnded {
                reason: "done".to_string(),
            },
        });
        state
    }

    #[test]
    fn parse_theme_accepts_known_values() {
        assert_eq!(
            parse_theme("default-dark").unwrap(),
            Some(rip_tui::ThemeId::DefaultDark)
        );
        assert_eq!(
            parse_theme("light").unwrap(),
            Some(rip_tui::ThemeId::DefaultLight)
        );
        assert!(parse_theme("nope").is_err());
    }

    #[test]
    fn parse_theme_empty_returns_none() {
        assert_eq!(parse_theme("   ").unwrap(), None);
    }

    #[test]
    fn osc52_sequence_wraps_base64_payload() {
        let seq = osc52_sequence(b"hi");
        assert!(seq.starts_with("\u{1b}]52;c;"));
        assert!(seq.ends_with('\u{7}'));
        assert!(seq.contains("aGk="));
    }

    #[test]
    fn base64_encode_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"hi"), "aGk=");
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }

    #[test]
    fn handle_key_event_applies_keymap_commands() {
        let keymap = Keymap::default();
        let mut state = seed_state();
        let mut mode = RenderMode::Json;
        let mut input = String::new();

        // Up selects previous event.
        assert_eq!(state.selected_seq, Some(2));
        let action = handle_key_event(
            KeyEvent::new(KeyCode::Up, KeyModifiers::empty()),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::None);
        assert_eq!(state.selected_seq, Some(1));

        // Ctrl+R toggles output view.
        assert_eq!(state.output_view, rip_tui::OutputViewMode::Rendered);
        let action = handle_key_event(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::None);
        assert_eq!(state.output_view, rip_tui::OutputViewMode::Raw);

        // Ctrl+Y triggers copy.
        let action = handle_key_event(
            KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::CopySelected);

        // Enter submits only when not running.
        let action = handle_key_event(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::None);

        let action = handle_key_event(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
        assert_eq!(action, UiAction::Submit);
    }

    #[test]
    fn handle_key_event_inserts_text_when_idle() {
        let keymap = Keymap::default();
        let mut state = seed_state();
        let mut mode = RenderMode::Json;
        let mut input = String::new();

        let action = handle_key_event(
            KeyEvent::new(KeyCode::Char('a'), KeyModifiers::empty()),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
        assert_eq!(action, UiAction::None);
        assert_eq!(input, "a");

        let action = handle_key_event(
            KeyEvent::new(KeyCode::Backspace, KeyModifiers::empty()),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
        assert_eq!(action, UiAction::None);
        assert_eq!(input, "");
    }

    #[test]
    fn handle_term_event_ignores_resize() {
        let keymap = Keymap::default();
        let mut state = seed_state();
        let mut mode = RenderMode::Json;
        let mut input = String::new();
        let action = handle_term_event(
            TermEvent::Resize(10, 10),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
        assert_eq!(action, UiAction::None);
    }

    #[test]
    fn handle_term_event_routes_key() {
        let keymap = Keymap::default();
        let mut state = seed_state();
        let mut mode = RenderMode::Json;
        let mut input = String::new();
        let action = handle_term_event(
            TermEvent::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::empty())),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
        assert_eq!(action, UiAction::Submit);
    }

    #[test]
    fn handle_key_event_toggles_mode_follow_theme() {
        let keymap = Keymap::default();
        let mut state = seed_state();
        let mut mode = RenderMode::Json;
        let mut input = String::new();

        let action = handle_key_event(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::empty()),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::None);
        assert_eq!(mode, RenderMode::Decoded);

        let action = handle_key_event(
            KeyEvent::new(KeyCode::Char('f'), KeyModifiers::CONTROL),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::None);
        assert!(!state.auto_follow);

        let previous_theme = state.theme;
        let action = handle_key_event(
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::None);
        assert_ne!(state.theme, previous_theme);
    }

    #[test]
    fn move_selected_sets_last_seq_and_clamps() {
        let mut state = seed_state();
        state.selected_seq = None;
        move_selected(&mut state, -1);
        assert_eq!(state.selected_seq, Some(2));

        move_selected(&mut state, 10);
        assert_eq!(state.selected_seq, Some(2));

        move_selected(&mut state, -10);
        assert_eq!(state.selected_seq, Some(0));
    }

    #[tokio::test]
    async fn next_frame_reads_and_handles_close() {
        let (tx, rx) = broadcast::channel(2);
        let mut receiver = Some(rx);
        let frame = FrameEvent {
            id: "e3".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 3,
            kind: EventKind::SessionEnded {
                reason: "done".to_string(),
            },
        };
        tx.send(frame.clone()).expect("send");
        let got = timeout(Duration::from_millis(50), next_frame(&mut receiver))
            .await
            .expect("timeout");
        assert_eq!(got.unwrap().seq, 3);

        drop(tx);
        let got = timeout(Duration::from_millis(50), next_frame(&mut receiver))
            .await
            .expect("timeout");
        assert!(got.is_none());
        assert!(receiver.is_none());
    }

    #[tokio::test]
    async fn next_frame_skips_lagged() {
        let (tx, rx) = broadcast::channel(1);
        let mut receiver = Some(rx);
        let first = FrameEvent {
            id: "e1".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 1,
            kind: EventKind::SessionStarted {
                input: "one".to_string(),
            },
        };
        let second = FrameEvent {
            id: "e2".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 2,
            kind: EventKind::SessionEnded {
                reason: "done".to_string(),
            },
        };
        tx.send(first).expect("send first");
        tx.send(second.clone()).expect("send second");
        let got = timeout(Duration::from_millis(50), next_frame(&mut receiver))
            .await
            .expect("timeout")
            .expect("frame");
        assert_eq!(got.seq, second.seq);
    }

    #[tokio::test]
    async fn next_sse_event_returns_open() {
        let server = MockServer::start();
        let _events = server.mock(|when, then| {
            when.method(GET).path("/events");
            then.status(200)
                .header("content-type", "text/event-stream")
                .body("data: {}\n\n");
        });

        let client = Client::new();
        let url = format!("{}/events", server.base_url());
        let mut source = Some(client.get(url).eventsource().expect("eventsource"));
        let next = timeout(Duration::from_millis(200), next_sse_event(&mut source))
            .await
            .expect("timeout");
        assert!(next.is_some());
    }

    #[tokio::test]
    async fn next_sse_event_pending_when_none() {
        let mut source: Option<EventSource> = None;
        let result = timeout(Duration::from_millis(10), next_sse_event(&mut source)).await;
        assert!(result.is_err());
    }

    struct EnvGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = self.previous.take() {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }

    fn set_env(key: &'static str, value: impl Into<OsString>) -> EnvGuard {
        let previous = std::env::var_os(key);
        let value = value.into();
        std::env::set_var(key, &value);
        EnvGuard { key, previous }
    }

    fn remove_env(key: &'static str) -> EnvGuard {
        let previous = std::env::var_os(key);
        std::env::remove_var(key);
        EnvGuard { key, previous }
    }

    #[test]
    fn load_theme_reads_env_and_file() {
        let _lock = test_env::lock_env();
        let _clear_theme = remove_env("RIP_TUI_THEME");
        let temp_root = std::env::temp_dir().join(format!("rip_theme_test_{}", std::process::id()));
        std::fs::create_dir_all(&temp_root).expect("temp dir");
        let theme_path = temp_root.join("theme.json");

        let _config = set_env("RIP_CONFIG_HOME", temp_root.as_os_str());
        std::fs::write(&theme_path, "\"default-dark\"").expect("theme");
        assert_eq!(
            load_theme().expect("theme load"),
            Some(rip_tui::ThemeId::DefaultDark)
        );

        std::fs::write(&theme_path, "{ \"theme\": \"light\" }").expect("theme");
        assert_eq!(
            load_theme().expect("theme load"),
            Some(rip_tui::ThemeId::DefaultLight)
        );

        let _theme_env = set_env("RIP_TUI_THEME", "dark");
        assert_eq!(
            load_theme().expect("theme load"),
            Some(rip_tui::ThemeId::DefaultDark)
        );
        drop(_theme_env);

        std::fs::write(&theme_path, "{").expect("theme");
        assert!(load_theme().is_err());
    }

    #[test]
    fn load_theme_missing_file_returns_none() {
        let _lock = test_env::lock_env();
        let _clear_theme = remove_env("RIP_TUI_THEME");
        let temp_root =
            std::env::temp_dir().join(format!("rip_theme_missing_{}", std::process::id()));
        let _config = set_env("RIP_CONFIG_HOME", temp_root.as_os_str());
        let value = load_theme().expect("load theme");
        assert!(value.is_none());
    }

    #[test]
    fn config_dir_prefers_env_override() {
        let _lock = test_env::lock_env();
        let temp_root =
            std::env::temp_dir().join(format!("rip_config_test_{}", std::process::id()));
        let _config = set_env("RIP_CONFIG_HOME", temp_root.as_os_str());
        let _home = set_env("HOME", "/tmp");
        assert_eq!(config_dir().expect("config dir"), temp_root);
        assert_eq!(
            theme_path().expect("theme path"),
            temp_root.join("theme.json")
        );
    }

    #[test]
    fn config_dir_falls_back_to_home() {
        let _lock = test_env::lock_env();
        let _config = remove_env("RIP_CONFIG_HOME");
        let temp_home = std::env::temp_dir().join(format!("rip_home_{}", std::process::id()));
        let _home = set_env("HOME", temp_home.as_os_str());
        assert_eq!(config_dir().expect("config dir"), temp_home.join(".rip"));
    }

    #[test]
    fn init_fullscreen_state_sets_warning_on_theme_error() {
        let _lock = test_env::lock_env();
        let _bad_theme = set_env("RIP_TUI_THEME", "unknown-theme");
        let init = init_fullscreen_state(Some("hello".to_string()));
        assert_eq!(init.mode, RenderMode::Json);
        assert_eq!(init.input, "hello");
        let status = init.state.status_message.unwrap_or_default();
        assert!(status.contains("theme:"));
    }

    #[test]
    fn init_fullscreen_state_includes_keymap_warning() {
        let _lock = test_env::lock_env();
        let _clear_theme = set_env("RIP_TUI_THEME", "dark");
        let temp_root = std::env::temp_dir().join(format!("rip_keys_{}", std::process::id()));
        std::fs::create_dir_all(&temp_root).expect("temp dir");
        let keymap_path = temp_root.join("keybindings.json");
        std::fs::write(&keymap_path, "{").expect("keymap");
        let _keymap = set_env("RIP_KEYBINDINGS_PATH", keymap_path.as_os_str());

        let init = init_fullscreen_state(None);
        let status = init.state.status_message.unwrap_or_default();
        assert!(status.contains("keybindings: invalid json"));
    }

    #[test]
    fn prepare_copy_selected_reports_no_selection() {
        let mut state = TuiState::default();
        let action = prepare_copy_selected(&mut state);
        assert_eq!(action, CopySelectedAction::None);
        assert_eq!(
            state.status_message.as_deref(),
            Some("clipboard: no frame selected")
        );
    }

    #[test]
    fn prepare_copy_selected_uses_osc52_for_small_payload() {
        let _lock = test_env::lock_env();
        let _disable = remove_env("RIP_TUI_DISABLE_OSC52");
        let mut state = seed_state();
        let action = prepare_copy_selected(&mut state);
        assert!(matches!(action, CopySelectedAction::Osc52(_)));
    }

    #[test]
    fn prepare_copy_selected_stores_when_disabled() {
        let _lock = test_env::lock_env();
        let _disable = set_env("RIP_TUI_DISABLE_OSC52", "1");
        let mut state = seed_state();
        let action = prepare_copy_selected(&mut state);
        assert_eq!(action, CopySelectedAction::Store);
        assert!(state.clipboard_buffer.is_some());
        let status = state.status_message.unwrap_or_default();
        assert!(status.contains("OSC52 disabled"));
    }

    #[test]
    fn prepare_copy_selected_stores_when_large() {
        let _lock = test_env::lock_env();
        let _disable = remove_env("RIP_TUI_DISABLE_OSC52");
        let mut state = TuiState::default();
        let payload = "x".repeat(OSC52_MAX_BYTES + 100);
        state.update(FrameEvent {
            id: "big".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted { input: payload },
        });
        state.selected_seq = Some(0);

        let action = prepare_copy_selected(&mut state);
        assert_eq!(action, CopySelectedAction::Store);
        let status = state.status_message.unwrap_or_default();
        assert!(status.contains("too large"));
    }
}
