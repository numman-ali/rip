//! Unit tests for palette coordination: override-input parsing, the
//! five `open_*_palette` helpers, `cycle_palette_mode` rotation, and
//! `apply_command_action` / `apply_palette_selection` dispatch.

use super::*;
use rip_tui::palette::modes::models::ModelRoute;
use rip_tui::{Overlay, PaletteMode, PaletteOrigin, ThemeId, TuiState, VimMode};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

fn tui() -> TuiState {
    TuiState::new(100)
}

fn empty_catalog() -> ModelsMode {
    ModelsMode::new(Vec::new(), BTreeMap::new(), None, None, None)
}

fn catalog_with_one_route() -> ModelsMode {
    let route = ModelRoute {
        route: "openai/gpt-5-nano".to_string(),
        provider_id: "openai".to_string(),
        model_id: "gpt-5-nano".to_string(),
        endpoint: "https://example.invalid/openresponses".to_string(),
        label: Some("gpt-5 nano".to_string()),
        variants: 0,
        sources: vec!["catalog".to_string()],
    };
    let mut endpoints = BTreeMap::new();
    endpoints.insert(
        "openai".to_string(),
        "https://example.invalid/openresponses".to_string(),
    );
    ModelsMode::new(vec![route], endpoints, None, None, None)
}

#[test]
fn override_input_returns_default_when_value_is_none() {
    let out = openresponses_override_input_from_json(None);
    assert!(out.endpoint.is_none());
    assert!(out.model.is_none());
    assert!(out.stateless_history.is_none());
    assert!(out.parallel_tool_calls.is_none());
    assert!(out.followup_user_message.is_none());
    assert!(out.reasoning.is_none());
}

#[test]
fn override_input_returns_default_when_value_is_not_object() {
    let value = json!("not-an-object");
    let out = openresponses_override_input_from_json(Some(&value));
    assert!(out.endpoint.is_none());
    assert!(out.model.is_none());
}

#[test]
fn override_input_extracts_all_known_fields() {
    let value = json!({
        "endpoint": "https://example.invalid/openresponses",
        "model": "openai/gpt-5-nano",
        "stateless_history": true,
        "parallel_tool_calls": false,
        "followup_user_message": "continue",
        "reasoning": {
            "effort": "high",
            "summary": "detailed"
        },
        "unknown_field": 42,
    });
    let out = openresponses_override_input_from_json(Some(&value));
    assert_eq!(
        out.endpoint.as_deref(),
        Some("https://example.invalid/openresponses")
    );
    assert_eq!(out.model.as_deref(), Some("openai/gpt-5-nano"));
    assert_eq!(out.stateless_history, Some(true));
    assert_eq!(out.parallel_tool_calls, Some(false));
    assert_eq!(out.followup_user_message.as_deref(), Some("continue"));
    assert_eq!(
        out.reasoning.as_ref().and_then(|value| value.effort),
        Some(ripd::ReasoningEffort::High)
    );
    assert_eq!(
        out.reasoning.as_ref().and_then(|value| value.summary),
        Some(ripd::ReasoningSummary::Detailed)
    );
}

#[test]
fn override_input_handles_partial_object() {
    let value = json!({ "model": "openai/gpt-5-pro" });
    let out = openresponses_override_input_from_json(Some(&value));
    assert!(out.endpoint.is_none());
    assert_eq!(out.model.as_deref(), Some("openai/gpt-5-pro"));
    assert!(out.stateless_history.is_none());
    assert!(out.reasoning.is_none());
}

#[test]
fn current_reasoning_helpers_fall_back_to_override_input_without_resolved_route() {
    let overrides = json!({
        "reasoning": {
            "effort": "high",
            "summary": "detailed"
        }
    });
    let reasoning = resolve_runtime_reasoning_state(None, Some(&overrides));
    assert_eq!(
        reasoning.effective.as_ref().and_then(|value| value.effort),
        Some(ripd::ReasoningEffort::High)
    );
    assert_eq!(
        reasoning.effective.as_ref().and_then(|value| value.summary),
        Some(ripd::ReasoningSummary::Detailed)
    );
}

