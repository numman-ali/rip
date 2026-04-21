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
    ModelRoute, ModelsMode, PaletteEntry, PaletteMode, PaletteOrigin, PaletteSource,
    ResolvedModelRoute, TuiState,
};
use serde_json::Value;

const OPTION_INCLUDE_PREFIX: &str = "options.include.";
const ALL_RESPONSE_INCLUDE_OPTIONS: &[(ripd::OpenResponsesInclude, &str)] = &[
    (
        ripd::OpenResponsesInclude::ReasoningEncryptedContent,
        "Include reasoning.encrypted_content",
    ),
    (
        ripd::OpenResponsesInclude::CodeInterpreterCallOutputs,
        "Include code_interpreter_call.outputs",
    ),
    (
        ripd::OpenResponsesInclude::FileSearchCallResults,
        "Include file_search_call.results",
    ),
    (
        ripd::OpenResponsesInclude::MessageInputImageImageUrl,
        "Include message.input_image.image_url",
    ),
    (
        ripd::OpenResponsesInclude::ComputerCallOutputOutputImageUrl,
        "Include computer_call_output.output.image_url",
    ),
    (
        ripd::OpenResponsesInclude::WebSearchCallResults,
        "Include web_search_call.results",
    ),
    (
        ripd::OpenResponsesInclude::WebSearchCallActionSources,
        "Include web_search_call.action.sources",
    ),
    (
        ripd::OpenResponsesInclude::MessageOutputTextLogprobs,
        "Include message.output_text.logprobs",
    ),
];

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
        include: obj
            .get("include")
            .and_then(|value| value.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .filter_map(|value| ripd::parse_openresponses_include(value).ok())
                    .fold(Vec::new(), |mut acc, value| {
                        if !acc.contains(&value) {
                            acc.push(value);
                        }
                        acc
                    })
            }),
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
#[cfg(test)]
pub(super) fn open_options_palette(state: &mut TuiState, origin: PaletteOrigin) {
    open_options_palette_with_overrides(state, None, origin);
}

