use std::collections::BTreeMap;
use std::future;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event as TermEvent, EventStream, KeyCode, KeyEvent,
    KeyModifiers, MouseEvent, MouseEventKind,
};
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
use rip_tui::palette::modes::models::{
    infer_provider_id_from_endpoint, push_route_from_string, upsert_model_route,
};
use rip_tui::{
    render, ModelRoute, ModelsMode, PaletteMode, PaletteSource, RenderMode, ResolvedModelRoute,
    TuiState,
};
use serde_json::Value;
use tokio::sync::mpsc;

mod keymap;

use keymap::{Command as KeyCommand, Keymap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SseUiMode {
    Interactive,
    Attach,
}

fn load_model_palette_catalog(openresponses_overrides: Option<&Value>) -> ModelsMode {
    let workspace_root = crate::local_authority::default_workspace_root();
    let (resolved, loaded) = ripd::resolve_openresponses_config(
        &workspace_root,
        openresponses_override_input_from_json(openresponses_overrides),
    );
    let mut routes_by_value = BTreeMap::<String, ModelRoute>::new();
    let mut provider_endpoints = BTreeMap::<String, String>::new();

    for (provider_id, provider_cfg) in &loaded.config.provider {
        let endpoint = provider_cfg
            .endpoint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        if let Some(endpoint) = endpoint.clone() {
            provider_endpoints.insert(provider_id.clone(), endpoint.clone());
        }

        if let Some(endpoint) = endpoint {
            for (model_id, model_cfg) in &provider_cfg.models {
                upsert_model_route(
                    &mut routes_by_value,
                    provider_id,
                    model_id,
                    &endpoint,
                    model_cfg.label.clone(),
                    model_cfg.variants.len(),
                    "catalog",
                );
            }
        }
    }

    if let Some(route) = loaded.config.model.as_deref() {
        push_route_from_string(
            &mut routes_by_value,
            &provider_endpoints,
            route,
            "config:model",
        );
    }
    if let Some(route) = loaded
        .config
        .roles
        .get("primary")
        .and_then(|route| route.to_route_string())
    {
        push_route_from_string(
            &mut routes_by_value,
            &provider_endpoints,
            &route,
            "config:roles.primary",
        );
    }
    for (role, route) in &loaded.config.roles {
        if role == "primary" {
            continue;
        }
        if let Some(route) = route.to_route_string() {
            push_route_from_string(
                &mut routes_by_value,
                &provider_endpoints,
                &route,
                &format!("config:roles.{role}"),
            );
        }
    }
    if let Some(route) = loaded.config.small_model.as_deref() {
        push_route_from_string(
            &mut routes_by_value,
            &provider_endpoints,
            route,
            "config:small_model",
        );
    }

    if let Some(resolved) = resolved.as_ref() {
        if let (Some(route), Some(endpoint), Some(model)) = (
            resolved.effective_route.as_deref(),
            Some(resolved.endpoint.as_str()),
            resolved.model.as_deref(),
        ) {
            let provider_id = resolved
                .provider_id
                .clone()
                .or_else(|| infer_provider_id_from_endpoint(endpoint))
                .unwrap_or_else(|| "openresponses".to_string());
            upsert_model_route(
                &mut routes_by_value,
                &provider_id,
                model,
                endpoint,
                None,
                0,
                "current",
            );
            if !routes_by_value.contains_key(route) {
                push_route_from_string(&mut routes_by_value, &provider_endpoints, route, "current");
            }
        }
    }

    ModelsMode::new(
        routes_by_value.into_values().collect(),
        provider_endpoints,
        resolved
            .as_ref()
            .and_then(|cfg| cfg.effective_route.clone()),
        resolved.as_ref().map(|cfg| cfg.endpoint.clone()),
        resolved.and_then(|cfg| cfg.model),
    )
}

fn openresponses_override_input_from_json(
    value: Option<&Value>,
) -> ripd::OpenResponsesOverrideInput {
    let Some(Value::Object(obj)) = value else {
        return ripd::OpenResponsesOverrideInput::default();
    };

    ripd::OpenResponsesOverrideInput {
        endpoint: obj
            .get("endpoint")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string()),
        model: obj
            .get("model")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string()),
        stateless_history: obj
            .get("stateless_history")
            .and_then(|value| value.as_bool()),
        parallel_tool_calls: obj
            .get("parallel_tool_calls")
            .and_then(|value| value.as_bool()),
        followup_user_message: obj
            .get("followup_user_message")
            .and_then(|value| value.as_str())
            .map(|value| value.to_string()),
    }
}

fn open_model_palette(state: &mut TuiState, catalog: &ModelsMode) {
    let empty_message = catalog.empty_state().to_string();
    let custom_prompt = catalog.allow_custom().unwrap_or("").to_string();
    state.open_palette(
        PaletteMode::Model,
        catalog.entries(),
        empty_message,
        catalog.allow_custom().is_some(),
        custom_prompt,
    );
}