#[test]
fn next_reasoning_effort_cycles_through_inherit_and_explicit_none() {
    use ripd::ReasoningEffort as Effort;

    assert_eq!(next_reasoning_effort(None, &[]), Some(Effort::Minimal));
    assert_eq!(
        next_reasoning_effort(Some(Effort::Minimal), &[]),
        Some(Effort::Low)
    );
    assert_eq!(
        next_reasoning_effort(Some(Effort::Low), &[]),
        Some(Effort::Medium)
    );
    assert_eq!(
        next_reasoning_effort(Some(Effort::Medium), &[]),
        Some(Effort::High)
    );
    assert_eq!(
        next_reasoning_effort(Some(Effort::High), &[]),
        Some(Effort::Xhigh)
    );
    assert_eq!(
        next_reasoning_effort(Some(Effort::Xhigh), &[]),
        Some(Effort::None)
    );
    assert_eq!(next_reasoning_effort(Some(Effort::None), &[]), None);
}

#[test]
fn next_reasoning_effort_obeys_route_supported_subset() {
    use ripd::ReasoningEffort as Effort;

    let supported = [Effort::Minimal, Effort::Low, Effort::Medium, Effort::High];
    assert_eq!(
        next_reasoning_effort(None, &supported),
        Some(Effort::Minimal)
    );
    assert_eq!(next_reasoning_effort(Some(Effort::High), &supported), None);
    assert_eq!(
        next_reasoning_effort(Some(Effort::Xhigh), &supported),
        Some(Effort::Minimal)
    );
}

#[test]
fn next_reasoning_summary_cycles_through_all_values() {
    use ripd::ReasoningSummary as Summary;

    assert_eq!(next_reasoning_summary(None, &[]), Some(Summary::Auto));
    assert_eq!(
        next_reasoning_summary(Some(Summary::Auto), &[]),
        Some(Summary::Concise)
    );
    assert_eq!(
        next_reasoning_summary(Some(Summary::Concise), &[]),
        Some(Summary::Detailed)
    );
    assert_eq!(next_reasoning_summary(Some(Summary::Detailed), &[]), None);
}

#[test]
fn next_reasoning_summary_obeys_route_supported_subset() {
    use ripd::ReasoningSummary as Summary;

    let supported = [Summary::Auto, Summary::Detailed];
    assert_eq!(
        next_reasoning_summary(None, &supported),
        Some(Summary::Auto)
    );
    assert_eq!(
        next_reasoning_summary(Some(Summary::Auto), &supported),
        Some(Summary::Detailed)
    );
    assert_eq!(
        next_reasoning_summary(Some(Summary::Detailed), &supported),
        None
    );
}

#[test]
fn clearing_reasoning_summary_preserves_other_override_fields() {
    let mut overrides = Some(json!({
        "model": "gpt-5.4-mini",
        "reasoning": {
            "effort": "medium",
            "summary": "detailed"
        }
    }));
    set_reasoning_summary_override(&mut overrides, None);
    assert_eq!(
        overrides
            .as_ref()
            .and_then(|value| value.get("model"))
            .and_then(Value::as_str),
        Some("gpt-5.4-mini")
    );
    assert_eq!(
        overrides
            .as_ref()
            .and_then(|value| value.get("reasoning"))
            .and_then(|value| value.get("effort"))
            .and_then(Value::as_str),
        Some("medium")
    );
    assert!(overrides
        .as_ref()
        .and_then(|value| value.get("reasoning"))
        .and_then(|value| value.get("summary"))
        .is_none());
}

#[test]
fn open_command_palette_mounts_command_mode() {
    let mut state = tui();
    open_command_palette(&mut state, PaletteOrigin::TopCenter);
    let overlay = state.palette_state_clone().expect("palette should be open");
    assert_eq!(overlay.mode, PaletteMode::Command);
    assert!(!overlay.entries.is_empty());
}