pub(super) fn open_options_palette_with_overrides(
    state: &mut TuiState,
    overrides: Option<&Value>,
    origin: PaletteOrigin,
) {
    use rip_tui::palette::modes::options::OptionsMode;
    let resolved = resolve_openresponses_runtime_config(overrides);
    let reasoning = resolve_runtime_reasoning_state(resolved.as_ref(), overrides);
    let include = resolve_runtime_include_state(resolved.as_ref(), overrides);
    let mode = OptionsMode {
        current_theme: Some(state.theme.as_str()),
        auto_follow: state.auto_follow,
        reasoning_visible: state.reasoning_visible,
        reasoning_effort: reasoning_effort_state_label(&reasoning),
        reasoning_summary: reasoning_summary_state_label(&reasoning),
        vim_input_mode: state.vim_input_mode,
        mouse_capture: true,
        activity_rail_pinned: state.activity_pinned,
        extra_entries: build_include_option_entries(&include),
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
#[cfg(test)]
pub(super) fn cycle_palette_mode(state: &mut TuiState, catalog: &ModelsMode) {
    cycle_palette_mode_with_overrides(state, catalog, None);
}

pub(super) fn cycle_palette_mode_with_overrides(
    state: &mut TuiState,
    catalog: &ModelsMode,
    overrides: Option<&Value>,
) {
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
        PaletteMode::Option => open_options_palette_with_overrides(state, overrides, origin),
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
            if overlay.mode == PaletteMode::Option {
                if let Some(include) = parse_include_option_value(&value) {
                    toggle_response_include_with_overrides(state, overrides, catalog, include);
                    state.close_overlay();
                    return Ok(());
                }
            }
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
            apply_command_action_with_overrides(action, state, overrides, catalog);
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
#[cfg(test)]
pub(super) fn apply_command_action(
    action: rip_tui::palette::modes::command::CommandAction,
    state: &mut TuiState,
    catalog: &ModelsMode,
) {
    let mut overrides = None;
    apply_command_action_with_overrides(action, state, &mut overrides, catalog);
}

pub(super) fn apply_command_action_with_overrides(
    action: rip_tui::palette::modes::command::CommandAction,
    state: &mut TuiState,
    overrides: &mut Option<Value>,
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
        A::ToggleReasoningVisibility => {
            state.toggle_reasoning_visibility();
            state.set_status_message(format!(
                "reasoning visibility: {}",
                if state.reasoning_visible { "on" } else { "off" }
            ));
        }
        A::CycleReasoningEffort => {
            let resolved = resolve_openresponses_runtime_config(overrides.as_ref());
            let reasoning = resolve_runtime_reasoning_state(resolved.as_ref(), overrides.as_ref());
            let current = reasoning.effective.as_ref().and_then(|cfg| cfg.effort);
            let next = next_reasoning_effort(current, &reasoning.support.supported_efforts);
            set_reasoning_effort_override(overrides, next);
            sync_preferred_openresponses_state(state, overrides.as_ref(), catalog);
            state.set_status_message(format!(
                "next reasoning effort: {}{}",
                reasoning_effort_label(next),
                reasoning_support_suffix(
                    reasoning.support.effort,
                    &reasoning.support.supported_efforts,
                )
            ));
        }
        A::CycleReasoningSummary => {
            let resolved = resolve_openresponses_runtime_config(overrides.as_ref());
            let reasoning = resolve_runtime_reasoning_state(resolved.as_ref(), overrides.as_ref());
            let current = reasoning.effective.as_ref().and_then(|cfg| cfg.summary);
            let next = next_reasoning_summary(current, &reasoning.support.supported_summaries);
            set_reasoning_summary_override(overrides, next);
            sync_preferred_openresponses_state(state, overrides.as_ref(), catalog);
            state.set_status_message(format!(
                "next reasoning summary: {}{}",
                reasoning_summary_label(next),
                reasoning_summary_support_suffix(
                    reasoning.support.summary,
                    &reasoning.support.supported_summaries,
                )
            ));
        }
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
        A::PinActivityRail => {
            state.activity_pinned = !state.activity_pinned;
            state.set_status_message(format!(
                "activity rail: {}",
                if state.activity_pinned {
                    "pinned"
                } else {
                    "auto"
                }
            ));
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

fn resolve_openresponses_runtime_config(
    openresponses_overrides: Option<&Value>,
) -> Option<ripd::OpenResponsesResolvedConfig> {
    let workspace_root = crate::local_authority::default_workspace_root();
    let (resolved, _) = ripd::resolve_openresponses_config(
        &workspace_root,
        openresponses_override_input_from_json(openresponses_overrides),
    );
    resolved
}

pub(super) fn sync_preferred_openresponses_state(
    state: &mut TuiState,
    openresponses_overrides: Option<&Value>,
    catalog: &ModelsMode,
) {
    let resolved = resolve_openresponses_runtime_config(openresponses_overrides);
    let reasoning = resolve_runtime_reasoning_state(resolved.as_ref(), openresponses_overrides);
    let endpoint = resolved
        .as_ref()
        .map(|cfg| cfg.endpoint.clone())
        .or_else(|| catalog.current_endpoint.clone());
    let model = resolved
        .as_ref()
        .and_then(|cfg| cfg.model.clone())
        .or_else(|| catalog.current_model.clone());
    state.set_preferred_openresponses_target(endpoint, model);
    state.set_preferred_openresponses_reasoning(
        reasoning
            .effective
            .as_ref()
            .and_then(|cfg| cfg.effort)
            .map(|value| reasoning_effort_label(Some(value)).to_string()),
        reasoning
            .effective
            .as_ref()
            .and_then(|cfg| cfg.summary)
            .map(|value| reasoning_summary_label(Some(value)).to_string()),
    );
}

fn resolve_runtime_reasoning_state(
    resolved: Option<&ripd::OpenResponsesResolvedConfig>,
    overrides: Option<&Value>,
) -> ripd::ResolvedOpenResponsesReasoning {
    if let Some(cfg) = resolved {
        return ripd::resolve_openresponses_compat_profile(
            cfg.provider_id.as_deref(),
            &cfg.endpoint,
            cfg.model.as_deref(),
        )
        .reasoning(cfg.reasoning.as_ref());
    }

    let overrides = openresponses_override_input_from_json(overrides);
    ripd::resolve_openresponses_compat_profile(
        None,
        overrides.endpoint.as_deref().unwrap_or(""),
        overrides.model.as_deref(),
    )
    .reasoning(overrides.reasoning.as_ref())
}

fn resolve_runtime_include_state(
    resolved: Option<&ripd::OpenResponsesResolvedConfig>,
    overrides: Option<&Value>,
) -> ripd::ResolvedOpenResponsesInclude {
    if let Some(cfg) = resolved {
        return ripd::resolve_openresponses_compat_profile(
            cfg.provider_id.as_deref(),
            &cfg.endpoint,
            cfg.model.as_deref(),
        )
        .include(&cfg.include);
    }

    let overrides = openresponses_override_input_from_json(overrides);
    ripd::resolve_openresponses_compat_profile(
        None,
        overrides.endpoint.as_deref().unwrap_or(""),
        overrides.model.as_deref(),
    )
    .include(overrides.include.as_deref().unwrap_or(&[]))
}

fn reasoning_effort_state_label(reasoning: &ripd::ResolvedOpenResponsesReasoning) -> String {
    reasoning_state_label(
        reasoning
            .effective
            .as_ref()
            .and_then(|cfg| cfg.effort)
            .map(|value| reasoning_effort_label(Some(value)).to_string())
            .unwrap_or_else(|| "inherit".to_string()),
        reasoning
            .requested
            .as_ref()
            .and_then(|cfg| cfg.effort)
            .map(|value| reasoning_effort_label(Some(value)).to_string()),
        reasoning.support.effort,
        format_reasoning_efforts(&reasoning.support.supported_efforts),
    )
}

fn reasoning_summary_state_label(reasoning: &ripd::ResolvedOpenResponsesReasoning) -> String {
    reasoning_state_label(
        reasoning
            .effective
            .as_ref()
            .and_then(|cfg| cfg.summary)
            .map(|value| reasoning_summary_label(Some(value)).to_string())
            .unwrap_or_else(|| "inherit".to_string()),
        reasoning
            .requested
            .as_ref()
            .and_then(|cfg| cfg.summary)
            .map(|value| reasoning_summary_label(Some(value)).to_string()),
        reasoning.support.summary,
        format_reasoning_summaries(&reasoning.support.supported_summaries),
    )
}

fn build_include_option_entries(include: &ripd::ResolvedOpenResponsesInclude) -> Vec<PaletteEntry> {
    ALL_RESPONSE_INCLUDE_OPTIONS
        .iter()
        .map(|(value, title)| PaletteEntry {
            value: include_option_value(*value),
            title: (*title).to_string(),
            subtitle: Some(include_state_label(include, *value)),
            chips: vec![include_support_chip(include, *value).to_string()],
        })
        .collect()
}

fn include_state_label(
    include: &ripd::ResolvedOpenResponsesInclude,
    value: ripd::OpenResponsesInclude,
) -> String {
    let requested = include.requested.contains(&value);
    let effective = include.effective.contains(&value);
    let support = include_support_level(include, value);

    let mut parts = vec![format!("effective: {}", on_off_label(effective))];
    if requested != effective {
        parts.push(format!("requested: {}", on_off_label(requested)));
    }
    parts.push(format!("route: {}", include_support_label(support)));
    parts.join(" • ")
}

fn include_option_value(value: ripd::OpenResponsesInclude) -> String {
    format!("{}{}", OPTION_INCLUDE_PREFIX, include_value_label(value))
}

fn parse_include_option_value(value: &str) -> Option<ripd::OpenResponsesInclude> {
    value
        .strip_prefix(OPTION_INCLUDE_PREFIX)
        .and_then(|value| ripd::parse_openresponses_include(value).ok())
}

fn include_value_label(value: ripd::OpenResponsesInclude) -> &'static str {
    match value {
        ripd::OpenResponsesInclude::FileSearchCallResults => "file_search_call.results",
        ripd::OpenResponsesInclude::WebSearchCallResults => "web_search_call.results",
        ripd::OpenResponsesInclude::WebSearchCallActionSources => "web_search_call.action.sources",
        ripd::OpenResponsesInclude::MessageInputImageImageUrl => "message.input_image.image_url",
        ripd::OpenResponsesInclude::ComputerCallOutputOutputImageUrl => {
            "computer_call_output.output.image_url"
        }
        ripd::OpenResponsesInclude::CodeInterpreterCallOutputs => "code_interpreter_call.outputs",
        ripd::OpenResponsesInclude::ReasoningEncryptedContent => "reasoning.encrypted_content",
        ripd::OpenResponsesInclude::MessageOutputTextLogprobs => "message.output_text.logprobs",
    }
}

fn include_support_level(
    include: &ripd::ResolvedOpenResponsesInclude,
    value: ripd::OpenResponsesInclude,
) -> ripd::CompatLevel {
    if include.support.native_values.contains(&value) {
        ripd::CompatLevel::Native
    } else if include.support.compat_values.contains(&value) {
        ripd::CompatLevel::Compat
    } else if include.support.unsupported_values.contains(&value) {
        ripd::CompatLevel::Unsupported
    } else if include.support.unknown_values.contains(&value) {
        ripd::CompatLevel::Unknown
    } else {
        include.support.request
    }
}

fn include_support_chip(
    include: &ripd::ResolvedOpenResponsesInclude,
    value: ripd::OpenResponsesInclude,
) -> &'static str {
    include_support_label(include_support_level(include, value))
}

fn include_support_label(value: ripd::CompatLevel) -> &'static str {
    match value {
        ripd::CompatLevel::Native => "native",
        ripd::CompatLevel::Compat => "compat",
        ripd::CompatLevel::Unsupported => "unsupported",
        ripd::CompatLevel::Unknown => "unverified",
    }
}

fn on_off_label(flag: bool) -> &'static str {
    if flag {
        "on"
    } else {
        "off"
    }
}

fn reasoning_state_label(
    effective: String,
    requested: Option<String>,
    support_level: ripd::CompatLevel,
    supported_values: Option<String>,
) -> String {
    let mut parts = vec![effective];
    if requested
        .as_deref()
        .filter(|requested| *requested != parts[0].as_str())
        .is_some()
    {
        parts.push(format!("requested: {}", requested.unwrap()));
    }
    if let Some(values) = supported_values {
        parts.push(format!("route: {values}"));
    } else if support_level == ripd::CompatLevel::Unknown {
        parts.push("route: unverified".to_string());
    }
    parts.join(" • ")
}

fn reasoning_effort_label(value: Option<ripd::ReasoningEffort>) -> &'static str {
    match value {
        None => "inherit",
        Some(ripd::ReasoningEffort::None) => "none",
        Some(ripd::ReasoningEffort::Minimal) => "minimal",
        Some(ripd::ReasoningEffort::Low) => "low",
        Some(ripd::ReasoningEffort::Medium) => "medium",
        Some(ripd::ReasoningEffort::High) => "high",
        Some(ripd::ReasoningEffort::Xhigh) => "xhigh",
    }
}

