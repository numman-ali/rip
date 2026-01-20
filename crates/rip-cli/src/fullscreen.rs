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

    let mut state = TuiState::default();
    let mut mode = RenderMode::Json;
    let mut input = initial_prompt.unwrap_or_default();

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

    let mut receiver: Option<broadcast::Receiver<FrameEvent>> = None;
    if !input.trim().is_empty() {
        receiver = Some(start_local_session(&engine, std::mem::take(&mut input)));
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
                                receiver = Some(start_local_session(&engine, prompt));
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

pub async fn run_fullscreen_tui_attach(server: String, session_id: String) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{server}/sessions/{session_id}/events");
    let mut stream = Some(client.get(url).eventsource()?);

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut guard = TerminalGuard::active();

    terminal.clear()?;

    let mut state = TuiState::default();
    let mut mode = RenderMode::Json;
    let mut input = String::new();

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
                match handle_term_event(event, &mut state, &mut mode, &mut input, true, &keymap) {
                    UiAction::None | UiAction::Submit => {}
                    UiAction::CopySelected => {
                        copy_selected(&mut terminal, &mut state)?;
                    }
                    UiAction::Quit => break,
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
                            state.update(frame);
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

fn start_local_session(
    engine: &ripd::SessionEngine,
    prompt: String,
) -> broadcast::Receiver<FrameEvent> {
    let handle = engine.create_session();
    let receiver = handle.subscribe();
    engine.spawn_session(handle, prompt);
    receiver
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiAction {
    None,
    Quit,
    Submit,
    CopySelected,
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
    let Some(event) = state.selected_event() else {
        state.set_status_message("clipboard: no frame selected");
        return Ok(());
    };

    let payload = match serde_json::to_string_pretty(event) {
        Ok(json) => json,
        Err(_) => {
            state.set_status_message("clipboard: failed to serialize frame");
            return Ok(());
        }
    };

    if std::env::var_os("RIP_TUI_DISABLE_OSC52").is_some() || payload.len() > OSC52_MAX_BYTES {
        state.clipboard_buffer = Some(payload);
        if std::env::var_os("RIP_TUI_DISABLE_OSC52").is_some() {
            state.set_status_message("clipboard: stored (OSC52 disabled)");
        } else {
            state.set_status_message("clipboard: stored (too large for OSC52)");
        }
        return Ok(());
    }

    let seq = osc52_sequence(payload.as_bytes());
    terminal.backend_mut().write_all(seq.as_bytes())?;
    terminal.backend_mut().flush()?;

    state.clipboard_buffer = None;
    state.set_status_message("clipboard: osc52");
    Ok(())
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
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use rip_kernel::{EventKind, ProviderEventStatus};

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
}