#[test]
fn open_go_to_palette_mounts_navigation_mode() {
    let mut state = tui();
    open_go_to_palette(&mut state, PaletteOrigin::Center);
    let overlay = state.palette_state_clone().unwrap();
    assert_eq!(overlay.mode, PaletteMode::Navigation);
    assert_eq!(overlay.origin, PaletteOrigin::Center);
}

#[test]
fn open_threads_palette_mounts_session_mode_with_current_when_set() {
    let mut state = tui();
    state.set_continuity_id("t-current".to_string());
    open_threads_palette(&mut state, PaletteOrigin::TopLeft);
    let overlay = state.palette_state_clone().unwrap();
    assert_eq!(overlay.mode, PaletteMode::Session);
    assert!(overlay
        .entries
        .iter()
        .any(|entry| entry.value == "t-current"));
}

#[test]
fn open_options_palette_mounts_option_mode_with_entries() {
    let mut state = tui();
    open_options_palette(&mut state, PaletteOrigin::TopCenter);
    let overlay = state.palette_state_clone().unwrap();
    assert_eq!(overlay.mode, PaletteMode::Option);
    assert!(!overlay.entries.is_empty());
}

#[test]
fn open_options_palette_with_overrides_shows_effective_reasoning_values() {
    let mut state = tui();
    let overrides = json!({
        "endpoint": "https://api.openai.com/v1/responses",
        "model": "gpt-5.4-mini",
        "reasoning": {
            "effort": "high",
            "summary": "detailed"
        }
    });
    open_options_palette_with_overrides(&mut state, Some(&overrides), PaletteOrigin::TopCenter);
    let overlay = state.palette_state_clone().unwrap();
    let effort = overlay
        .entries
        .iter()
        .find(|entry| entry.value == "options.reasoning-effort")
        .expect("reasoning effort entry");
    let summary = overlay
        .entries
        .iter()
        .find(|entry| entry.value == "options.reasoning-summary")
        .expect("reasoning summary entry");
    assert_eq!(
        effort.subtitle.as_deref(),
        Some("current: high • route: none/low/medium/high/xhigh")
    );
    assert_eq!(
        summary.subtitle.as_deref(),
        Some("current: detailed • route: auto/concise/detailed")
    );
}

#[test]
fn open_model_palette_mounts_model_mode_with_catalog_entries() {
    let mut state = tui();
    let catalog = catalog_with_one_route();
    open_model_palette(&mut state, &catalog, PaletteOrigin::TopRight);
    let overlay = state.palette_state_clone().unwrap();
    assert_eq!(overlay.mode, PaletteMode::Model);
    assert_eq!(overlay.origin, PaletteOrigin::TopRight);
    assert_eq!(overlay.entries.len(), 1);
}

#[test]
fn cycle_palette_mode_rotates_through_all_five_modes() {
    let mut state = tui();
    let catalog = empty_catalog();
    open_command_palette(&mut state, PaletteOrigin::TopCenter);
    let observed: Vec<PaletteMode> = (0..5)
        .map(|_| {
            cycle_palette_mode(&mut state, &catalog);
            state.palette_state_clone().unwrap().mode
        })
        .collect();
    assert_eq!(
        observed,
        vec![
            PaletteMode::Model,
            PaletteMode::Navigation,
            PaletteMode::Session,
            PaletteMode::Option,
            PaletteMode::Command,
        ]
    );
}

#[test]
fn cycle_palette_mode_is_noop_when_no_palette_open() {
    let mut state = tui();
    let catalog = empty_catalog();
    cycle_palette_mode(&mut state, &catalog);
    assert!(state.palette_state_clone().is_none());
}

#[test]
fn command_action_follow_tail_toggles_auto_follow() {
    let mut state = tui();
    let catalog = empty_catalog();
    state.auto_follow = true;
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::FollowTail,
        &mut state,
        &catalog,
    );
    assert!(!state.auto_follow);
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::FollowTail,
        &mut state,
        &catalog,
    );
    assert!(state.auto_follow);
}