/// C.5: Command palette — the primary entry point. Surfaces the full
/// list of `CommandAction`s (see `rip_tui::palette::modes::command`)
/// tagged with category subtitles and an `unavailable` chip for
/// [deferred] entries whose backing capability is not yet in the
/// registry.
fn open_command_palette(state: &mut TuiState) {
    use rip_tui::palette::modes::command::CommandMode;
    let mode = CommandMode::new();
    state.open_palette(
        PaletteMode::Command,
        mode.entries(),
        mode.empty_state().to_string(),
        false,
        String::new(),
    );
}

/// C.5: Go To palette — a fuzzy-search over canvas messages.
fn open_go_to_palette(state: &mut TuiState) {
    use rip_tui::palette::modes::go_to::GoToMode;
    let mode = GoToMode::from_canvas(&state.canvas);
    let empty_message = mode.empty_state().to_string();
    let entries = mode.entries();
    state.open_palette(
        PaletteMode::Navigation,
        entries,
        empty_message,
        false,
        String::new(),
    );
}

/// C.5: Threads palette — minimal local-runtime form ships only the
/// current thread until the driver wires up `thread.list` seeding.
fn open_threads_palette(state: &mut TuiState) {
    use rip_tui::palette::modes::threads::{ThreadSummary, ThreadsMode};
    let current = state
        .continuity_id
        .as_deref()
        .map(|id| ThreadSummary {
            thread_id: id.to_string(),
            title: None,
            last_message_preview: None,
            updated_at_ms: None,
            is_current: true,
        })
        .into_iter()
        .collect();
    let mode = ThreadsMode::new(current);
    let empty_message = mode.empty_state().to_string();
    state.open_palette(
        PaletteMode::Session,
        mode.entries(),
        empty_message,
        false,
        String::new(),
    );
}

/// C.5: Options palette — toggles for UI-local prefs. Reads the
/// current state so each entry's subtitle reflects the active value.
fn open_options_palette(state: &mut TuiState) {
    use rip_tui::palette::modes::options::OptionsMode;
    let mode = OptionsMode {
        current_theme: Some(state.theme.as_str()),
        auto_follow: state.auto_follow,
        reasoning_visible: false,
        vim_input_mode: false,
        mouse_capture: true,
        activity_rail_pinned: state.activity_pinned,
    };
    let entries = mode.entries();
    state.open_palette(
        PaletteMode::Option,
        entries,
        mode.empty_state().to_string(),
        false,
        String::new(),
    );
}

/// C.5: cycle palette mode when `Tab` is pressed inside an open
/// palette. Order mirrors the visual ranking in the plan:
/// Command → Models → Go To → Threads → Options → Command.
fn cycle_palette_mode(state: &mut TuiState, catalog: &ModelsMode) {
    let next = match state.overlay() {
        rip_tui::Overlay::Palette(p) => match p.mode {
            PaletteMode::Command => PaletteMode::Model,
            PaletteMode::Model => PaletteMode::Navigation,
            PaletteMode::Navigation => PaletteMode::Session,
            PaletteMode::Session => PaletteMode::Option,
            PaletteMode::Option => PaletteMode::Command,
        },
        _ => return,
    };
    match next {
        PaletteMode::Command => open_command_palette(state),
        PaletteMode::Model => open_model_palette(state, catalog),
        PaletteMode::Navigation => open_go_to_palette(state),
        PaletteMode::Session => open_threads_palette(state),
        PaletteMode::Option => open_options_palette(state),
    }
}

/// C.5: mode-aware apply. Routes the currently-selected palette entry
/// to the appropriate dispatcher:
/// - `Command` → map the value (a `CommandAction` id) to a concrete
///   action handler.
/// - `Model` → existing `apply_model_palette_selection` path.
/// - `Navigation` (Go To) → focus the target canvas message.
/// - `Session` (Threads) → set the continuity id.
/// - `Option` → treat as a command id (same table as Command mode).
fn apply_palette_selection(
    state: &mut TuiState,
    overrides: &mut Option<Value>,
    catalog: &mut ModelsMode,
) -> Result<(), String> {
    use rip_tui::palette::modes::command::CommandAction;

    let Some(overlay) = state.palette_state_clone() else {
        return Err("palette: no palette open".to_string());
    };
    match overlay.mode {
        PaletteMode::Model => apply_model_palette_selection(state, overrides, catalog),
        PaletteMode::Navigation => {
            let Some(value) = state.palette_selected_value() else {
                return Err("palette: no entry selected".to_string());
            };
            state.focused_message_id = Some(value);
            state.close_overlay();
            Ok(())
        }
        PaletteMode::Session => {
            let Some(value) = state.palette_selected_value() else {
                return Err("palette: no entry selected".to_string());
            };
            state.set_continuity_id(value.clone());
            state.set_status_message(format!("switched thread: {value}"));
            state.close_overlay();
            Ok(())
        }
        PaletteMode::Command | PaletteMode::Option => {
            let Some(value) = state.palette_selected_value() else {
                return Err("palette: no entry selected".to_string());
            };
            let Some(action) = CommandAction::from_value(&value) else {
                return Err(format!("palette: unknown action '{value}'"));
            };
            if !action.is_available() {
                state.set_status_message(format!(
                    "{}: capability not supported yet",
                    action.title()
                ));
                state.close_overlay();
                return Ok(());
            }
            apply_command_action(action, state, catalog);
            state.close_overlay();
            Ok(())
        }
    }
}

