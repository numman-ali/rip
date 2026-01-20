use std::future;
use std::io;
use std::time::Duration;

use crossterm::event::{Event as TermEvent, EventStream, KeyCode, KeyEvent, KeyModifiers};
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
                match handle_term_event(event, &mut state, &mut mode, &mut input, receiver.is_some()) {
                    UiAction::None => {}
                    UiAction::Quit => break,
                    UiAction::Submit => {
                        if receiver.is_none() {
                            let prompt = input.trim().to_string();
                            if !prompt.is_empty() {
                                input.clear();
                                state = TuiState::default();
                                receiver = Some(start_local_session(&engine, prompt));
                            }
                        }
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
                match handle_term_event(event, &mut state, &mut mode, &mut input, true) {
                    UiAction::None | UiAction::Submit => {}
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

enum UiAction {
    None,
    Quit,
    Submit,
}

fn handle_term_event(
    event: TermEvent,
    state: &mut TuiState,
    mode: &mut RenderMode,
    input: &mut String,
    session_running: bool,
) -> UiAction {
    match event {
        TermEvent::Key(key) => handle_key_event(key, state, mode, input, session_running),
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
) -> UiAction {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return UiAction::Quit;
    }

    match key.code {
        KeyCode::Char('q') if !key.modifiers.contains(KeyModifiers::CONTROL) => UiAction::Quit,
        KeyCode::Tab => {
            *mode = match mode {
                RenderMode::Json => RenderMode::Decoded,
                RenderMode::Decoded => RenderMode::Json,
            };
            UiAction::None
        }
        KeyCode::Char('f') => {
            state.auto_follow = !state.auto_follow;
            UiAction::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            state.auto_follow = false;
            move_selected(state, -1);
            UiAction::None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            state.auto_follow = false;
            move_selected(state, 1);
            UiAction::None
        }
        KeyCode::Enter => UiAction::Submit,
        KeyCode::Backspace => {
            input.pop();
            UiAction::None
        }
        KeyCode::Char(ch) => {
            if !session_running {
                input.push(ch);
            }
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