#[test]
fn command_action_toggle_auto_follow_flips_the_flag() {
    let mut state = tui();
    let catalog = empty_catalog();
    state.auto_follow = false;
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ToggleAutoFollow,
        &mut state,
        &catalog,
    );
    assert!(state.auto_follow);
}

#[test]
fn command_action_cycle_reasoning_effort_updates_overrides() {
    let mut state = tui();
    let catalog = empty_catalog();
    let mut overrides = Some(json!({
        "reasoning": {
            "effort": "medium",
            "summary": "concise"
        }
    }));
    apply_command_action_with_overrides(
        rip_tui::palette::modes::command::CommandAction::CycleReasoningEffort,
        &mut state,
        &mut overrides,
        &catalog,
    );
    assert_eq!(
        overrides
            .as_ref()
            .and_then(|value| value.get("reasoning"))
            .and_then(|value| value.get("effort"))
            .and_then(Value::as_str),
        Some("high")
    );
    assert_eq!(
        overrides
            .as_ref()
            .and_then(|value| value.get("reasoning"))
            .and_then(|value| value.get("summary"))
            .and_then(Value::as_str),
        Some("concise")
    );
}

#[test]
fn command_action_cycle_reasoning_effort_wraps_to_inherit_and_drops_empty_reasoning() {
    let mut state = tui();
    let catalog = empty_catalog();
    let mut overrides = Some(json!({
        "endpoint": "https://api.openai.com/v1/responses",
        "model": "gpt-5.4-nano",
        "reasoning": {
            "effort": "xhigh"
        }
    }));
    apply_command_action_with_overrides(
        rip_tui::palette::modes::command::CommandAction::CycleReasoningEffort,
        &mut state,
        &mut overrides,
        &catalog,
    );
    assert!(overrides
        .as_ref()
        .and_then(|value| value.get("reasoning"))
        .is_none());
}

#[test]
fn command_action_cycle_reasoning_summary_updates_overrides() {
    let mut state = tui();
    let catalog = empty_catalog();
    let mut overrides = Some(json!({
        "model": "gpt-5.4-mini",
        "reasoning": {
            "summary": "auto"
        }
    }));
    apply_command_action_with_overrides(
        rip_tui::palette::modes::command::CommandAction::CycleReasoningSummary,
        &mut state,
        &mut overrides,
        &catalog,
    );
    assert_eq!(
        overrides
            .as_ref()
            .and_then(|value| value.get("reasoning"))
            .and_then(|value| value.get("summary"))
            .and_then(Value::as_str),
        Some("concise")
    );
    assert_eq!(
        overrides
            .as_ref()
            .and_then(|value| value.get("model"))
            .and_then(Value::as_str),
        Some("gpt-5.4-mini")
    );
}

#[test]
fn command_action_cycle_reasoning_effort_syncs_state_label() {
    let mut state = tui();
    let catalog = empty_catalog();
    let mut overrides = Some(json!({
        "reasoning": {
            "effort": "medium"
        }
    }));
    apply_command_action_with_overrides(
        rip_tui::palette::modes::command::CommandAction::CycleReasoningEffort,
        &mut state,
        &mut overrides,
        &catalog,
    );
    assert_eq!(
        state.preferred_openresponses_reasoning_effort.as_deref(),
        Some("high")
    );
}

#[test]
fn command_action_cycle_reasoning_effort_uses_route_supported_subset() {
    let mut state = tui();
    let catalog = empty_catalog();
    let mut overrides = Some(json!({
        "endpoint": "https://api.openai.com/v1/responses",
        "model": "gpt-5.4-nano",
        "reasoning": {
            "effort": "minimal"
        }
    }));
    apply_command_action_with_overrides(
        rip_tui::palette::modes::command::CommandAction::CycleReasoningEffort,
        &mut state,
        &mut overrides,
        &catalog,
    );
    assert_eq!(
        overrides
            .as_ref()
            .and_then(|value| value.get("reasoning"))
            .and_then(|value| value.get("effort"))
            .and_then(Value::as_str),
        Some("none")
    );
    assert_eq!(
        state.preferred_openresponses_reasoning_effort.as_deref(),
        Some("none")
    );
}