/// C.5: map a `CommandAction` to its concrete effect. Capability-
/// backed actions (compaction, cursor rotate, etc.) still flow
/// through their existing async dispatch in the main loop — we
/// surface them via `state.set_status_message` so users get feedback
/// that the palette accepted their choice. The heavy async path
/// (reach over HTTP) will be wired in Phase C.10 when error-recovery
/// lands — for Phase C.5 the table is: toggles apply immediately,
/// palette openers re-open the right mode, everything else posts a
/// "coming soon" status note pointing at the capability.
fn apply_command_action(
    action: rip_tui::palette::modes::command::CommandAction,
    state: &mut TuiState,
    catalog: &ModelsMode,
) {
    use rip_tui::palette::modes::command::CommandAction as A;
    match action {
        A::ScrollCanvasTop => state.scroll_canvas_up(u16::MAX),
        A::ScrollCanvasBottom => {
            state.canvas_scroll_from_bottom = 0;
            state.auto_follow = true;
        }
        A::FollowTail => state.auto_follow = !state.auto_follow,
        A::PrevMessage => state.focus_prev_message(),
        A::NextMessage => state.focus_next_message(),
        A::PrevError => {
            if let Some(seq) = state.last_error_seq {
                state.selected_seq = Some(seq);
            }
        }
        A::ClearSelection => {
            state.clear_focus();
        }
        A::ToggleTheme => state.toggle_theme(),
        A::ToggleAutoFollow => state.auto_follow = !state.auto_follow,
        A::ShowDebugInfo => state.set_overlay(rip_tui::Overlay::Debug),
        A::OpenXrayOnFocused => {
            if let Some(overlay) = focused_detail_overlay(state) {
                state.set_overlay(overlay);
            }
        }
        A::SwitchModel => {
            open_model_palette(state, catalog);
        }
        A::Quit => {
            // Signalled via the global Quit action in the caller; the
            // palette apply path can't return UiAction::Quit without
            // refactoring the outer loop. Surface a status note so
            // users know `Ctrl-C` is the canonical path.
            state.set_status_message("press Ctrl-C to quit".to_string());
        }
        // Everything else surfaces as a status hint — its backing
        // capability call is owned by the outer loop's dedicated
        // UiAction (see the KeyCommand → UiAction arms below) or by
        // future phases. This keeps the palette semantically useful
        // without silently failing.
        other => {
            state.set_status_message(format!(
                "{}: use the dedicated hotkey or command (palette routing lands in a later phase)",
                other.title()
            ));
        }
    }
}