fn reasoning_summary_label(value: Option<ripd::ReasoningSummary>) -> &'static str {
    match value {
        None => "inherit",
        Some(ripd::ReasoningSummary::Auto) => "auto",
        Some(ripd::ReasoningSummary::Concise) => "concise",
        Some(ripd::ReasoningSummary::Detailed) => "detailed",
    }
}

fn next_reasoning_effort(
    current: Option<ripd::ReasoningEffort>,
    supported: &[ripd::ReasoningEffort],
) -> Option<ripd::ReasoningEffort> {
    use ripd::ReasoningEffort as Effort;

    let order = if supported.is_empty() {
        vec![
            None,
            Some(Effort::Minimal),
            Some(Effort::Low),
            Some(Effort::Medium),
            Some(Effort::High),
            Some(Effort::Xhigh),
            Some(Effort::None),
        ]
    } else {
        std::iter::once(None)
            .chain(supported.iter().copied().map(Some))
            .collect()
    };

    cycle_optional_enum(&order, current)
}

fn next_reasoning_summary(
    current: Option<ripd::ReasoningSummary>,
    supported: &[ripd::ReasoningSummary],
) -> Option<ripd::ReasoningSummary> {
    use ripd::ReasoningSummary as Summary;

    let order = if supported.is_empty() {
        vec![
            None,
            Some(Summary::Auto),
            Some(Summary::Concise),
            Some(Summary::Detailed),
        ]
    } else {
        std::iter::once(None)
            .chain(supported.iter().copied().map(Some))
            .collect()
    };

    cycle_optional_enum(&order, current)
}