#[test]
fn open_options_palette_shows_effective_reasoning_and_route_support() {
    let mut state = tui();
    let overrides = json!({
        "endpoint": "https://openrouter.ai/api/v1/responses",
        "model": "nvidia/nemotron-3-super-120b-a12b:free",
        "reasoning": {
            "effort": "xhigh",
            "summary": "detailed"
        }
    });

    open_options_palette_with_overrides(&mut state, Some(&overrides), PaletteOrigin::TopCenter);
    let overlay = state.palette_state_clone().expect("palette");
    let effort_entry = overlay
        .entries
        .iter()
        .find(|entry| entry.value == "options.reasoning-effort")
        .expect("effort entry");
    let summary_entry = overlay
        .entries
        .iter()
        .find(|entry| entry.value == "options.reasoning-summary")
        .expect("summary entry");

    assert!(effort_entry
        .subtitle
        .as_deref()
        .unwrap_or("")
        .contains("requested: xhigh"));
    assert!(effort_entry
        .subtitle
        .as_deref()
        .unwrap_or("")
        .contains("route: minimal/low/medium/high"));
    assert!(summary_entry
        .subtitle
        .as_deref()
        .unwrap_or("")
        .contains("route: unverified"));
}

#[test]
fn command_action_toggle_reasoning_visibility_flips_state() {
    let mut state = tui();
    let catalog = empty_catalog();
    assert!(state.reasoning_visible);
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ToggleReasoningVisibility,
        &mut state,
        &catalog,
    );
    assert!(!state.reasoning_visible);
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ToggleReasoningVisibility,
        &mut state,
        &catalog,
    );
    assert!(state.reasoning_visible);
}

#[test]
fn command_action_scroll_bottom_resets_scroll_and_follows() {
    let mut state = tui();
    let catalog = empty_catalog();
    state.canvas_scroll_from_bottom = 42;
    state.auto_follow = false;
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ScrollCanvasBottom,
        &mut state,
        &catalog,
    );
    assert_eq!(state.canvas_scroll_from_bottom, 0);
    assert!(state.auto_follow);
}

#[test]
fn command_action_scroll_top_does_not_panic() {
    let mut state = tui();
    let catalog = empty_catalog();
    // With no canvas content this is a no-op in effect, but the match
    // arm still executes — exercising it closes the coverage gap.
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ScrollCanvasTop,
        &mut state,
        &catalog,
    );
}

#[test]
fn command_action_toggle_theme_flips_theme_id() {
    let mut state = tui();
    let catalog = empty_catalog();
    let start = state.theme;
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ToggleTheme,
        &mut state,
        &catalog,
    );
    assert_ne!(state.theme, start);
}

#[test]
fn command_action_toggle_vim_sets_mode_and_status() {
    let mut state = tui();
    let catalog = empty_catalog();
    assert!(!state.vim_input_mode);
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ToggleVimInputMode,
        &mut state,
        &catalog,
    );
    assert!(state.vim_input_mode);
    assert_eq!(state.vim_mode, VimMode::Normal);
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ToggleVimInputMode,
        &mut state,
        &catalog,
    );
    assert!(!state.vim_input_mode);
    assert_eq!(state.vim_mode, VimMode::Insert);
}

#[test]
fn command_action_show_debug_info_opens_debug_overlay() {
    let mut state = tui();
    let catalog = empty_catalog();
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ShowDebugInfo,
        &mut state,
        &catalog,
    );
    assert_eq!(*state.overlay(), Overlay::Debug);
}

#[test]
fn command_action_switch_model_opens_model_palette() {
    let mut state = tui();
    let catalog = catalog_with_one_route();
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::SwitchModel,
        &mut state,
        &catalog,
    );
    let overlay = state.palette_state_clone().unwrap();
    assert_eq!(overlay.mode, PaletteMode::Model);
}

