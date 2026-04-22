use std::future;
use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{EnableMouseCapture, EventStream};
use crossterm::terminal::{enable_raw_mode, EnterAlternateScreen};
use crossterm::ExecutableCommand;
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use ratatui_textarea::TextArea;
use reqwest::Client;
use reqwest_eventsource::{
    Error as EventSourceError, Event as SseEvent, EventSource, RequestBuilderExt,
};
use rip_kernel::Event as FrameEvent;
use rip_tui::{
    canvas_screen_regions, render, reveal_focused_canvas_message, PaletteOrigin, RenderMode,
    TuiState,
};
use serde_json::Value;
use tokio::sync::mpsc;

mod actions;
mod copy;
mod events;
mod keymap;
mod palette;
mod terminal;
mod theme;
mod thread_picker;

use copy::copy_selected;
use events::{
    buffer_is_effectively_empty, buffer_trimmed_prompt, handle_term_event, last_user_prompt,
    move_selected, UiAction,
};
use keymap::Keymap;
use palette::{
    apply_palette_selection, cycle_palette_mode_with_overrides, load_model_palette_catalog,
    open_command_palette, open_go_to_palette, open_model_palette,
    open_options_palette_with_overrides, open_threads_palette, sync_preferred_openresponses_state,
};
use terminal::TerminalGuard;
use theme::load_theme;
use thread_picker::load_thread_picker_entries;