fn cycle_optional_enum<T: Copy + PartialEq>(order: &[Option<T>], current: Option<T>) -> Option<T> {
    let idx = order
        .iter()
        .position(|candidate| *candidate == current)
        .unwrap_or(0);
    order[(idx + 1) % order.len()]
}

fn toggle_response_include_with_overrides(
    state: &mut TuiState,
    overrides: &mut Option<Value>,
    catalog: &ModelsMode,
    include_value: ripd::OpenResponsesInclude,
) {
    let resolved = resolve_openresponses_runtime_config(overrides.as_ref());
    let current = resolve_runtime_include_state(resolved.as_ref(), overrides.as_ref());
    let requested = current.requested.contains(&include_value);
    set_include_override(overrides, include_value, !requested);
    sync_preferred_openresponses_state(state, overrides.as_ref(), catalog);

    let resolved = resolve_openresponses_runtime_config(overrides.as_ref());
    let updated = resolve_runtime_include_state(resolved.as_ref(), overrides.as_ref());
    let effective = updated.effective.contains(&include_value);
    let support = include_support_level(&updated, include_value);
    let message = if !requested && !effective {
        format!(
            "requested include {} but route {} drops it",
            include_value_label(include_value),
            include_support_label(support)
        )
    } else {
        format!(
            "include {}: {} (route: {})",
            include_value_label(include_value),
            on_off_label(!requested),
            include_support_label(support)
        )
    };
    state.set_status_message(message);
}

