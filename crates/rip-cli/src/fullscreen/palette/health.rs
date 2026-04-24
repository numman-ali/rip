use std::collections::BTreeMap;

use rip_tui::palette::modes::models::set_model_route_health;
use rip_tui::{ModelRoute, PaletteEntry};
use serde_json::Value;

use super::{format_reasoning_efforts, openresponses_override_input_from_json};

pub(super) const OPTION_ROUTE_HEALTH: &str = "options.route-health";

#[derive(Debug, Clone, PartialEq, Eq)]
struct RouteHealth {
    summary: String,
    chips: Vec<String>,
}

pub(super) fn annotate_model_route_health(routes_by_value: &mut BTreeMap<String, ModelRoute>) {
    let routes = routes_by_value
        .iter()
        .map(|(route, record)| {
            (
                route.clone(),
                record.provider_id.clone(),
                record.endpoint.clone(),
                record.model_id.clone(),
            )
        })
        .collect::<Vec<_>>();

    for (route, provider_id, endpoint, model_id) in routes {
        let health = build_route_health(
            Some(provider_id.as_str()),
            endpoint.as_str(),
            Some(model_id.as_str()),
            false,
        );
        set_model_route_health(routes_by_value, &route, Some(health.summary), health.chips);
    }
}

pub(super) fn build_route_health_option_entries(
    resolved: Option<&ripd::OpenResponsesResolvedConfig>,
    overrides: Option<&Value>,
) -> Vec<PaletteEntry> {
    build_active_route_health(resolved, overrides)
        .map(|health| {
            vec![PaletteEntry {
                value: OPTION_ROUTE_HEALTH.to_string(),
                title: "Active provider/model health".to_string(),
                subtitle: Some(health.summary),
                chips: health.chips,
            }]
        })
        .unwrap_or_default()
}

fn build_active_route_health(
    resolved: Option<&ripd::OpenResponsesResolvedConfig>,
    overrides: Option<&Value>,
) -> Option<RouteHealth> {
    let (route, provider_id, endpoint, model, stateless_history) = if let Some(resolved) = resolved
    {
        (
            resolved
                .effective_route
                .clone()
                .or_else(|| resolved.route.clone())
                .or_else(|| resolved.model.clone())
                .unwrap_or_else(|| resolved.endpoint.clone()),
            resolved.provider_id.clone(),
            resolved.endpoint.clone(),
            resolved.model.clone(),
            resolved.stateless_history,
        )
    } else {
        let overrides = openresponses_override_input_from_json(overrides);
        let endpoint = overrides.endpoint?;
        let route = overrides.model.clone().unwrap_or_else(|| endpoint.clone());
        (
            route,
            None,
            endpoint,
            overrides.model,
            overrides.stateless_history.unwrap_or(false),
        )
    };

    let mut health = build_route_health(
        provider_id.as_deref(),
        &endpoint,
        model.as_deref(),
        stateless_history,
    );
    health.summary = format!("{route} | {}", health.summary);
    Some(health)
}

fn build_route_health(
    provider_id: Option<&str>,
    endpoint: &str,
    model: Option<&str>,
    requested_stateless_history: bool,
) -> RouteHealth {
    let compat = ripd::resolve_openresponses_compat_profile(provider_id, endpoint, model);
    let conversation = compat.conversation(requested_stateless_history);
    let reasoning = compat.reasoning_support();
    let web_search = compat.web_search_support();
    let modalities = compat
        .model
        .map(|model| model.health.input_modalities)
        .unwrap_or(compat.provider.input_modalities);

    let stream = compat_level_label(compat.provider.stream_shape);
    let conversation_label = conversation_strategy_chip(conversation.effective);
    let reasoning_label = reasoning_support_summary(&reasoning);
    let web_label = compat_level_label(web_search.request);
    let image_label = compat_level_label(modalities.input_image);

    let mut chips = vec![stream.to_string(), conversation_label.to_string()];
    chips.push(reasoning_chip(&reasoning).to_string());
    chips.push(format!("web:{web_label}"));
    if matches!(
        modalities.input_image,
        ripd::CompatLevel::Native | ripd::CompatLevel::Compat
    ) {
        chips.push("image".to_string());
    }
    chips.dedup();

    RouteHealth {
        summary: format!(
            "{} | {} | reasoning {} | web {} | image {}",
            compat.provider.label, conversation_label, reasoning_label, web_label, image_label
        ),
        chips,
    }
}

fn compat_level_label(value: ripd::CompatLevel) -> &'static str {
    match value {
        ripd::CompatLevel::Native => "native",
        ripd::CompatLevel::Compat => "compat",
        ripd::CompatLevel::Unsupported => "unsupported",
        ripd::CompatLevel::Unknown => "unverified",
    }
}

fn conversation_strategy_chip(value: ripd::ConversationStrategy) -> &'static str {
    match value {
        ripd::ConversationStrategy::PreviousResponseId => "stateful",
        ripd::ConversationStrategy::StatelessHistory => "stateless",
        ripd::ConversationStrategy::ConfigDriven => "config",
    }
}

fn reasoning_chip(reasoning: &ripd::OpenResponsesReasoningSupport) -> &'static str {
    match reasoning.parameter {
        ripd::CompatLevel::Unsupported => "no-reasoning",
        ripd::CompatLevel::Unknown => "reasoning?",
        ripd::CompatLevel::Native | ripd::CompatLevel::Compat => "reasoning",
    }
}

fn reasoning_support_summary(reasoning: &ripd::OpenResponsesReasoningSupport) -> String {
    format_reasoning_efforts(&reasoning.supported_efforts)
        .unwrap_or_else(|| compat_level_label(reasoning.effort).to_string())
}