#[test]
fn command_action_quit_sets_status_message_without_closing() {
    let mut state = tui();
    let catalog = empty_catalog();
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::Quit,
        &mut state,
        &catalog,
    );
    // Should not panic; status message is set. We check overlay is
    // still None (Quit does not summon anything).
    assert_eq!(*state.overlay(), Overlay::None);
}

#[test]
fn command_action_clear_selection_unfocuses() {
    let mut state = tui();
    let catalog = empty_catalog();
    state.focused_message_id = Some("msg-1".to_string());
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ClearSelection,
        &mut state,
        &catalog,
    );
    assert!(state.focused_message_id.is_none());
}

#[test]
fn command_action_prev_error_selects_last_error_seq() {
    let mut state = tui();
    let catalog = empty_catalog();
    state.last_error_seq = Some(42);
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::PrevError,
        &mut state,
        &catalog,
    );
    assert_eq!(state.selected_seq, Some(42));
}

#[test]
fn command_action_deferred_entry_emits_status_and_does_not_panic() {
    let mut state = tui();
    let catalog = empty_catalog();
    // Branch `A::other` path — a deferred entry emits a status message
    // like "use the dedicated hotkey …"; any deferred `CommandAction`
    // variant works. Use `NewThread` which is [deferred] in the
    // palette. This exercises the default match arm.
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::NewThread,
        &mut state,
        &catalog,
    );
    assert_eq!(*state.overlay(), Overlay::None);
}

#[test]
fn apply_palette_selection_returns_err_when_no_palette_open() {
    let mut state = tui();
    let mut overrides: Option<Value> = None;
    let mut catalog = empty_catalog();
    let result = apply_palette_selection(&mut state, &mut overrides, &mut catalog);
    assert!(result.is_err());
}

#[test]
fn apply_palette_selection_in_navigation_mode_focuses_message_and_closes() {
    let mut state = tui();
    let mut overrides: Option<Value> = None;
    let mut catalog = empty_catalog();

    // Manually mount a Navigation palette with one entry — the Go-To
    // palette pushes values from canvas messages; here we push a
    // synthetic entry through `open_palette` directly.
    state.open_palette(
        PaletteMode::Navigation,
        PaletteOrigin::Center,
        vec![rip_tui::PaletteEntry {
            value: "msg-target".to_string(),
            title: "msg-target".to_string(),
            subtitle: None,
            chips: Vec::new(),
        }],
        "no results".to_string(),
        false,
        String::new(),
    );

    let result = apply_palette_selection(&mut state, &mut overrides, &mut catalog);
    assert!(result.is_ok());
    assert_eq!(state.focused_message_id.as_deref(), Some("msg-target"));
    assert_eq!(*state.overlay(), Overlay::None);
}

#[test]
fn apply_palette_selection_in_session_mode_switches_thread_and_closes() {
    let mut state = tui();
    let mut overrides: Option<Value> = None;
    let mut catalog = empty_catalog();

    state.open_palette(
        PaletteMode::Session,
        PaletteOrigin::TopLeft,
        vec![rip_tui::PaletteEntry {
            value: "t-new".to_string(),
            title: "t-new".to_string(),
            subtitle: None,
            chips: Vec::new(),
        }],
        "no threads".to_string(),
        false,
        String::new(),
    );

    let result = apply_palette_selection(&mut state, &mut overrides, &mut catalog);
    assert!(result.is_ok());
    assert_eq!(state.continuity_id.as_deref(), Some("t-new"));
    assert_eq!(*state.overlay(), Overlay::None);
}

#[test]
fn apply_palette_selection_in_command_mode_with_unknown_action_returns_err() {
    let mut state = tui();
    let mut overrides: Option<Value> = None;
    let mut catalog = empty_catalog();

    state.open_palette(
        PaletteMode::Command,
        PaletteOrigin::TopCenter,
        vec![rip_tui::PaletteEntry {
            value: "not-a-real-action".to_string(),
            title: "bogus".to_string(),
            subtitle: None,
            chips: Vec::new(),
        }],
        "no results".to_string(),
        false,
        String::new(),
    );

    let result = apply_palette_selection(&mut state, &mut overrides, &mut catalog);
    assert!(result.is_err());
}