fn set_reasoning_effort_override(
    overrides: &mut Option<Value>,
    next: Option<ripd::ReasoningEffort>,
) {
    let value = next.map(|value| Value::String(reasoning_effort_label(Some(value)).to_string()));
    set_reasoning_override_field(overrides, "effort", value);
}

fn set_reasoning_summary_override(
    overrides: &mut Option<Value>,
    next: Option<ripd::ReasoningSummary>,
) {
    let value = next.map(|value| Value::String(reasoning_summary_label(Some(value)).to_string()));
    set_reasoning_override_field(overrides, "summary", value);
}

fn set_include_override(
    overrides: &mut Option<Value>,
    include_value: ripd::OpenResponsesInclude,
    enabled: bool,
) {
    let mut root = match overrides.take() {
        Some(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };

    let mut include = root
        .get("include")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let include_json = serde_json::to_value(include_value).expect("include serializes");
    include.retain(|value| value != &include_json);
    if enabled {
        include.push(include_json);
    }

    if include.is_empty() {
        root.remove("include");
    } else {
        root.insert("include".to_string(), Value::Array(include));
    }

    *overrides = (!root.is_empty()).then_some(Value::Object(root));
}

fn reasoning_support_suffix(
    level: ripd::CompatLevel,
    supported: &[ripd::ReasoningEffort],
) -> String {
    match format_reasoning_efforts(supported) {
        Some(values) => format!(" (route: {values})"),
        None if level == ripd::CompatLevel::Unknown => " (route support unverified)".to_string(),
        _ => String::new(),
    }
}

fn reasoning_summary_support_suffix(
    level: ripd::CompatLevel,
    supported: &[ripd::ReasoningSummary],
) -> String {
    match format_reasoning_summaries(supported) {
        Some(values) => format!(" (route: {values})"),
        None if level == ripd::CompatLevel::Unknown => " (route support unverified)".to_string(),
        _ => String::new(),
    }
}

fn format_reasoning_efforts(values: &[ripd::ReasoningEffort]) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    Some(
        values
            .iter()
            .map(|value| reasoning_effort_label(Some(*value)))
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn format_reasoning_summaries(values: &[ripd::ReasoningSummary]) -> Option<String> {
    if values.is_empty() {
        return None;
    }
    Some(
        values
            .iter()
            .map(|value| reasoning_summary_label(Some(*value)))
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn set_reasoning_override_field(
    overrides: &mut Option<Value>,
    field: &'static str,
    value: Option<Value>,
) {
    let mut root = match overrides.take() {
        Some(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };

    if let Some(value) = value {
        let reasoning = root
            .entry("reasoning".to_string())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if let Value::Object(reasoning) = reasoning {
            reasoning.insert(field.to_string(), value);
        }
    } else if let Some(Value::Object(reasoning)) = root.get_mut("reasoning") {
        reasoning.remove(field);
        if reasoning.is_empty() {
            root.remove("reasoning");
        }
    }

    *overrides = (!root.is_empty()).then_some(Value::Object(root));
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
    sync_preferred_openresponses_state(state, overrides.as_ref(), catalog);
    state.close_overlay();
    state.set_status_message(format!("next model: {}", resolved.route));
    Ok(())
}