fn apply_model_palette_selection(
    state: &mut TuiState,
    overrides: &mut Option<Value>,
    catalog: &mut ModelsMode,
) -> Result<(), String> {
    let Some(selection) = state.palette_selected_value() else {
        return Err("palette: no model selected".to_string());
    };
    let resolved: ResolvedModelRoute = catalog.resolve_selection(&selection)?;
    let mut map = match overrides.take() {
        Some(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    map.insert(
        "endpoint".to_string(),
        Value::String(resolved.endpoint.clone()),
    );
    map.insert("model".to_string(), Value::String(resolved.model.clone()));
    *overrides = Some(Value::Object(map));
    catalog.record_resolution(&resolved);
    state.set_preferred_openresponses_target(Some(resolved.endpoint), Some(resolved.model));
    state.close_overlay();
    state.set_status_message(format!("next model: {}", resolved.route));
    Ok(())
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

    if ui_mode == SseUiMode::Interactive && stream.is_none() && !input.trim().is_empty() {
        let prompt = std::mem::take(&mut input);
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
                            let prompt = input.trim().to_string();
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
                            open_command_palette(&mut state);
                        }
                    }
                    UiAction::OpenPaletteModels => {
                        if ui_mode == SseUiMode::Interactive {
                            open_model_palette(&mut state, &model_catalog);
                        }
                    }
                    UiAction::OpenPaletteGoTo => {
                        open_go_to_palette(&mut state);
                    }
                    UiAction::OpenPaletteThreads => {
                        open_threads_palette(&mut state);
                    }
                    UiAction::OpenPaletteOptions => {
                        open_options_palette(&mut state);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UiAction {
    None,
    Quit,
    Submit,
    CloseOverlay,
    /// Primary palette trigger — `⌃K` opens the Command palette
    /// (Phase C.5). Backward-compat: when the operator has a
    /// `C-k → TogglePalette` binding in `~/.rip/keybindings.json`,
    /// that still opens a palette; the driver now routes it to the
    /// Command mode instead of straight to Models.
    TogglePalette,
    /// `⌃M` / `Alt+M` → Models palette directly.
    OpenPaletteModels,
    /// `⌃G` → Go To palette.
    OpenPaletteGoTo,
    /// `⌃T` → Threads palette.
    OpenPaletteThreads,
    /// `Alt+O` → Options palette.
    OpenPaletteOptions,
    /// `?` → Help overlay (Phase C.7).
    ShowHelp,
    /// `Tab` inside an open palette cycles through modes in order
    /// Command → Models → Go To → Threads → Options → Command…
    /// Outside the palette this is a no-op (the legacy details-mode
    /// toggle is retired per the plan).
    PaletteCycleMode,
    ApplyPalette,
    ToggleActivity,
    ToggleTasks,
    OpenSelectedDetail,
    OpenFocusedDetail,
    ExpandFocusedCard,
    CopySelected,
    CompactionAuto,
    CompactionAutoSchedule,
    CompactionCutPoints,
    CompactionStatus,
    ProviderCursorStatus,
    ProviderCursorRotate,
    ContextSelectionStatus,
    ScrollCanvasUp,
    ScrollCanvasDown,
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
        TermEvent::Mouse(mouse) => handle_mouse_event(mouse, state),
        TermEvent::Resize(_, _) => UiAction::None,
        _ => UiAction::None,
    }
}

fn handle_mouse_event(mouse: MouseEvent, state: &mut TuiState) -> UiAction {
    if state.is_palette_open() {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                state.palette_move_selection(-1);
                return UiAction::None;
            }
            MouseEventKind::ScrollDown => {
                state.palette_move_selection(1);
                return UiAction::None;
            }
            _ => {}
        }
    }

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if state.output_view == rip_tui::OutputViewMode::Rendered {
                UiAction::ScrollCanvasUp
            } else {
                state.auto_follow = false;
                move_selected(state, -6);
                UiAction::None
            }
        }
        MouseEventKind::ScrollDown => {
            if state.output_view == rip_tui::OutputViewMode::Rendered {
                UiAction::ScrollCanvasDown
            } else {
                state.auto_follow = false;
                move_selected(state, 6);
                UiAction::None
            }
        }
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
    if state.is_palette_open() {
        if let Some(cmd) = keymap.command_for(key) {
            return match cmd {
                KeyCommand::Quit => UiAction::Quit,
                KeyCommand::Submit => UiAction::ApplyPalette,
                KeyCommand::CloseOverlay | KeyCommand::TogglePalette => UiAction::CloseOverlay,
                KeyCommand::PaletteCycleMode => UiAction::PaletteCycleMode,
                KeyCommand::SelectPrev => {
                    state.palette_move_selection(-1);
                    UiAction::None
                }
                KeyCommand::SelectNext => {
                    state.palette_move_selection(1);
                    UiAction::None
                }
                KeyCommand::ScrollCanvasUp => {
                    state.palette_move_selection(-5);
                    UiAction::None
                }
                KeyCommand::ScrollCanvasDown => {
                    state.palette_move_selection(5);
                    UiAction::None
                }
                KeyCommand::ToggleTheme => {
                    state.toggle_theme();
                    UiAction::None
                }
                _ => UiAction::None,
            };
        }

        return match key.code {
            KeyCode::Backspace => {
                state.palette_backspace();
                UiAction::None
            }
            KeyCode::Char(ch) => {
                state.palette_push_char(ch);
                UiAction::None
            }
            _ => UiAction::None,
        };
    }

    if let Some(cmd) = keymap.command_for(key) {
        return match cmd {
            KeyCommand::Quit => UiAction::Quit,
            KeyCommand::Submit => {
                // `⏎` is contextual per the revamp plan (Part 9.1): if a
                // tool/task card is focused and the input is empty, Enter
                // toggles expand on that card. Otherwise: submit (when
                // the editor is the focus) or open the detail overlay
                // for the selected frame (when a run is active).
                if input.trim().is_empty() && card_expand_target(state) {
                    UiAction::ExpandFocusedCard
                } else if session_running {
                    UiAction::OpenSelectedDetail
                } else {
                    UiAction::Submit
                }
            }
            KeyCommand::CloseOverlay => UiAction::CloseOverlay,
            KeyCommand::TogglePalette => UiAction::TogglePalette,
            KeyCommand::PaletteModels => UiAction::OpenPaletteModels,
            KeyCommand::PaletteGoTo => UiAction::OpenPaletteGoTo,
            KeyCommand::PaletteThreads => UiAction::OpenPaletteThreads,
            KeyCommand::PaletteOptions => UiAction::OpenPaletteOptions,
            KeyCommand::ShowHelp => UiAction::ShowHelp,
            KeyCommand::PaletteCycleMode => UiAction::None,
            KeyCommand::ToggleActivity => UiAction::ToggleActivity,
            KeyCommand::ToggleTasks => UiAction::ToggleTasks,
            KeyCommand::FocusPrevMessage => {
                state.focus_prev_message();
                UiAction::None
            }
            KeyCommand::FocusNextMessage => {
                state.focus_next_message();
                UiAction::None
            }
            KeyCommand::FocusClear => {
                state.clear_focus();
                UiAction::None
            }
            KeyCommand::OpenFocusedDetail => UiAction::OpenFocusedDetail,
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
            KeyCommand::CompactionAuto => UiAction::CompactionAuto,
            KeyCommand::CompactionAutoSchedule => UiAction::CompactionAutoSchedule,
            KeyCommand::CompactionCutPoints => UiAction::CompactionCutPoints,
            KeyCommand::CompactionStatus => UiAction::CompactionStatus,
            KeyCommand::ProviderCursorStatus => UiAction::ProviderCursorStatus,
            KeyCommand::ProviderCursorRotate => UiAction::ProviderCursorRotate,
            KeyCommand::ContextSelectionStatus => UiAction::ContextSelectionStatus,
            KeyCommand::ScrollCanvasUp => UiAction::ScrollCanvasUp,
            KeyCommand::ScrollCanvasDown => UiAction::ScrollCanvasDown,
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
        // Alt-Enter / Shift-Enter inserts a newline (Part 7: "multi-line
        // with ⇧⏎ newline"). Alt- is more reliable across terminals than
        // Shift-Enter, which many terminals don't distinguish from Enter;
        // accepting both is harmless and matches the keylight's advertised
        // `⇧⏎ newline` affordance.
        KeyCode::Enter
            if key
                .modifiers
                .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT) =>
        {
            input.push('\n');
            UiAction::None
        }
        KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Emacs BOL — with a plain String buffer we don't track
            // cursor position, so this is a no-op for now. Left as a
            // stub so the keybinding exists and a future
            // ratatui-textarea swap can wire it up without another
            // touch through this match arm.
            UiAction::None
        }
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Emacs EOL — see `C-a` above.
            UiAction::None
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Emacs kill-bol → clear the whole buffer (no cursor model).
            input.clear();
            UiAction::None
        }
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Emacs kill-word (backwards).
            kill_word_backward(input);
            UiAction::None
        }
        KeyCode::Char(ch) => {
            input.push(ch);
            UiAction::None
        }
        _ => UiAction::None,
    }
}

