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
use rip_tui::{render, PaletteOrigin, RenderMode, TuiState};
use serde_json::Value;
use tokio::sync::mpsc;

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
    apply_palette_selection, cycle_palette_mode, load_model_palette_catalog, open_command_palette,
    open_go_to_palette, open_model_palette, open_options_palette, open_threads_palette,
};
use terminal::TerminalGuard;
use theme::load_theme;
use thread_picker::load_thread_picker_entries;

#[cfg(test)]
use copy::{base64_encode, osc52_sequence, prepare_copy_selected, CopySelectedAction};
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

async fn run_fullscreen_tui_sse(
    client: &Client,
    server: String,
    initial_prompt: Option<String>,
    mut stream: Option<EventSource>,
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
    state.set_preferred_openresponses_target(
        model_catalog.current_endpoint.clone(),
        model_catalog.current_model.clone(),
    );

    if ui_mode == SseUiMode::Interactive && stream.is_none() && !buffer_is_effectively_empty(&input)
    {
        let prompt = buffer_trimmed_prompt(&input);
        input.clear();
        state.begin_pending_turn(&prompt);
        terminal.draw(|f| render(f, &state, mode, &input))?;
        match start_remote_session(client, &server, prompt, current_overrides.clone()).await {
            Ok(next) => {
                state.set_continuity_id(next.thread_id);
                stream = Some(next.stream);
            }
            Err(err) => {
                state.awaiting_response = false;
                state.set_status_message(format!("start failed: {err}"));
            }
        }
    }

    let mut term_events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(33));
    let mut dirty = true;
    let (status_tx, mut status_rx) = mpsc::channel::<String>(16);

    loop {
        state.set_now_ms(current_time_ms());
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
                    UiAction::Quit => break,
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
                        open_options_palette(&mut state, PaletteOrigin::BottomCenter);
                    }
                    UiAction::ShowHelp => {
                        state.set_overlay(rip_tui::Overlay::Help);
                    }
                    UiAction::PaletteCycleMode => {
                        cycle_palette_mode(&mut state, &model_catalog);
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
                            let client = client.clone();
                            let server = server.clone();
                            let tx = status_tx.clone();
                            tokio::spawn(async move {
                                let message = match crate::ensure_thread(&client, &server).await {
                                    Ok(thread_id) => {
                                        let url = format!("{server}/threads/{thread_id}/compaction-cut-points");
                                        let response = client
                                            .post(url)
                                            .json(&serde_json::json!({ "stride_messages": null, "limit": 1 }))
                                            .send()
                                            .await;
                                        match response {
                                            Ok(resp) if resp.status().is_success() => {
                                                match resp.json::<ripd::CompactionCutPointsV1Response>().await {
                                                    Ok(out) => {
                                                        let latest = out.cut_points.first();
                                                        match latest {
                                                            Some(cp) => format!(
                                                                "cut_points: messages={} latest ordinal={} to_seq={} checkpointed={}",
                                                                out.message_count, cp.target_message_ordinal, cp.to_seq, cp.already_checkpointed
                                                            ),
                                                            None => format!("cut_points: messages={} (no eligible cut points)", out.message_count),
                                                        }
                                                    }
                                                    Err(err) => format!("cut_points: parse failed: {err}"),
                                                }
                                            }
                                            Ok(resp) => format!("cut_points: request failed: {}", resp.status()),
                                            Err(err) => format!("cut_points: request failed: {err}"),
                                        }
                                    }
                                    Err(err) => format!("cut_points: thread ensure failed: {err}"),
                                };
                                let _ = tx.send(message).await;
                            });
                        }
                    }
                    UiAction::CompactionAuto => {
                        if ui_mode == SseUiMode::Interactive {
                            let client = client.clone();
                            let server = server.clone();
                            let tx = status_tx.clone();
                            tokio::spawn(async move {
                                let message = match crate::ensure_thread(&client, &server).await {
                                    Ok(thread_id) => {
                                        let url = format!("{server}/threads/{thread_id}/compaction-auto");
                                        let response = client
                                            .post(url)
                                            .json(&serde_json::json!({
                                                "stride_messages": null,
                                                "max_new_checkpoints": null,
                                                "dry_run": false,
                                                "actor_id": "user",
                                                "origin": "tui"
                                            }))
                                            .send()
                                            .await;
                                        match response {
                                            Ok(resp) if resp.status().is_success() => {
                                                match resp.json::<ripd::CompactionAutoV1Response>().await {
                                                    Ok(out) => match out.job_id {
                                                        Some(job_id) => format!("compaction auto: status={} job_id={job_id}", out.status),
                                                        None => format!("compaction auto: status={}", out.status),
                                                    },
                                                    Err(err) => format!("compaction auto: parse failed: {err}"),
                                                }
                                            }
                                            Ok(resp) => format!("compaction auto: request failed: {}", resp.status()),
                                            Err(err) => format!("compaction auto: request failed: {err}"),
                                        }
                                    }
                                    Err(err) => format!("compaction auto: thread ensure failed: {err}"),
                                };
                                let _ = tx.send(message).await;
                            });
                        }
                    }
                    UiAction::CompactionAutoSchedule => {
                        if ui_mode == SseUiMode::Interactive {
                            let client = client.clone();
                            let server = server.clone();
                            let tx = status_tx.clone();
                            tokio::spawn(async move {
                                let message = match crate::ensure_thread(&client, &server).await {
                                    Ok(thread_id) => {
                                        let url =
                                            format!("{server}/threads/{thread_id}/compaction-auto-schedule");
                                        let response = client
                                            .post(url)
                                            .json(&serde_json::json!({
                                                "stride_messages": null,
                                                "max_new_checkpoints": null,
                                                "block_on_inflight": true,
                                                "execute": true,
                                                "dry_run": false,
                                                "actor_id": "user",
                                                "origin": "tui"
                                            }))
                                            .send()
                                            .await;
                                        match response {
                                            Ok(resp) if resp.status().is_success() => {
                                                match resp.json::<ripd::CompactionAutoScheduleV1Response>().await {
                                                    Ok(out) => match out.job_id {
                                                        Some(job_id) => format!(
                                                            "compaction schedule: decision={} job_id={job_id}",
                                                            out.decision
                                                        ),
                                                        None => format!(
                                                            "compaction schedule: decision={}",
                                                            out.decision
                                                        ),
                                                    },
                                                    Err(err) => format!("compaction schedule: parse failed: {err}"),
                                                }
                                            }
                                            Ok(resp) => format!(
                                                "compaction schedule: request failed: {}",
                                                resp.status()
                                            ),
                                            Err(err) => format!("compaction schedule: request failed: {err}"),
                                        }
                                    }
                                    Err(err) => format!("compaction schedule: thread ensure failed: {err}"),
                                };
                                let _ = tx.send(message).await;
                            });
                        }
                    }
                    UiAction::CompactionStatus => {
                        if ui_mode == SseUiMode::Interactive {
                            let client = client.clone();
                            let server = server.clone();
                            let tx = status_tx.clone();
                            tokio::spawn(async move {
                                let message = match crate::ensure_thread(&client, &server).await {
                                    Ok(thread_id) => {
                                        let url =
                                            format!("{server}/threads/{thread_id}/compaction-status");
                                        let response = client
                                            .post(url)
                                            .json(&serde_json::json!({ "stride_messages": null }))
                                            .send()
                                            .await;
                                        match response {
                                            Ok(resp) if resp.status().is_success() => match resp
                                                .json::<ripd::CompactionStatusV1Response>()
                                                .await
                                            {
                                                Ok(status) => {
                                                    let ckpt = status
                                                        .latest_checkpoint
                                                        .as_ref()
                                                        .map(|c| c.to_seq.to_string())
                                                        .unwrap_or_else(|| "none".to_string());
                                                    let next = status
                                                        .next_cut_point
                                                        .as_ref()
                                                        .map(|c| c.to_seq.to_string())
                                                        .unwrap_or_else(|| "none".to_string());
                                                    let sched = status
                                                        .last_schedule_decision
                                                        .as_ref()
                                                        .map(|d| d.decision.as_str())
                                                        .unwrap_or("none");
                                                    let job = status
                                                        .last_job_outcome
                                                        .as_ref()
                                                        .map(|j| j.status.as_str())
                                                        .unwrap_or("none");
                                                    let inflight = status
                                                        .inflight_job_id
                                                        .as_deref()
                                                        .map(|id| {
                                                            let short = id
                                                                .chars()
                                                                .take(16)
                                                                .collect::<String>();
                                                            format!(" inflight={short}")
                                                        })
                                                        .unwrap_or_default();
                                                    format!(
                                                        "compaction status: messages={} ckpt_to_seq={} next_to_seq={} sched={} job={}{}",
                                                        status.message_count, ckpt, next, sched, job, inflight
                                                    )
                                                }
                                                Err(err) => format!(
                                                    "compaction status: parse failed: {err}"
                                                ),
                                            },
                                            Ok(resp) => format!(
                                                "compaction status: request failed: {}",
                                                resp.status()
                                            ),
                                            Err(err) => {
                                                format!("compaction status: request failed: {err}")
                                            }
                                        }
                                    }
                                    Err(err) => format!(
                                        "compaction status: thread ensure failed: {err}"
                                    ),
                                };
                                let _ = tx.send(message).await;
                            });
                        }
                    }
                    UiAction::ProviderCursorStatus => {
                        if ui_mode == SseUiMode::Interactive {
                            let client = client.clone();
                            let server = server.clone();
                            let tx = status_tx.clone();
                            tokio::spawn(async move {
                                let message = match crate::ensure_thread(&client, &server).await {
                                    Ok(thread_id) => {
                                        let url =
                                            format!("{server}/threads/{thread_id}/provider-cursor-status");
                                        let response = client
                                            .post(url)
                                            .json(&serde_json::json!({}))
                                            .send()
                                            .await;
                                        match response {
                                            Ok(resp) if resp.status().is_success() => match resp
                                                .json::<ripd::ProviderCursorStatusV1Response>()
                                                .await
                                            {
                                                Ok(status) => match status.active {
                                                    Some(active) => {
                                                        let prev = active
                                                            .cursor
                                                            .as_ref()
                                                            .and_then(|value| {
                                                                value
                                                                    .get("previous_response_id")
                                                                    .and_then(|value| value.as_str())
                                                            })
                                                            .unwrap_or("");
                                                        let prev_short = prev
                                                            .chars()
                                                            .take(16)
                                                            .collect::<String>();
                                                        let cursor_desc = if active.cursor.is_some()
                                                            && !prev_short.is_empty()
                                                        {
                                                            format!("prev={prev_short}")
                                                        } else if active.cursor.is_some() {
                                                            "cursor=set".to_string()
                                                        } else {
                                                            "cursor=none".to_string()
                                                        };
                                                        format!(
                                                            "provider cursor: action={} {}",
                                                            active.action, cursor_desc
                                                        )
                                                    }
                                                    None => "provider cursor: none".to_string(),
                                                },
                                                Err(err) => {
                                                    format!("provider cursor status: parse failed: {err}")
                                                }
                                            },
                                            Ok(resp) => format!(
                                                "provider cursor status: request failed: {}",
                                                resp.status()
                                            ),
                                            Err(err) => {
                                                format!("provider cursor status: request failed: {err}")
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        format!("provider cursor status: thread ensure failed: {err}")
                                    }
                                };
                                let _ = tx.send(message).await;
                            });
                        }
                    }
                    UiAction::ProviderCursorRotate => {
                        if ui_mode == SseUiMode::Interactive {
                            let client = client.clone();
                            let server = server.clone();
                            let tx = status_tx.clone();
                            tokio::spawn(async move {
                                let message = match crate::ensure_thread(&client, &server).await {
                                    Ok(thread_id) => {
                                        let url =
                                            format!("{server}/threads/{thread_id}/provider-cursor-rotate");
                                        let response = client
                                            .post(url)
                                            .json(&serde_json::json!({
                                                "provider": null,
                                                "endpoint": null,
                                                "model": null,
                                                "reason": "tui",
                                                "actor_id": "user",
                                                "origin": "tui"
                                            }))
                                            .send()
                                            .await;
                                        match response {
                                            Ok(resp) if resp.status().is_success() => match resp
                                                .json::<ripd::ProviderCursorRotateV1Response>()
                                                .await
                                            {
                                                Ok(out) => {
                                                    if out.rotated {
                                                        "provider cursor: rotated".to_string()
                                                    } else {
                                                        "provider cursor: rotate noop".to_string()
                                                    }
                                                }
                                                Err(err) => {
                                                    format!("provider cursor rotate: parse failed: {err}")
                                                }
                                            },
                                            Ok(resp) => format!(
                                                "provider cursor rotate: request failed: {}",
                                                resp.status()
                                            ),
                                            Err(err) => {
                                                format!("provider cursor rotate: request failed: {err}")
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        format!("provider cursor rotate: thread ensure failed: {err}")
                                    }
                                };
                                let _ = tx.send(message).await;
                            });
                        }
                    }
                    UiAction::ContextSelectionStatus => {
                        if ui_mode == SseUiMode::Interactive {
                            let client = client.clone();
                            let server = server.clone();
                            let tx = status_tx.clone();
                            tokio::spawn(async move {
                                let message = match crate::ensure_thread(&client, &server).await {
                                    Ok(thread_id) => {
                                        let url = format!(
                                            "{server}/threads/{thread_id}/context-selection-status"
                                        );
                                        let response = client
                                            .post(url)
                                            .json(&serde_json::json!({ "limit": 1 }))
                                            .send()
                                            .await;
                                        match response {
                                            Ok(resp) if resp.status().is_success() => match resp
                                                .json::<ripd::ContextSelectionStatusV1Response>()
                                                .await
                                            {
                                                Ok(status) => match status.decisions.first() {
                                                    Some(active) => {
                                                        let ckpt = active
                                                            .compaction_checkpoint
                                                            .as_ref()
                                                            .map(|c| c.to_seq.to_string())
                                                            .unwrap_or_else(|| "none".to_string());
                                                        format!(
                                                            "context selection: strategy={} ckpt_to_seq={} resets={}",
                                                            active.compiler_strategy,
                                                            ckpt,
                                                            active.resets.len()
                                                        )
                                                    }
                                                    None => "context selection: none".to_string(),
                                                },
                                                Err(err) => {
                                                    format!(
                                                        "context selection status: parse failed: {err}"
                                                    )
                                                }
                                            },
                                            Ok(resp) => format!(
                                                "context selection status: request failed: {}",
                                                resp.status()
                                            ),
                                            Err(err) => format!(
                                                "context selection status: request failed: {err}"
                                            ),
                                        }
                                    }
                                    Err(err) => {
                                        format!("context selection: thread ensure failed: {err}")
                                    }
                                };
                                let _ = tx.send(message).await;
                            });
                        }
                    }
                    UiAction::CopySelected => {
                        copy_selected(&mut terminal, &mut state)?;
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
                            let client = client.clone();
                            let server = server.clone();
                            let tx = status_tx.clone();
                            tokio::spawn(async move {
                                let message =
                                    match crate::ensure_thread(&client, &server).await {
                                        Ok(thread_id) => {
                                            let url = format!(
                                                "{server}/threads/{thread_id}/provider-cursor-rotate"
                                            );
                                            match client.post(url).send().await {
                                                Ok(resp) if resp.status().is_success() => {
                                                    "provider cursor rotated".to_string()
                                                }
                                                Ok(resp) => format!(
                                                    "rotate cursor: {}",
                                                    resp.status()
                                                ),
                                                Err(err) => format!("rotate cursor: {err}"),
                                            }
                                        }
                                        Err(err) => format!("rotate cursor: {err}"),
                                    };
                                let _ = tx.send(message).await;
                            });
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
                            if ui_mode == SseUiMode::Interactive && ended {
                                stream.take();
                            }
                        }
                    }
                    Err(EventSourceError::StreamEnded) => {
                        state.awaiting_response = false;
                        if state.status_message.is_none() {
                            state.set_status_message("stream ended");
                        }
                        stream.take();
                    }
                    Err(err) => {
                        state.awaiting_response = false;
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
        SseUiMode::Attach,
        None,
    )
    .await
}

struct StartedRemoteSession {
    thread_id: String,
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
        stream: client.get(url).eventsource()?,
    })
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