#[cfg(test)]
use copy::{base64_encode, osc52_sequence, prepare_copy_selected, CopySelectedAction, CopySource};
#[cfg(test)]
use events::{
    handle_key_event, handle_mouse_event, mouse_canvas_hit_geometry, mouse_footer_activity_row,
};
#[cfg(test)]
use palette::{
    apply_command_action, apply_model_palette_selection, openresponses_override_input_from_json,
};
#[cfg(test)]
use theme::{config_dir, parse_theme, theme_path};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SseUiMode {
    Interactive,
    Attach,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TickMotionSignature {
    IdleBreath(u8),
    Thinking(u8),
    StreamingPulse(bool),
    Stalled,
}

async fn run_fullscreen_tui_sse(
    client: &Client,
    server: String,
    initial_prompt: Option<String>,
    mut stream: Option<EventSource>,
    mut active_session_id: Option<String>,
    ui_mode: SseUiMode,
    openresponses_overrides: Option<Value>,
) -> anyhow::Result<()> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let mut guard = TerminalGuard::active();

    terminal.clear()?;

    let InitState {
        mut state,
        mut mode,
        mut input,
        keymap,
    } = init_fullscreen_state(initial_prompt);
    let mut current_overrides = openresponses_overrides;
    let mut model_catalog = load_model_palette_catalog(current_overrides.as_ref());
    sync_preferred_openresponses_state(&mut state, current_overrides.as_ref(), &model_catalog);

    if ui_mode == SseUiMode::Interactive && stream.is_none() && !buffer_is_effectively_empty(&input)
    {
        let prompt = buffer_trimmed_prompt(&input);
        input.clear();
        state.begin_pending_turn(&prompt);
        terminal.draw(|f| render(f, &state, mode, &input))?;
        match start_remote_session(client, &server, prompt, current_overrides.clone()).await {
            Ok(next) => {
                state.set_continuity_id(next.thread_id);
                active_session_id = Some(next.session_id);
                stream = Some(next.stream);
            }
            Err(err) => {
                state.awaiting_response = false;
                state.set_status_message(format!("start failed: {err}"));
            }
        }
    }

    let mut term_events = EventStream::new();
    // The old 33ms unconditional tick forced a full repaint ~30x/sec
    // even when the only thing changing was a slow breath/thinking
    // glyph. Keep a lightweight cadence, but only redraw when the
    // visible animation phase actually changed.
    let mut tick = tokio::time::interval(Duration::from_millis(125));
    let mut dirty = true;
    let mut last_tick_signature = None;
    let (status_tx, mut status_rx) = mpsc::channel::<String>(16);

    loop {
        if dirty {
            state.set_now_ms(current_time_ms());
            reveal_pending_canvas_focus(&mut terminal, &mut state, &input)?;
            terminal.draw(|f| render(f, &state, mode, &input))?;
            last_tick_signature = tick_motion_signature(&state, &input);
            dirty = false;
        }

        let session_running = match ui_mode {
            SseUiMode::Attach => true,
            SseUiMode::Interactive => stream.is_some(),
        };

        tokio::select! {
            _ = tick.tick() => {
                state.set_now_ms(current_time_ms());
                let signature = tick_motion_signature(&state, &input);
                if signature != last_tick_signature {
                    dirty = true;
                    last_tick_signature = signature;
                }
            }
            maybe_status = status_rx.recv() => {
                if let Some(message) = maybe_status {
                    state.set_status_message(message);
                    dirty = true;
                }
            }
            maybe_event = term_events.next() => {
                let Some(Ok(event)) = maybe_event else {
                    continue;
                };
                match handle_term_event(event, &mut state, &mut mode, &mut input, session_running, &keymap) {
                    UiAction::None => {}
                    UiAction::Quit => {
                        if let Some(session_id) = active_session_id.as_deref() {
                            match cancel_remote_session(client, &server, session_id).await {
                                Ok(()) => break,
                                Err(err) => {
                                    state.set_status_message(format!("stop failed: {err}"));
                                }
                            }
                        } else {
                            break;
                        }
                    }
                    UiAction::CancelSession => {
                        if let Some(session_id) = active_session_id.as_deref() {
                            match cancel_remote_session(client, &server, session_id).await {
                                Ok(()) => {
                                    state.set_status_message("stopping session…");
                                }
                                Err(err) => {
                                    state.set_status_message(format!("stop failed: {err}"));
                                }
                            }
                        } else {
                            state.set_status_message("this stream cannot be stopped here");
                        }
                    }
                    UiAction::Submit => {
                        if ui_mode == SseUiMode::Interactive && stream.is_none() {
                            let prompt = buffer_trimmed_prompt(&input);
                            if !prompt.is_empty() {
                                input.clear();
                                state.begin_pending_turn(&prompt);
                                terminal.draw(|f| render(f, &state, mode, &input))?;
                                match start_remote_session(
                                    client,
                                    &server,
                                    prompt,
                                    current_overrides.clone(),
                                )
                                .await
                                {
                                    Ok(next) => {
                                        state.set_continuity_id(next.thread_id);
                                        active_session_id = Some(next.session_id);
                                        stream = Some(next.stream);
                                    }
                                    Err(err) => {
                                        state.awaiting_response = false;
                                        state.set_status_message(format!("start failed: {err}"));
                                    }
                                }
                            }
                        }
                    }
                    UiAction::CloseOverlay => {
                        state.close_overlay();
                    }
                    UiAction::TogglePalette => {
                        if ui_mode == SseUiMode::Interactive {
                            // C.5: `⌃K` now opens the Command palette
                            // (the primary entry point). Models stays
                            // one hotkey away (`M-m`) and one palette
                            // mode cycle (`Tab`) away.
                            open_command_palette(&mut state, PaletteOrigin::TopCenter);
                        }
                    }
                    UiAction::OpenPaletteModels => {
                        if ui_mode == SseUiMode::Interactive {
                            open_model_palette(
                                &mut state,
                                &model_catalog,
                                PaletteOrigin::TopRight,
                            );
                        }
                    }
                    UiAction::OpenPaletteGoTo => {
                        open_go_to_palette(&mut state, PaletteOrigin::Center);
                    }
                    UiAction::OpenPaletteThreads => {
                        if ui_mode == SseUiMode::Interactive {
                            match load_thread_picker_entries(
                                client,
                                &server,
                                state.continuity_id.as_deref(),
                            )
                            .await
                            {
                                Ok(entries) => state.open_thread_picker(entries),
                                Err(err) => {
                                    open_threads_palette(&mut state, PaletteOrigin::TopLeft);
                                    state.set_status_message(err);
                                }
                            }
                        }
                    }
                    UiAction::OpenPaletteOptions => {
                        open_options_palette_with_overrides(
                            &mut state,
                            current_overrides.as_ref(),
                            PaletteOrigin::BottomCenter,
                        );
                    }
                    UiAction::ShowHelp => {
                        state.set_overlay(rip_tui::Overlay::Help);
                    }
                    UiAction::PaletteCycleMode => {
                        cycle_palette_mode_with_overrides(
                            &mut state,
                            &model_catalog,
                            current_overrides.as_ref(),
                        );
                    }
                    UiAction::ApplyPalette => {
                        if let Err(err) = apply_palette_selection(
                            &mut state,
                            &mut current_overrides,
                            &mut model_catalog,
                        ) {
                            state.set_status_message(err);
                        }
                    }
                    UiAction::ApplyThreadPicker => {
                        if let Some(thread_id) = state.thread_picker_selected_value() {
                            state.set_continuity_id(thread_id.clone());
                            state.set_status_message(format!(
                                "next run targets thread: {thread_id}"
                            ));
                            state.close_overlay();
                        } else {
                            state.set_status_message("thread picker: no thread selected");
                        }
                    }
                    UiAction::ToggleActivity => {
                        state.toggle_activity_overlay();
                    }
                    UiAction::ToggleTasks => {
                        state.toggle_tasks_overlay();
                    }
                    UiAction::OpenSelectedDetail => {
                        state.open_selected_detail();
                    }
                    UiAction::OpenFocusedDetail => {
                        // Phase B.4: `x` on a focused card routes into the
                        // per-item detail overlay scoped to that card's
                        // tool/task. Future Phase C.8 lands a proper X-ray
                        // overlay that narrows to the card's frame range.
                        if let Some(overlay) = focused_detail_overlay(&state) {
                            state.set_overlay(overlay);
                        }
                    }
                    UiAction::ExpandFocusedCard => {
                        state.toggle_focused_card_expanded();
                    }
                    UiAction::CompactionCutPoints => {
                        if ui_mode == SseUiMode::Interactive {
                            actions::spawn_compaction_cut_points(
                                client.clone(),
                                server.clone(),
                                status_tx.clone(),
                            );
                        }
                    }
                    UiAction::CompactionAuto => {
                        if ui_mode == SseUiMode::Interactive {
                            actions::spawn_compaction_auto(
                                client.clone(),
                                server.clone(),
                                status_tx.clone(),
                            );
                        }
                    }
                    UiAction::CompactionAutoSchedule => {
                        if ui_mode == SseUiMode::Interactive {
                            actions::spawn_compaction_auto_schedule(
                                client.clone(),
                                server.clone(),
                                status_tx.clone(),
                            );
                        }
                    }
                    UiAction::CompactionStatus => {
                        if ui_mode == SseUiMode::Interactive {
                            actions::spawn_compaction_status(
                                client.clone(),
                                server.clone(),
                                status_tx.clone(),
                            );
                        }
                    }
                    UiAction::ProviderCursorStatus => {
                        if ui_mode == SseUiMode::Interactive {
                            actions::spawn_provider_cursor_status(
                                client.clone(),
                                server.clone(),
                                status_tx.clone(),
                            );
                        }
                    }
                    UiAction::ProviderCursorRotate => {
                        if ui_mode == SseUiMode::Interactive {
                            actions::spawn_provider_cursor_rotate(
                                client.clone(),
                                server.clone(),
                                status_tx.clone(),
                            );
                        }
                    }
                    UiAction::ContextSelectionStatus => {
                        if ui_mode == SseUiMode::Interactive {
                            actions::spawn_context_selection_status(
                                client.clone(),
                                server.clone(),
                                status_tx.clone(),
                            );
                        }
                    }
                    UiAction::CopySelected => {
                        copy_selected(&mut terminal, &mut state)?;
                    }
                    UiAction::ScrollCanvasTop => {
                        if state.output_view == rip_tui::OutputViewMode::Rendered {
                            state.scroll_canvas_up(u16::MAX);
                        } else {
                            state.auto_follow = false;
                            state.selected_seq = state.frames.first_seq();
                        }
                    }
                    UiAction::ScrollCanvasBottom => {
                        if state.output_view == rip_tui::OutputViewMode::Rendered {
                            state.scroll_canvas_to_bottom();
                        } else {
                            state.auto_follow = true;
                            state.selected_seq = state.frames.last_seq();
                        }
                    }
                    UiAction::ScrollCanvasUp => {
                        if state.output_view == rip_tui::OutputViewMode::Rendered {
                            state.scroll_canvas_up(4);
                        } else {
                            state.auto_follow = false;
                            move_selected(&mut state, -8);
                        }
                    }
                    UiAction::ScrollCanvasDown => {
                        if state.output_view == rip_tui::OutputViewMode::Rendered {
                            state.scroll_canvas_down(4);
                        } else {
                            state.auto_follow = false;
                            move_selected(&mut state, 8);
                        }
                    }
                    UiAction::ErrorRecoveryRetry => {
                        // `r` → re-post the last user message on the
                        // same continuity so the kernel spawns a
                        // fresh retry run. We go through the same
                        // start_remote_session path the Submit arm
                        // uses; nothing new about the capability
                        // contract, just a different trigger.
                        if ui_mode == SseUiMode::Interactive {
                            if let Some(prompt) = last_user_prompt(&state) {
                                let prompt = prompt.to_string();
                                state.close_overlay();
                                state.begin_pending_turn(&prompt);
                                match start_remote_session(
                                    client,
                                    &server,
                                    prompt,
                                    current_overrides.clone(),
                                )
                                .await
                                {
                                    Ok(next) => {
                                        state.set_continuity_id(next.thread_id);
                                        active_session_id = Some(next.session_id);
                                        stream = Some(next.stream);
                                    }
                                    Err(err) => {
                                        state.awaiting_response = false;
                                        state
                                            .set_status_message(format!("retry failed: {err}"));
                                    }
                                }
                            } else {
                                state.set_status_message(
                                    "retry: no previous user message to re-post",
                                );
                            }
                        }
                    }
                    UiAction::ErrorRecoveryRotateCursor => {
                        if ui_mode == SseUiMode::Interactive {
                            actions::spawn_error_recovery_rotate_cursor(
                                client.clone(),
                                server.clone(),
                                status_tx.clone(),
                            );
                            state.close_overlay();
                        }
                    }
                    UiAction::ErrorRecoverySwitchModel => {
                        if ui_mode == SseUiMode::Interactive {
                            // Swap the error-recovery overlay for the
                            // Models palette so the operator picks a
                            // model, then invokes retry manually.
                            open_model_palette(
                                &mut state,
                                &model_catalog,
                                PaletteOrigin::TopRight,
                            );
                        }
                    }
                    UiAction::ErrorRecoveryXray => {
                        if let rip_tui::Overlay::ErrorRecovery { seq } = state.overlay() {
                            let seq = *seq;
                            state.set_overlay(rip_tui::Overlay::ErrorDetail { seq });
                        }
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
                            if ended {
                                active_session_id = None;
                                stream.take();
                            }
                        }
                    }
                    Err(EventSourceError::StreamEnded) => {
                        state.awaiting_response = false;
                        active_session_id = None;
                        if state.status_message.is_none() {
                            state.set_status_message("stream ended");
                        }
                        stream.take();
                    }
                    Err(err) => {
                        state.awaiting_response = false;
                        active_session_id = None;
                        state.set_status_message(format!("stream error: {err}"));
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

fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn reveal_pending_canvas_focus(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut TuiState,
    input: &TextArea<'static>,
) -> io::Result<()> {
    if state.output_view != rip_tui::OutputViewMode::Rendered || !state.focus_reveal_pending() {
        return Ok(());
    }

    let area = terminal.size()?;
    let regions = canvas_screen_regions(state, area.into(), input);
    reveal_focused_canvas_message(state, regions.canvas.width, regions.canvas.height);
    Ok(())
}

fn tick_motion_signature(
    state: &TuiState,
    input: &TextArea<'static>,
) -> Option<TickMotionSignature> {
    let now_ms = state.now_ms.unwrap_or(0);

    if primary_turn_is_thinking(state) {
        return Some(TickMotionSignature::Thinking(((now_ms / 400) % 4) as u8));
    }

    if primary_turn_is_streaming(state) {
        let hot = state
            .last_event_ms
            .is_some_and(|last_ms| now_ms.saturating_sub(last_ms) < 350);
        return Some(TickMotionSignature::StreamingPulse(hot));
    }

    if state.is_stalled(5_000) {
        return Some(TickMotionSignature::Stalled);
    }

    if input_is_fully_idle(state, input) {
        let phase = if (800..1600).contains(&(now_ms % 2400)) {
            1
        } else {
            0
        };
        return Some(TickMotionSignature::IdleBreath(phase));
    }

    None
}

fn input_is_fully_idle(state: &TuiState, input: &TextArea<'static>) -> bool {
    matches!(state.overlay(), rip_tui::Overlay::None)
        && !state.awaiting_response
        && state.pending_prompt.is_none()
        && buffer_is_effectively_empty(input)
}

fn primary_turn_is_thinking(state: &TuiState) -> bool {
    state.canvas.messages.iter().rev().any(|message| {
        matches!(
            message,
            rip_tui::CanvasMessage::AgentTurn {
                role: rip_tui::AgentRole::Primary,
                streaming: true,
                blocks,
                streaming_tail,
                ..
            } if blocks.is_empty() && streaming_tail.is_empty()
        )
    })
}

fn primary_turn_is_streaming(state: &TuiState) -> bool {
    state.canvas.messages.iter().rev().any(|message| {
        matches!(
            message,
            rip_tui::CanvasMessage::AgentTurn {
                role: rip_tui::AgentRole::Primary,
                streaming: true,
                ..
            }
        )
    })
}

pub async fn run_fullscreen_tui_remote(
    server: String,
    initial_prompt: Option<String>,
    openresponses_overrides: Option<Value>,
) -> anyhow::Result<()> {
    let client = Client::new();
    run_fullscreen_tui_sse(
        &client,
        server,
        initial_prompt,
        None,
        None,
        SseUiMode::Interactive,
        openresponses_overrides,
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
        Some(session_id),
        SseUiMode::Attach,
        None,
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
        None,
        SseUiMode::Attach,
        None,
    )
    .await
}

struct StartedRemoteSession {
    thread_id: String,
    session_id: String,
    stream: EventSource,
}

async fn start_remote_session(
    client: &Client,
    server: &str,
    prompt: String,
    openresponses_overrides: Option<Value>,
) -> anyhow::Result<StartedRemoteSession> {
    let thread_id = crate::ensure_thread(client, server).await?;
    let response = crate::post_thread_message(
        client,
        server,
        &thread_id,
        &prompt,
        "user",
        "tui",
        openresponses_overrides,
    )
    .await?;
    let url = format!("{server}/sessions/{}/events", response.session_id);
    Ok(StartedRemoteSession {
        thread_id,
        session_id: response.session_id,
        stream: client.get(url).eventsource()?,
    })
}

async fn cancel_remote_session(
    client: &Client,
    server: &str,
    session_id: &str,
) -> anyhow::Result<()> {
    let response = client
        .post(format!("{server}/sessions/{session_id}/cancel"))
        .send()
        .await?;
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if body.trim().is_empty() {
        anyhow::bail!("server returned {status}");
    }
    anyhow::bail!("server returned {status}: {}", body.trim());
}

struct InitState {
    state: TuiState,
    mode: RenderMode,
    input: TextArea<'static>,
    keymap: Keymap,
}

fn init_fullscreen_state(initial_prompt: Option<String>) -> InitState {
    let mut state = TuiState::default();
    let mode = RenderMode::Json;
    let mut input = TextArea::default();
    if let Some(prompt) = initial_prompt {
        if !prompt.is_empty() {
            input.insert_str(&prompt);
        }
    }

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

/// `x` on a focused canvas item opens the per-item detail overlay. For
/// now this routes into the existing `ToolDetail` / `TaskDetail` overlays
/// (scoped to that tool/task id); Phase C.8 replaces them with a proper
/// `XrayOverlay` that takes a `(from_seq, to_seq)` window.
pub(super) fn focused_detail_overlay(state: &TuiState) -> Option<rip_tui::Overlay> {
    use rip_tui::{CanvasMessage, Overlay};
    match state.focused_message()? {
        CanvasMessage::ToolCard { tool_id, .. } => Some(Overlay::ToolDetail {
            tool_id: tool_id.clone(),
        }),
        CanvasMessage::TaskCard { task_id, .. } => Some(Overlay::TaskDetail {
            task_id: task_id.clone(),
        }),
        CanvasMessage::SystemNotice { seq, .. } => Some(Overlay::ErrorDetail { seq: *seq }),
        _ => None,
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

#[cfg(test)]
mod tests;