/// Remove the last whitespace-delimited word (and any trailing
/// whitespace) from `input`. Mirrors the Emacs `C-w` convention.
fn kill_word_backward(input: &mut String) {
    // Strip trailing whitespace first so repeated `C-w` keeps removing
    // words instead of just whitespace.
    while input.ends_with(|c: char| c.is_whitespace()) {
        input.pop();
    }
    while input.ends_with(|c: char| !c.is_whitespace()) {
        input.pop();
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

/// `⏎` on a focused card expands it — but only if there's something to
/// expand onto. `true` when the focused message is a `ToolCard` or
/// `TaskCard`; `false` when it's a plain turn / notice or when focus is
/// empty (in which case submit falls through to its usual path).
fn card_expand_target(state: &TuiState) -> bool {
    use rip_tui::CanvasMessage;
    matches!(
        state.focused_message(),
        Some(CanvasMessage::ToolCard { .. } | CanvasMessage::TaskCard { .. })
    )
}

/// `x` on a focused canvas item opens the per-item detail overlay. For
/// now this routes into the existing `ToolDetail` / `TaskDetail` overlays
/// (scoped to that tool/task id); Phase C.8 replaces them with a proper
/// `XrayOverlay` that takes a `(from_seq, to_seq)` window.
fn focused_detail_overlay(state: &TuiState) -> Option<rip_tui::Overlay> {
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
        terminal.backend_mut().execute(DisableMouseCapture)?;
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
        let _ = execute!(stdout, DisableMouseCapture);
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
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
    use httpmock::Method::GET;
    use httpmock::MockServer;
    use rip_kernel::{EventKind, ProviderEventStatus};
    use rip_tui::palette::modes::models::{default_endpoint_for_provider, parse_model_route};
    use std::ffi::OsString;
    use tokio::time::timeout;

    fn seed_state() -> TuiState {
        let mut state = TuiState::new(100);
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

        // Phase C.8 reassigns `Ctrl-R` from "toggle raw global view"
        // to "open X-ray on focused item". The X-ray overlay is a
        // per-item drill-down, not a canvas-wide mode swap.
        assert_eq!(state.output_view, rip_tui::OutputViewMode::Rendered);
        let action = handle_key_event(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::OpenFocusedDetail);
        // Global output_view is unchanged — no more mode swap.
        assert_eq!(state.output_view, rip_tui::OutputViewMode::Rendered);

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
        assert_eq!(action, UiAction::OpenSelectedDetail);

        let action = handle_key_event(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
        assert_eq!(action, UiAction::Submit);

        let action = handle_key_event(
            KeyEvent::new(KeyCode::PageUp, KeyModifiers::empty()),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::ScrollCanvasUp);
    }

    #[test]
    fn handle_key_event_routes_palette_input_and_selection() {
        let keymap = Keymap::default();
        let mut state = seed_state();
        state.open_palette(
            rip_tui::PaletteMode::Model,
            vec![
                rip_tui::PaletteEntry {
                    value: "openrouter/openai/gpt-oss-20b".to_string(),
                    title: "openrouter/openai/gpt-oss-20b".to_string(),
                    subtitle: Some("OpenRouter".to_string()),
                    chips: vec!["current".to_string()],
                },
                rip_tui::PaletteEntry {
                    value: "openai/gpt-5-nano-2025-08-07".to_string(),
                    title: "openai/gpt-5-nano-2025-08-07".to_string(),
                    subtitle: Some("OpenAI".to_string()),
                    chips: vec![],
                },
            ],
            "No models",
            true,
            "Use typed route",
        );
        let mut mode = RenderMode::Json;
        let mut input = String::new();

        let action = handle_key_event(
            KeyEvent::new(KeyCode::Char('n'), KeyModifiers::empty()),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::None);
        assert_eq!(state.palette_query(), Some("n"));

        let action = handle_key_event(
            KeyEvent::new(KeyCode::Down, KeyModifiers::empty()),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::None);
        assert_eq!(
            state.palette_selected_value().as_deref(),
            Some("openai/gpt-5-nano-2025-08-07")
        );

        let action = handle_key_event(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::empty()),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::ApplyPalette);

        let action = handle_key_event(
            KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
            &mut state,
            &mut mode,
            &mut input,
            true,
            &keymap,
        );
        assert_eq!(action, UiAction::CloseOverlay);
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
    fn handle_key_event_alt_enter_inserts_newline() {
        // C.4 multi-line input: ⌥⏎ / ⇧⏎ sends a `\n` instead of
        // submitting the turn.
        let keymap = Keymap::default();
        let mut state = seed_state();
        let mut mode = RenderMode::Json;
        let mut input = "first".to_string();

        let action = handle_key_event(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
        assert_eq!(action, UiAction::None);
        assert_eq!(input, "first\n");

        let action = handle_key_event(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
        assert_eq!(action, UiAction::None);
        assert_eq!(input, "first\n\n");
    }

    #[test]
    fn handle_key_event_emacs_kill_bindings_trim_the_buffer() {
        let keymap = Keymap::default();
        let mut state = seed_state();
        let mut mode = RenderMode::Json;
        let mut input = "hello world ".to_string();

        // C-w kills the trailing word + any trailing whitespace.
        handle_key_event(
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
        assert_eq!(input, "hello ");

        // C-u clears the whole buffer.
        handle_key_event(
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
        assert_eq!(input, "");
    }

    #[test]
    fn kill_word_backward_handles_whitespace_and_words() {
        let mut buf = "  foo".to_string();
        kill_word_backward(&mut buf);
        assert_eq!(buf, "  ");
        let mut buf = "foo bar  ".to_string();
        kill_word_backward(&mut buf);
        assert_eq!(buf, "foo ");
        let mut buf = String::new();
        kill_word_backward(&mut buf);
        assert_eq!(buf, "");
    }

    #[test]
    fn apply_model_palette_selection_updates_overrides_and_preferred_target() {
        let mut state = seed_state();
        state.open_palette(
            rip_tui::PaletteMode::Model,
            vec![rip_tui::PaletteEntry {
                value: "openrouter/openai/gpt-oss-20b".to_string(),
                title: "openrouter/openai/gpt-oss-20b".to_string(),
                subtitle: None,
                chips: vec![],
            }],
            "No models",
            true,
            "Use typed route",
        );

        let mut catalog = ModelsMode::new(
            vec![ModelRoute {
                route: "openrouter/openai/gpt-oss-20b".to_string(),
                provider_id: "openrouter".to_string(),
                model_id: "openai/gpt-oss-20b".to_string(),
                endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
                label: None,
                variants: 0,
                sources: vec!["catalog".to_string()],
            }],
            BTreeMap::from([(
                "openrouter".to_string(),
                "https://openrouter.ai/api/v1/responses".to_string(),
            )]),
            None,
            None,
            None,
        );
        let mut overrides = Some(serde_json::json!({
            "parallel_tool_calls": true
        }));

        apply_model_palette_selection(&mut state, &mut overrides, &mut catalog).expect("apply");

        assert_eq!(state.palette_query(), None);
        assert_eq!(
            state.preferred_openresponses_endpoint.as_deref(),
            Some("https://openrouter.ai/api/v1/responses")
        );
        assert_eq!(
            state.preferred_openresponses_model.as_deref(),
            Some("openai/gpt-oss-20b")
        );
        assert_eq!(
            overrides,
            Some(serde_json::json!({
                "parallel_tool_calls": true,
                "endpoint": "https://openrouter.ai/api/v1/responses",
                "model": "openai/gpt-oss-20b"
            }))
        );
        assert_eq!(
            catalog.current_route.as_deref(),
            Some("openrouter/openai/gpt-oss-20b")
        );
        assert_eq!(
            catalog.current_endpoint.as_deref(),
            Some("https://openrouter.ai/api/v1/responses")
        );
        assert_eq!(catalog.current_model.as_deref(), Some("openai/gpt-oss-20b"));
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
    fn handle_term_event_routes_mouse_scroll() {
        let keymap = Keymap::default();
        let mut state = seed_state();
        let mut mode = RenderMode::Json;
        let mut input = String::new();
        let action = handle_term_event(
            TermEvent::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::empty(),
            }),
            &mut state,
            &mut mode,
            &mut input,
            false,
            &keymap,
        );
        assert_eq!(action, UiAction::ScrollCanvasUp);
    }

    #[test]
    fn handle_key_event_toggles_follow_and_palette_cycle_is_noop_outside_palette() {
        // Phase C.5 retires Tab's legacy "details-mode toggle" role
        // and reassigns Tab to `PaletteCycleMode`. Outside of an open
        // palette, Tab is a no-op.
        //
        // `Alt+T` no longer toggles the theme — theme switching is a
        // palette action (Options mode). Users who want the legacy
        // binding back can re-add `"M-t": "ToggleTheme"` in
        // `~/.rip/keybindings.json`.
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
        assert_eq!(mode, RenderMode::Json);

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
    }

    #[test]
    fn palette_hotkeys_dispatch_to_correct_ui_actions() {
        // The four new palette openers bound in Phase C.5 all have
        // direct `UiAction::OpenPalette…` arms. We exercise each one
        // to make sure the keymap→UiAction glue didn't regress to
        // generic `TogglePalette` routing.
        let keymap = Keymap::default();
        let mut state = seed_state();
        let mut mode = RenderMode::Json;
        let mut input = String::new();

        let k = |ch: char, mods: KeyModifiers| KeyEvent::new(KeyCode::Char(ch), mods);
        assert_eq!(
            handle_key_event(
                k('k', KeyModifiers::CONTROL),
                &mut state,
                &mut mode,
                &mut input,
                true,
                &keymap,
            ),
            UiAction::TogglePalette
        );
        assert_eq!(
            handle_key_event(
                k('g', KeyModifiers::CONTROL),
                &mut state,
                &mut mode,
                &mut input,
                true,
                &keymap,
            ),
            UiAction::OpenPaletteGoTo
        );
        assert_eq!(
            handle_key_event(
                k('m', KeyModifiers::ALT),
                &mut state,
                &mut mode,
                &mut input,
                true,
                &keymap,
            ),
            UiAction::OpenPaletteModels
        );
        assert_eq!(
            handle_key_event(
                k('o', KeyModifiers::ALT),
                &mut state,
                &mut mode,
                &mut input,
                true,
                &keymap,
            ),
            UiAction::OpenPaletteOptions
        );
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
    fn load_model_palette_catalog_reads_config_and_current_override() {
        let _lock = test_env::lock_env();
        let temp_root =
            std::env::temp_dir().join(format!("rip_model_palette_test_{}", std::process::id()));
        let workspace_dir = temp_root.join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");
        std::fs::create_dir_all(&temp_root).expect("config dir");
        std::fs::write(
            temp_root.join("config.jsonc"),
            r#"{
  "provider": {
    "openrouter": {
      "endpoint": "https://openrouter.ai/api/v1/responses",
      "models": {
        "openai/gpt-oss-20b": { "label": "OSS 20B" }
      }
    },
    "openai": {
      "endpoint": "https://api.openai.com/v1/responses",
      "models": {
        "gpt-5-nano-2025-08-07": { "label": "GPT-5 Nano" }
      }
    }
  },
  "model": "openrouter/openai/gpt-oss-20b"
}"#,
        )
        .expect("config");

        let _config_home = set_env("RIP_CONFIG_HOME", temp_root.as_os_str());
        let _workspace = set_env("RIP_WORKSPACE_ROOT", workspace_dir.as_os_str());
        let _clear_custom = remove_env("RIP_CONFIG");
        let _clear_endpoint = remove_env("RIP_OPENRESPONSES_ENDPOINT");
        let _clear_model = remove_env("RIP_OPENRESPONSES_MODEL");
        let _clear_stateful = remove_env("RIP_OPENRESPONSES_STATELESS_HISTORY");
        let _clear_parallel = remove_env("RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS");
        let _clear_followup = remove_env("RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE");

        let overrides = serde_json::json!({
            "endpoint": "https://openrouter.ai/api/v1/responses",
            "model": "nvidia/nemotron-3-nano-30b-a3b:free"
        });
        let catalog = load_model_palette_catalog(Some(&overrides));

        assert_eq!(
            catalog.current_route.as_deref(),
            Some("openrouter/nvidia/nemotron-3-nano-30b-a3b:free")
        );
        let values = catalog
            .entries()
            .into_iter()
            .map(|entry| entry.value)
            .collect::<Vec<_>>();
        assert!(values.contains(&"openrouter/openai/gpt-oss-20b".to_string()));
        assert!(values.contains(&"openai/gpt-5-nano-2025-08-07".to_string()));
        assert!(values.contains(&"openrouter/nvidia/nemotron-3-nano-30b-a3b:free".to_string()));
    }

    #[test]
    fn model_palette_helper_functions_cover_override_paths() {
        assert_eq!(
            parse_model_route(" openrouter / model-x "),
            Some(("openrouter".to_string(), "model-x".to_string()))
        );
        assert_eq!(parse_model_route("openrouter"), None);
        assert_eq!(
            default_endpoint_for_provider("openrouter").as_deref(),
            Some("https://openrouter.ai/api/v1/responses")
        );
        assert_eq!(default_endpoint_for_provider("missing"), None);
        assert_eq!(
            infer_provider_id_from_endpoint("https://openrouter.ai/api/v1/responses").as_deref(),
            Some("openrouter")
        );
        assert_eq!(
            infer_provider_id_from_endpoint("https://api.openai.com/v1/responses").as_deref(),
            Some("openai")
        );
        assert_eq!(infer_provider_id_from_endpoint("https://example.com"), None);

        let overrides = openresponses_override_input_from_json(Some(&serde_json::json!({
            "endpoint": "https://openrouter.ai/api/v1/responses",
            "model": "openai/gpt-oss-20b",
            "stateless_history": true,
            "parallel_tool_calls": false,
            "followup_user_message": "keep going"
        })));
        assert_eq!(
            overrides.endpoint.as_deref(),
            Some("https://openrouter.ai/api/v1/responses")
        );
        assert_eq!(overrides.model.as_deref(), Some("openai/gpt-oss-20b"));
        assert_eq!(overrides.stateless_history, Some(true));
        assert_eq!(overrides.parallel_tool_calls, Some(false));
        assert_eq!(
            overrides.followup_user_message.as_deref(),
            Some("keep going")
        );

        let mut routes = BTreeMap::new();
        let provider_endpoints = BTreeMap::from([(
            "openrouter".to_string(),
            "https://openrouter.ai/api/v1/responses".to_string(),
        )]);
        push_route_from_string(
            &mut routes,
            &provider_endpoints,
            "openrouter/openai/gpt-oss-20b",
            "config:model",
        );
        upsert_model_route(
            &mut routes,
            "openrouter",
            "openai/gpt-oss-20b",
            "https://openrouter.ai/api/v1/responses",
            Some("OSS 20B".to_string()),
            3,
            "config:roles.primary",
        );
        let record = routes
            .get("openrouter/openai/gpt-oss-20b")
            .expect("route present");
        assert_eq!(record.label.as_deref(), Some("OSS 20B"));
        assert_eq!(record.variants, 3);
        assert!(record.sources.iter().any(|source| source == "config:model"));
        assert!(record
            .sources
            .iter()
            .any(|source| source == "config:roles.primary"));
    }

    #[test]
    fn open_model_palette_uses_catalog_entries_and_mouse_scroll_moves_selection() {
        let mut state = seed_state();
        let catalog = ModelsMode::new(
            vec![
                ModelRoute {
                    route: "openrouter/openai/gpt-oss-20b".to_string(),
                    provider_id: "openrouter".to_string(),
                    model_id: "openai/gpt-oss-20b".to_string(),
                    endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
                    label: Some("OSS 20B".to_string()),
                    variants: 0,
                    sources: vec!["catalog".to_string()],
                },
                ModelRoute {
                    route: "openai/gpt-5-nano".to_string(),
                    provider_id: "openai".to_string(),
                    model_id: "gpt-5-nano".to_string(),
                    endpoint: "https://api.openai.com/v1/responses".to_string(),
                    label: Some("GPT-5 Nano".to_string()),
                    variants: 0,
                    sources: vec!["catalog".to_string()],
                },
            ],
            BTreeMap::new(),
            Some("openrouter/openai/gpt-oss-20b".to_string()),
            Some("https://openrouter.ai/api/v1/responses".to_string()),
            Some("openai/gpt-oss-20b".to_string()),
        );

        open_model_palette(&mut state, &catalog);
        assert!(state.is_palette_open());
        assert_eq!(
            state.palette_selected_value().as_deref(),
            Some("openrouter/openai/gpt-oss-20b")
        );

        let action = handle_mouse_event(
            MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 0,
                row: 0,
                modifiers: KeyModifiers::empty(),
            },
            &mut state,
        );
        assert_eq!(action, UiAction::None);
        assert_eq!(
            state.palette_selected_value().as_deref(),
            Some("openai/gpt-5-nano")
        );
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
