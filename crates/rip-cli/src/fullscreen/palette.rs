//! Palette coordination: catalog + open-mode dispatch + apply-selection.
//!
//! Keeps the driver's palette wiring in one place so fullscreen.rs can
//! focus on the run loop. The five shipped modes (Command, Model,
//! Navigation/"Go To", Session/"Threads", Option) each get an opener;
//! `cycle_palette_mode` rotates between them on `Tab`; `apply_palette_selection`
//! routes the chosen entry through the right dispatcher.

use std::collections::BTreeMap;

use rip_tui::palette::modes::models::{
    infer_provider_id_from_endpoint, push_route_from_string, upsert_model_route,
};
use rip_tui::{
    ModelRoute, ModelsMode, PaletteMode, PaletteOrigin, PaletteSource, ResolvedModelRoute, TuiState,
};
use serde_json::Value;

pub(super) fn load_model_palette_catalog(openresponses_overrides: Option<&Value>) -> ModelsMode {
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

pub(super) fn openresponses_override_input_from_json(
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
        reasoning: obj
            .get("reasoning")
            .and_then(|value| value.as_object())
            .and_then(|reasoning| {
                let effort = reasoning
                    .get("effort")
                    .and_then(|value| value.as_str())
                    .and_then(|value| ripd::parse_reasoning_effort(value).ok());
                let summary = reasoning
                    .get("summary")
                    .and_then(|value| value.as_str())
                    .and_then(|value| ripd::parse_reasoning_summary(value).ok());
                ripd::OpenResponsesReasoningConfig { effort, summary }.normalized()
            }),
    }
}

pub(super) fn open_model_palette(
    state: &mut TuiState,
    catalog: &ModelsMode,
    origin: PaletteOrigin,
) {
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
pub(super) fn open_command_palette(state: &mut TuiState, origin: PaletteOrigin) {
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
pub(super) fn open_go_to_palette(state: &mut TuiState, origin: PaletteOrigin) {
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
pub(super) fn open_threads_palette(state: &mut TuiState, origin: PaletteOrigin) {
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
pub(super) fn open_options_palette(state: &mut TuiState, origin: PaletteOrigin) {
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
pub(super) fn cycle_palette_mode(state: &mut TuiState, catalog: &ModelsMode) {
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
pub(super) fn apply_palette_selection(
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
pub(super) fn apply_command_action(
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
            if let Some(overlay) = super::focused_detail_overlay(state) {
                state.set_overlay(overlay);
            }
        }
        A::SwitchModel => {
            let origin = state.palette_origin().unwrap_or(PaletteOrigin::TopCenter);
            open_model_palette(state, catalog, origin);
        }
        A::Quit => {
            state.set_status_message("press Ctrl-C to quit".to_string());
        }
        other => {
            state.set_status_message(format!(
                "{}: use the dedicated hotkey or command (palette routing lands in a later phase)",
                other.title()
            ));
        }
    }
}

#[cfg(test)]
mod tests;

pub(super) fn apply_model_palette_selection(
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
