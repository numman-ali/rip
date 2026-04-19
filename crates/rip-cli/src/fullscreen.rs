use std::collections::BTreeMap;
use std::future;
use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crossterm::event::{
    EnableMouseCapture, Event as TermEvent, EventStream, KeyCode, KeyEvent, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::terminal::{enable_raw_mode, size as terminal_size, EnterAlternateScreen};
use crossterm::ExecutableCommand;
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use ratatui_textarea::{Input, TextArea};
use reqwest::Client;
use reqwest_eventsource::{
    Error as EventSourceError, Event as SseEvent, EventSource, RequestBuilderExt,
};
use rip_kernel::Event as FrameEvent;
use rip_tui::palette::modes::models::{
    infer_provider_id_from_endpoint, push_route_from_string, upsert_model_route,
};
use rip_tui::{
    canvas_hit_message_id, hero_click_target, render, HeroClickTarget, ModelRoute, ModelsMode,
    PaletteMode, PaletteOrigin, PaletteSource, RenderMode, ResolvedModelRoute, TuiState,
};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

mod copy;
mod keymap;
mod terminal;
mod theme;

use copy::copy_selected;
use keymap::{Command as KeyCommand, Keymap};
use terminal::TerminalGuard;
use theme::load_theme;

#[cfg(test)]
use copy::{base64_encode, osc52_sequence, prepare_copy_selected, CopySelectedAction};
#[cfg(test)]
use theme::{config_dir, parse_theme, theme_path};

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

fn open_model_palette(state: &mut TuiState, catalog: &ModelsMode, origin: PaletteOrigin) {
    let empty_message = catalog.empty_state().to_string();
    let custom_prompt = catalog.allow_custom().unwrap_or("").to_string();
    state.open_palette(
        PaletteMode::Model,
        origin,
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
fn open_command_palette(state: &mut TuiState, origin: PaletteOrigin) {
    use rip_tui::palette::modes::command::CommandMode;
    let mode = CommandMode::new();
    state.open_palette(
        PaletteMode::Command,
        origin,
        mode.entries(),
        mode.empty_state().to_string(),
        false,
        String::new(),
    );
}

/// C.5: Go To palette — a fuzzy-search over canvas messages.
fn open_go_to_palette(state: &mut TuiState, origin: PaletteOrigin) {
    use rip_tui::palette::modes::go_to::GoToMode;
    let mode = GoToMode::from_canvas(&state.canvas);
    let empty_message = mode.empty_state().to_string();
    let entries = mode.entries();
    state.open_palette(
        PaletteMode::Navigation,
        origin,
        entries,
        empty_message,
        false,
        String::new(),
    );
}

/// C.5: Threads palette — minimal local-runtime form ships only the
/// current thread until the driver wires up `thread.list` seeding.
fn open_threads_palette(state: &mut TuiState, origin: PaletteOrigin) {
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
        origin,
        mode.entries(),
        empty_message,
        false,
        String::new(),
    );
}

/// C.5: Options palette — toggles for UI-local prefs. Reads the
/// current state so each entry's subtitle reflects the active value.
fn open_options_palette(state: &mut TuiState, origin: PaletteOrigin) {
    use rip_tui::palette::modes::options::OptionsMode;
    let mode = OptionsMode {
        current_theme: Some(state.theme.as_str()),
        auto_follow: state.auto_follow,
        reasoning_visible: false,
        vim_input_mode: state.vim_input_mode,
        mouse_capture: true,
        activity_rail_pinned: state.activity_pinned,
    };
    let entries = mode.entries();
    state.open_palette(
        PaletteMode::Option,
        origin,
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
    let origin = state.palette_origin().unwrap_or(PaletteOrigin::TopCenter);
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
        PaletteMode::Command => open_command_palette(state, origin),
        PaletteMode::Model => open_model_palette(state, catalog, origin),
        PaletteMode::Navigation => open_go_to_palette(state, origin),
        PaletteMode::Session => open_threads_palette(state, origin),
        PaletteMode::Option => open_options_palette(state, origin),
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
        A::ToggleVimInputMode => {
            state.vim_input_mode = !state.vim_input_mode;
            // Canonical vim behaviour: entering vim mode drops you into
            // Normal; leaving resets to Insert so the textarea's
            // ambient keymap is what drives the buffer again.
            state.vim_mode = if state.vim_input_mode {
                rip_tui::VimMode::Normal
            } else {
                rip_tui::VimMode::Insert
            };
            state.vim_pending = None;
            state.set_status_message(format!(
                "vim input mode: {}",
                if state.vim_input_mode { "on" } else { "off" }
            ));
        }
        A::ShowDebugInfo => state.set_overlay(rip_tui::Overlay::Debug),
        A::OpenXrayOnFocused => {
            if let Some(overlay) = focused_detail_overlay(state) {
                state.set_overlay(overlay);
            }
        }
        A::SwitchModel => {
            let origin = state.palette_origin().unwrap_or(PaletteOrigin::TopCenter);
            open_model_palette(state, catalog, origin);
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

#[derive(Debug, Deserialize)]
struct ThreadMetaResponse {
    thread_id: String,
    created_at_ms: u64,
    title: Option<String>,
    archived: bool,
}

async fn load_thread_picker_entries(
    client: &Client,
    server: &str,
    current_thread_id: Option<&str>,
) -> Result<Vec<rip_tui::ThreadPickerEntry>, String> {
    let url = format!("{server}/threads");
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| format!("thread list failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("thread list failed: {}", response.status()));
    }

    let mut threads = response
        .json::<Vec<ThreadMetaResponse>>()
        .await
        .map_err(|err| format!("thread list parse failed: {err}"))?;

    if let Some(current_id) = current_thread_id.filter(|id| !id.is_empty()) {
        if !threads.iter().any(|thread| thread.thread_id == current_id) {
            let url = format!("{server}/threads/{current_id}");
            if let Ok(response) = client.get(url).send().await {
                if response.status().is_success() {
                    if let Ok(meta) = response.json::<ThreadMetaResponse>().await {
                        threads.push(meta);
                    }
                }
            }
        }
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);

    threads.sort_by(|a, b| {
        match (
            current_thread_id == Some(a.thread_id.as_str()),
            current_thread_id == Some(b.thread_id.as_str()),
        ) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.created_at_ms.cmp(&a.created_at_ms),
        }
    });

    Ok(threads
        .into_iter()
        .map(|thread| {
            let mut chips = vec![
                format!("age {}", relative_age_chip(now_ms, thread.created_at_ms)),
                "size —".to_string(),
                "actors —".to_string(),
            ];
            if current_thread_id == Some(thread.thread_id.as_str()) {
                chips.insert(0, "current".to_string());
            }
            if thread.archived {
                chips.push("archived".to_string());
            }
            rip_tui::ThreadPickerEntry {
                thread_id: thread.thread_id.clone(),
                title: thread
                    .title
                    .clone()
                    .unwrap_or_else(|| short_thread_label(&thread.thread_id)),
                preview: "preview —".to_string(),
                chips,
            }
        })
        .collect())
}

fn short_thread_label(thread_id: &str) -> String {
    if thread_id.chars().count() <= 20 {
        return thread_id.to_string();
    }
    let tail: String = thread_id.chars().rev().take(12).collect();
    let tail: String = tail.chars().rev().collect();
    format!("…{tail}")
}

fn relative_age_chip(now_ms: u64, created_at_ms: u64) -> String {
    let age_ms = now_ms.saturating_sub(created_at_ms);
    let minute = 60_000;
    let hour = 60 * minute;
    let day = 24 * hour;
    if age_ms >= day {
        format!("{}d", age_ms / day)
    } else if age_ms >= hour {
        format!("{}h", age_ms / hour)
    } else if age_ms >= minute {
        format!("{}m", age_ms / minute)
    } else {
        "now".to_string()
    }
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
    ApplyThreadPicker,
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
    /// C.10 error-recovery actions. Routed through capabilities —
    /// none of them reach disk or the event log directly.
    /// `r` re-posts the last user message (kernel spawns the retry
    /// run per the capability contract).
    ErrorRecoveryRetry,
    /// `c` rotates the provider cursor.
    ErrorRecoveryRotateCursor,
    /// `m` opens the Models palette so the operator can switch
    /// before retrying.
    ErrorRecoverySwitchModel,
    /// `x` opens the X-ray window scoped to this error's seq (for
    /// now it routes into the existing `ErrorDetail` overlay —
    /// a Phase D follow-up widens it to a proper `XrayOverlay`).
    ErrorRecoveryXray,
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

fn handle_term_event(
    event: TermEvent,
    state: &mut TuiState,
    mode: &mut RenderMode,
    input: &mut TextArea<'static>,
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
        return UiAction::None;
    }

    if state.is_thread_picker_open() {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                state.thread_picker_move_selection(-1);
                return UiAction::None;
            }
            MouseEventKind::ScrollDown => {
                state.thread_picker_move_selection(1);
                return UiAction::None;
            }
            _ => {}
        }
        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            return UiAction::ApplyThreadPicker;
        }
        return UiAction::None;
    }

    let (width, height) = match terminal_size() {
        Ok(size) => size,
        Err(_) => return UiAction::None,
    };

    if mouse.row == 0 && matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
        return match hero_click_target(state, width, mouse.column) {
            Some(HeroClickTarget::Thread) => UiAction::OpenPaletteThreads,
            Some(HeroClickTarget::Agent) => UiAction::TogglePalette,
            Some(HeroClickTarget::Model) => UiAction::OpenPaletteModels,
            None => UiAction::None,
        };
    }

    if mouse_hits_activity_surface(state, width, height, mouse.column, mouse.row) {
        return match mouse.kind {
            MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown => {
                state.set_overlay(rip_tui::Overlay::Activity);
                UiAction::None
            }
            _ => UiAction::None,
        };
    }

    if matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
    ) {
        if let Some((viewport_width, viewport_height, row_in_canvas)) =
            mouse_canvas_hit_geometry(state, width, height, mouse.column, mouse.row)
        {
            if let Some(message_id) =
                canvas_hit_message_id(state, viewport_width, viewport_height, row_in_canvas)
            {
                state.focused_message_id = Some(message_id);
                state.auto_follow = false;
            }
            return UiAction::None;
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

fn mouse_hits_activity_surface(
    state: &TuiState,
    width: u16,
    height: u16,
    column: u16,
    row: u16,
) -> bool {
    if state.activity_pinned && width >= 100 {
        let rail_width = 32u16;
        let rail_start = width.saturating_sub(rail_width);
        if column >= rail_start && row > 0 && row < height.saturating_sub(2) {
            return true;
        }
    }

    let Some(activity_row) = mouse_footer_activity_row(height) else {
        return false;
    };
    row == activity_row
}

fn mouse_footer_activity_row(height: u16) -> Option<u16> {
    (height >= 4).then_some(height.saturating_sub(3))
}

fn mouse_canvas_hit_geometry(
    state: &TuiState,
    width: u16,
    height: u16,
    column: u16,
    row: u16,
) -> Option<(u16, u16, u16)> {
    let body_top = 1u16;
    let bottom_reserved = 3u16;
    let body_height = height.saturating_sub(body_top + bottom_reserved);
    if body_height == 0 || row < body_top || row >= body_top.saturating_add(body_height) {
        return None;
    }

    let viewport_width = if state.activity_pinned && width >= 100 {
        let canvas_width = width.saturating_sub(32);
        if column >= canvas_width {
            return None;
        }
        canvas_width
    } else {
        width
    };

    Some((viewport_width, body_height, row.saturating_sub(body_top)))
}

fn handle_key_event(
    key: KeyEvent,
    state: &mut TuiState,
    mode: &mut RenderMode,
    input: &mut TextArea<'static>,
    session_running: bool,
    keymap: &Keymap,
) -> UiAction {
    // C.10: ErrorRecovery owns the key stream while it's on top of
    // the overlay stack. `r/c/m/x` dispatch to capabilities; `⎋`
    // dismisses. We intercept here so recovery actions don't have
    // to be bound globally in the keymap.
    if let rip_tui::Overlay::ErrorRecovery { .. } = state.overlay() {
        return match key.code {
            KeyCode::Char('r') => UiAction::ErrorRecoveryRetry,
            KeyCode::Char('c') => UiAction::ErrorRecoveryRotateCursor,
            KeyCode::Char('m') => UiAction::ErrorRecoverySwitchModel,
            KeyCode::Char('x') => UiAction::ErrorRecoveryXray,
            KeyCode::Esc => UiAction::CloseOverlay,
            _ => UiAction::None,
        };
    }

    if state.is_thread_picker_open() {
        if let Some(cmd) = keymap.command_for(key) {
            return match cmd {
                KeyCommand::Quit => UiAction::Quit,
                KeyCommand::Submit => UiAction::ApplyThreadPicker,
                KeyCommand::CloseOverlay | KeyCommand::TogglePalette => UiAction::CloseOverlay,
                KeyCommand::SelectPrev => {
                    state.thread_picker_move_selection(-1);
                    UiAction::None
                }
                KeyCommand::SelectNext => {
                    state.thread_picker_move_selection(1);
                    UiAction::None
                }
                KeyCommand::ScrollCanvasUp => {
                    state.thread_picker_move_selection(-5);
                    UiAction::None
                }
                KeyCommand::ScrollCanvasDown => {
                    state.thread_picker_move_selection(5);
                    UiAction::None
                }
                _ => UiAction::None,
            };
        }
        return UiAction::None;
    }

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

    // D.5: vim layer gets first refusal on non-overlay keys, but only
    // when the session isn't streaming and no palette / overlay has
    // already claimed the input. Normal mode fully owns plain-keyed
    // input (letters, motions, Esc-as-no-op); Insert mode only takes
    // Esc so the textarea's emacs-ish bindings still work for typing.
    // We intercept BEFORE the global keymap consult so vim's Esc isn't
    // eaten by the keymap's default `Esc → CloseOverlay` binding, and
    // so Normal-mode letter keys can't fall through to bindings like
    // `x = ToggleOutputView` that would otherwise fire.
    if state.vim_input_mode && !session_running {
        if let Some(action) = try_vim_intercept(key, state, input) {
            return action;
        }
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
                if buffer_is_effectively_empty(input) && card_expand_target(state) {
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

    // Alt-Enter / Shift-Enter inserts a newline (Part 7: "multi-line
    // with ⇧⏎ newline"). Alt is more reliable across terminals than
    // Shift-Enter, which many terminals don't distinguish from Enter;
    // accepting both is harmless and matches the keylight's advertised
    // `⇧⏎ newline` affordance. The textarea's own `Enter` handler
    // inserts a newline only when `input.input(...)` receives bare
    // Enter, so we intercept the modifier combo and splice manually —
    // bare Enter stays bound to `UiAction::Submit` via the keymap.
    if key.code == KeyCode::Enter
        && key
            .modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::SHIFT)
    {
        input.insert_newline();
        return UiAction::None;
    }

    // Everything else the editor needs — Backspace, arrow keys,
    // Home/End, Ctrl-A/E (BOL/EOL), Ctrl-U (kill-bol), Ctrl-W
    // (kill-word), Char insertion — is already implemented by
    // ratatui-textarea. Rather than re-implementing cursor math over a
    // `String`, we hand the event off via `Input::from(key)` and let
    // the textarea drive its own buffer + cursor + undo history.
    let _ = input.input(Input::from(key));
    UiAction::None
}

/// D.5: decides whether the vim layer owns a given key press. Returns
/// `Some` when the vim dispatcher has consumed it, `None` to let the
/// global keymap + textarea pipe continue. Normal mode claims all
/// non-Ctrl keys (letters, motions, Esc-as-no-op). Insert mode only
/// claims Esc so the ambient emacs-ish textarea bindings keep working
/// for actual typing, which is the whole point of making Insert mode
/// "the textarea's native mode" rather than an alternate keymap.
fn try_vim_intercept(
    key: KeyEvent,
    state: &mut TuiState,
    input: &mut TextArea<'static>,
) -> Option<UiAction> {
    match state.vim_mode {
        rip_tui::VimMode::Insert => {
            if key.code == KeyCode::Esc && key.modifiers.is_empty() {
                state.vim_mode = rip_tui::VimMode::Normal;
                state.vim_pending = None;
                return Some(UiAction::None);
            }
            None
        }
        rip_tui::VimMode::Normal => {
            // Ctrl-modified keys remain available to the outer keymap
            // so Ctrl-C / Ctrl-K / etc. keep working in Normal mode.
            // Shift is allowed through — `A`, `I`, `O`, `G`, `$` all
            // need it, and vim treats shifted letters as first-class
            // operators rather than as chord prefixes.
            if key.modifiers.contains(KeyModifiers::CONTROL)
                || key.modifiers.contains(KeyModifiers::ALT)
            {
                return None;
            }
            match key.code {
                KeyCode::Char(_)
                | KeyCode::Esc
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::Backspace => Some(handle_vim_normal_key(key, state, input)),
                // Enter / Tab / function keys / everything else stays
                // on the global keymap path — vim's own `:` command-
                // line handling is out of scope, and Submit should
                // still feel like Submit.
                _ => None,
            }
        }
    }
}

/// D.5: dispatcher for vim Normal-mode keys. Covers the subset named
/// in the revamp plan (Esc / i / a / o / dd / yy / p / gg / G) plus
/// enough cursor motion (h/j/k/l, w/b, 0/$) and edit primitives (x, A,
/// I, O) to make Normal mode actually usable. Anything we don't
/// implement is silently swallowed rather than handed to the textarea
/// — that way the user can't accidentally type text into the buffer
/// while they think they're in Normal. The `vim_pending` field on
/// `TuiState` tracks the waiting-for-second-char state for `dd`, `yy`,
/// and `gg`; every path through this function must either set it or
/// clear it so a stale prefix can't survive a completed action.
fn handle_vim_normal_key(
    key: KeyEvent,
    state: &mut TuiState,
    input: &mut TextArea<'static>,
) -> UiAction {
    use ratatui_textarea::CursorMove;

    let pending = state.vim_pending.take();

    let ch = match key.code {
        KeyCode::Char(c) => c,
        KeyCode::Esc => {
            state.vim_pending = None;
            return UiAction::None;
        }
        KeyCode::Up => {
            input.move_cursor(CursorMove::Up);
            return UiAction::None;
        }
        KeyCode::Down => {
            input.move_cursor(CursorMove::Down);
            return UiAction::None;
        }
        KeyCode::Left => {
            input.move_cursor(CursorMove::Back);
            return UiAction::None;
        }
        KeyCode::Right => {
            input.move_cursor(CursorMove::Forward);
            return UiAction::None;
        }
        KeyCode::Home => {
            input.move_cursor(CursorMove::Head);
            return UiAction::None;
        }
        KeyCode::End => {
            input.move_cursor(CursorMove::End);
            return UiAction::None;
        }
        KeyCode::Backspace => {
            // Vim's Backspace in Normal mode is "move cursor left" —
            // it never deletes. This matches the textarea only after
            // we opt out of `Input::from(key)`'s delete behaviour.
            input.move_cursor(CursorMove::Back);
            return UiAction::None;
        }
        _ => return UiAction::None,
    };

    if let Some(prefix) = pending {
        match (prefix, ch) {
            ('d', 'd') => {
                input.move_cursor(CursorMove::Head);
                input.start_selection();
                input.move_cursor(CursorMove::End);
                let _ = input.cut();
                // Leave the now-empty line behind so `p` pastes on the
                // blank line — matches Vim's `dd` leaving a blank when
                // it's the only line in the buffer. Multi-line buffers
                // get the followup newline swallowed too so the cursor
                // lands on the next logical line.
                input.delete_next_char();
                return UiAction::None;
            }
            ('y', 'y') => {
                input.start_selection();
                input.move_cursor(CursorMove::Head);
                input.start_selection();
                input.move_cursor(CursorMove::End);
                input.copy();
                input.cancel_selection();
                return UiAction::None;
            }
            ('g', 'g') => {
                input.move_cursor(CursorMove::Top);
                return UiAction::None;
            }
            _ => {
                // Unmatched follow-up: fall through so `ch` is
                // interpreted as a fresh Normal-mode key rather than
                // the second half of an operator.
            }
        }
    }

    match ch {
        'i' => state.vim_mode = rip_tui::VimMode::Insert,
        'a' => {
            input.move_cursor(CursorMove::Forward);
            state.vim_mode = rip_tui::VimMode::Insert;
        }
        'I' => {
            input.move_cursor(CursorMove::Head);
            state.vim_mode = rip_tui::VimMode::Insert;
        }
        'A' => {
            input.move_cursor(CursorMove::End);
            state.vim_mode = rip_tui::VimMode::Insert;
        }
        'o' => {
            input.move_cursor(CursorMove::End);
            input.insert_newline();
            state.vim_mode = rip_tui::VimMode::Insert;
        }
        'O' => {
            input.move_cursor(CursorMove::Head);
            input.insert_newline();
            input.move_cursor(CursorMove::Up);
            state.vim_mode = rip_tui::VimMode::Insert;
        }
        'h' => input.move_cursor(CursorMove::Back),
        'l' => input.move_cursor(CursorMove::Forward),
        'j' => input.move_cursor(CursorMove::Down),
        'k' => input.move_cursor(CursorMove::Up),
        'w' => input.move_cursor(CursorMove::WordForward),
        'b' => input.move_cursor(CursorMove::WordBack),
        'e' => input.move_cursor(CursorMove::WordEnd),
        '0' => input.move_cursor(CursorMove::Head),
        '$' => input.move_cursor(CursorMove::End),
        'G' => input.move_cursor(CursorMove::Bottom),
        'x' => {
            input.delete_next_char();
        }
        'p' => {
            input.paste();
        }
        'u' => {
            input.undo();
        }
        'd' | 'y' | 'g' => {
            state.vim_pending = Some(ch);
        }
        _ => {}
    }
    UiAction::None
}

/// Whitespace-only buffer counts as empty for submit / expand-card
/// gating — matches the keylight / placeholder rule in the renderer.
/// Using `TextArea::is_empty` alone would flip to "typing" as soon as
/// the user pressed space, which would swap the keylight mid-pause
/// and let Enter submit an all-whitespace prompt.
fn buffer_is_effectively_empty(input: &TextArea<'_>) -> bool {
    input.lines().iter().all(|line| line.trim().is_empty())
}

/// Flatten the textarea's lines back into a `\n`-joined prompt and
/// trim surrounding whitespace. Used whenever we need the user's
/// typed input as a single `String` (sending to the kernel, copying,
/// etc.).
fn buffer_trimmed_prompt(input: &TextArea<'_>) -> String {
    input.lines().join("\n").trim().to_string()
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

/// C.10 — dig out the last user message's plain text from the canvas.
/// Used by `ErrorRecoveryRetry` to re-post the turn that triggered
/// the error. Returns `None` when the canvas has no UserTurn to
/// replay (fresh thread, or the user hasn't submitted anything yet).
fn last_user_prompt(state: &TuiState) -> Option<String> {
    use rip_tui::canvas::{Block, CanvasMessage};
    let user_turn = state
        .canvas
        .messages
        .iter()
        .rev()
        .find_map(|msg| match msg {
            CanvasMessage::UserTurn { blocks, .. } => Some(blocks),
            _ => None,
        })?;
    for block in user_turn {
        let text = match block {
            Block::Paragraph(t)
            | Block::Markdown(t)
            | Block::Heading { text: t, .. }
            | Block::CodeFence { text: t, .. } => t,
            _ => continue,
        };
        let mut out = String::new();
        for (idx, line) in text.text.lines.iter().enumerate() {
            if idx > 0 {
                out.push('\n');
            }
            for span in &line.spans {
                out.push_str(&span.content);
            }
        }
        if !out.trim().is_empty() {
            return Some(out);
        }
    }
    None
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

#[cfg(test)]
mod tests;