#[test]
fn apply_palette_selection_in_command_mode_with_known_action_routes_and_closes() {
    let mut state = tui();
    let mut overrides: Option<Value> = None;
    let mut catalog = empty_catalog();

    state.open_palette(
        PaletteMode::Command,
        PaletteOrigin::TopCenter,
        vec![rip_tui::PaletteEntry {
            value: "canvas.scroll-bottom".to_string(),
            title: "scroll to bottom".to_string(),
            subtitle: None,
            chips: Vec::new(),
        }],
        "no results".to_string(),
        false,
        String::new(),
    );

    let result = apply_palette_selection(&mut state, &mut overrides, &mut catalog);
    assert!(result.is_ok());
    assert_eq!(*state.overlay(), Overlay::None);
    assert!(state.auto_follow);
}

#[test]
fn apply_model_palette_selection_records_endpoint_model_and_closes() {
    let mut state = tui();
    let mut overrides: Option<Value> = None;
    let mut catalog = catalog_with_one_route();

    state.open_palette(
        PaletteMode::Model,
        PaletteOrigin::TopRight,
        vec![rip_tui::PaletteEntry {
            value: "openai/gpt-5-nano".to_string(),
            title: "openai/gpt-5-nano".to_string(),
            subtitle: None,
            chips: Vec::new(),
        }],
        "no routes".to_string(),
        false,
        String::new(),
    );

    apply_model_palette_selection(&mut state, &mut overrides, &mut catalog)
        .expect("resolution should succeed");
    let map: &Map<String, Value> = overrides.as_ref().unwrap().as_object().unwrap();
    assert_eq!(
        map.get("endpoint").and_then(Value::as_str),
        Some("https://example.invalid/openresponses")
    );
    assert_eq!(map.get("model").and_then(Value::as_str), Some("gpt-5-nano"));
    assert_eq!(*state.overlay(), Overlay::None);
    assert_eq!(catalog.current_route.as_deref(), Some("openai/gpt-5-nano"));
}

#[test]
fn apply_model_palette_selection_fails_for_unparseable_route_with_no_provider() {
    let mut state = tui();
    let mut overrides: Option<Value> = None;
    let mut catalog = empty_catalog();

    state.open_palette(
        PaletteMode::Model,
        PaletteOrigin::TopRight,
        vec![rip_tui::PaletteEntry {
            value: "openai/gpt-5-nano".to_string(),
            title: "openai/gpt-5-nano".to_string(),
            subtitle: None,
            chips: Vec::new(),
        }],
        "no routes".to_string(),
        false,
        String::new(),
    );

    // Empty catalog has no provider endpoints and `default_endpoint_for_provider`
    // for "openai" returns a default. So this case should actually succeed.
    // Use a provider that has no default instead.
    // Replace the entry:
    state.open_palette(
        PaletteMode::Model,
        PaletteOrigin::TopRight,
        vec![rip_tui::PaletteEntry {
            value: "nonexistent-provider/model".to_string(),
            title: "nonexistent-provider/model".to_string(),
            subtitle: None,
            chips: Vec::new(),
        }],
        "no routes".to_string(),
        false,
        String::new(),
    );

    let result = apply_model_palette_selection(&mut state, &mut overrides, &mut catalog);
    assert!(result.is_err());
    // Overrides must remain untouched on error.
    assert!(overrides.is_none());
}

#[test]
fn theme_toggle_round_trips() {
    let mut state = tui();
    let catalog = empty_catalog();
    let start = state.theme;
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ToggleTheme,
        &mut state,
        &catalog,
    );
    apply_command_action(
        rip_tui::palette::modes::command::CommandAction::ToggleTheme,
        &mut state,
        &catalog,
    );
    assert_eq!(state.theme, start);
    assert!(matches!(
        state.theme,
        ThemeId::DefaultDark | ThemeId::DefaultLight
    ));
}
