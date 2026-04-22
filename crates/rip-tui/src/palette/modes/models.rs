//! Models palette mode — migrated from `rip-cli::fullscreen::ModelPaletteCatalog`.
//!
//! The UI-facing pieces (entry shaping, route resolution, provider
//! endpoint heuristics) live here. The config-loading side (reading
//! ripd's resolved OpenResponses config and seeding route sources)
//! stays in rip-cli: that layer owns the dependency on ripd and
//! should not bleed into rip-tui.

use std::collections::BTreeMap;

use crate::PaletteEntry;

use super::super::PaletteSource;

/// A single model-route record surfaced in the Models palette mode.
#[derive(Debug, Clone)]
pub struct ModelRoute {
    pub route: String,
    pub provider_id: String,
    pub model_id: String,
    pub endpoint: String,
    pub label: Option<String>,
    pub variants: usize,
    pub sources: Vec<String>,
}

/// Resolved target of a Models palette selection — endpoint + model
/// pair the driver can push into `set_preferred_openresponses_target`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModelRoute {
    pub route: String,
    pub endpoint: String,
    pub model: String,
}

/// Models palette mode — builds `PaletteEntry`s from a loaded catalog
/// and resolves a selected string back to an endpoint + model pair.
///
/// `current_*` fields track what the driver set last so the palette
/// can chip the current route and so typed custom routes survive
/// re-entering the palette.
#[derive(Debug, Clone, Default)]
pub struct ModelsMode {
    pub routes: Vec<ModelRoute>,
    pub provider_endpoints: BTreeMap<String, String>,
    pub current_route: Option<String>,
    pub current_endpoint: Option<String>,
    pub current_model: Option<String>,
}

impl ModelsMode {
    pub fn new(
        routes: Vec<ModelRoute>,
        provider_endpoints: BTreeMap<String, String>,
        current_route: Option<String>,
        current_endpoint: Option<String>,
        current_model: Option<String>,
    ) -> Self {
        Self {
            routes,
            provider_endpoints,
            current_route,
            current_endpoint,
            current_model,
        }
    }

    pub fn resolve_selection(&self, selection: &str) -> Result<ResolvedModelRoute, String> {
        if let Some(record) = self.routes.iter().find(|record| record.route == selection) {
            return Ok(ResolvedModelRoute {
                route: record.route.clone(),
                endpoint: record.endpoint.clone(),
                model: record.model_id.clone(),
            });
        }

        let Some((provider_id, model_id)) = parse_model_route(selection) else {
            return Err("model route must look like provider/model_id".to_string());
        };
        let endpoint = self
            .provider_endpoints
            .get(&provider_id)
            .cloned()
            .or_else(|| default_endpoint_for_provider(&provider_id))
            .ok_or_else(|| format!("provider '{provider_id}' is not configured"))?;
        Ok(ResolvedModelRoute {
            route: selection.trim().to_string(),
            endpoint,
            model: model_id,
        })
    }

    pub fn record_resolution(&mut self, resolved: &ResolvedModelRoute) {
        self.current_route = Some(resolved.route.clone());
        self.current_endpoint = Some(resolved.endpoint.clone());
        self.current_model = Some(resolved.model.clone());
    }
}

impl PaletteSource for ModelsMode {
    fn id(&self) -> &'static str {
        "models"
    }

    fn label(&self) -> &str {
        "Models"
    }

    fn placeholder(&self) -> &str {
        "type to filter or enter provider/model_id"
    }

    fn entries(&self) -> Vec<PaletteEntry> {
        let mut routes = self.routes.clone();
        let current = self.current_route.as_deref();
        routes.sort_by(|a, b| {
            let a_current = current == Some(a.route.as_str());
            let b_current = current == Some(b.route.as_str());
            b_current
                .cmp(&a_current)
                .then_with(|| a.provider_id.cmp(&b.provider_id))
                .then_with(|| a.model_id.cmp(&b.model_id))
        });

        routes
            .into_iter()
            .map(|record| {
                let mut chips = Vec::new();
                if current == Some(record.route.as_str()) {
                    chips.push("active".to_string());
                }
                for source in &record.sources {
                    if let Some(chip) = source_chip(source) {
                        if !chips.iter().any(|value| value == chip) {
                            chips.push(chip.to_string());
                        }
                    }
                }
                if record.variants > 0 {
                    chips.push(format!("variants:{}", record.variants));
                }
                PaletteEntry {
                    value: record.route.clone(),
                    title: record.route,
                    subtitle: record.label,
                    chips,
                }
            })
            .collect()
    }

    fn empty_state(&self) -> &str {
        "No configured model routes. Type provider/model_id to use a custom route."
    }

    fn allow_custom(&self) -> Option<&str> {
        Some("Use typed route")
    }
}

pub fn parse_model_route(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    let (provider_id, model_id) = trimmed.split_once('/')?;
    let provider_id = provider_id.trim();
    let model_id = model_id.trim();
    if provider_id.is_empty() || model_id.is_empty() {
        return None;
    }
    Some((provider_id.to_string(), model_id.to_string()))
}

pub fn default_endpoint_for_provider(provider_id: &str) -> Option<String> {
    match provider_id {
        "openai" => Some("https://api.openai.com/v1/responses".to_string()),
        "openrouter" => Some("https://openrouter.ai/api/v1/responses".to_string()),
        _ => None,
    }
}

pub fn infer_provider_id_from_endpoint(endpoint: &str) -> Option<String> {
    if endpoint.contains("openrouter.ai") {
        Some("openrouter".to_string())
    } else if endpoint.contains("api.openai.com") || endpoint.contains("openai.com") {
        Some("openai".to_string())
    } else {
        None
    }
}

pub fn upsert_model_route(
    routes_by_value: &mut BTreeMap<String, ModelRoute>,
    provider_id: &str,
    model_id: &str,
    endpoint: &str,
    label: Option<String>,
    variants: usize,
    source: &str,
) {
    let route = format!("{provider_id}/{model_id}");
    let record = routes_by_value
        .entry(route.clone())
        .or_insert_with(|| ModelRoute {
            route: route.clone(),
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
            endpoint: endpoint.to_string(),
            label: None,
            variants: 0,
            sources: Vec::new(),
        });
    if record.label.is_none() && label.is_some() {
        record.label = label;
    }
    record.variants = record.variants.max(variants);
    if !record.sources.iter().any(|value| value == source) {
        record.sources.push(source.to_string());
    }
}

pub fn push_route_from_string(
    routes_by_value: &mut BTreeMap<String, ModelRoute>,
    provider_endpoints: &BTreeMap<String, String>,
    raw_route: &str,
    source: &str,
) {
    let Some((provider_id, model_id)) = parse_model_route(raw_route) else {
        return;
    };
    let Some(endpoint) = provider_endpoints
        .get(&provider_id)
        .cloned()
        .or_else(|| default_endpoint_for_provider(&provider_id))
    else {
        return;
    };

    upsert_model_route(
        routes_by_value,
        &provider_id,
        &model_id,
        &endpoint,
        None,
        0,
        source,
    );
}

fn source_chip(source: &str) -> Option<&'static str> {
    match source {
        "catalog" => Some("catalog"),
        "config:model" => Some("default"),
        "config:roles.primary" => Some("primary"),
        "config:small_model" => Some("small"),
        "current" => None,
        _ if source.starts_with("config:roles.") => Some("role"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoints() -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        map.insert(
            "openrouter".to_string(),
            "https://openrouter.ai/api/v1/responses".to_string(),
        );
        map.insert(
            "openai".to_string(),
            "https://api.openai.com/v1/responses".to_string(),
        );
        map
    }

    fn route(
        provider_id: &str,
        model_id: &str,
        endpoint: &str,
        label: Option<&str>,
        sources: &[&str],
    ) -> ModelRoute {
        ModelRoute {
            route: format!("{provider_id}/{model_id}"),
            provider_id: provider_id.to_string(),
            model_id: model_id.to_string(),
            endpoint: endpoint.to_string(),
            label: label.map(ToString::to_string),
            variants: 0,
            sources: sources.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn entries_put_current_first_and_include_chips() {
        let mode = ModelsMode::new(
            vec![
                route(
                    "openai",
                    "gpt-5-nano-2025-08-07",
                    "https://api.openai.com/v1/responses",
                    Some("OpenAI"),
                    &["catalog"],
                ),
                route(
                    "openrouter",
                    "openai/gpt-oss-20b",
                    "https://openrouter.ai/api/v1/responses",
                    None,
                    &["catalog", "config:roles.primary"],
                ),
            ],
            endpoints(),
            Some("openrouter/openai/gpt-oss-20b".to_string()),
            None,
            None,
        );
        let entries = mode.entries();
        assert_eq!(entries[0].title, "openrouter/openai/gpt-oss-20b");
        assert!(entries[0].chips.contains(&"active".to_string()));
        assert!(entries[0].chips.contains(&"catalog".to_string()));
        assert!(entries[0].chips.contains(&"primary".to_string()));
        assert_eq!(entries[1].subtitle.as_deref(), Some("OpenAI"));
    }

    #[test]
    fn resolve_selection_prefers_known_routes() {
        let mode = ModelsMode::new(
            vec![route(
                "openai",
                "gpt-5-nano-2025-08-07",
                "https://api.openai.com/v1/responses",
                None,
                &["catalog"],
            )],
            endpoints(),
            None,
            None,
            None,
        );
        let resolved = mode
            .resolve_selection("openai/gpt-5-nano-2025-08-07")
            .expect("resolves");
        assert_eq!(resolved.endpoint, "https://api.openai.com/v1/responses");
        assert_eq!(resolved.model, "gpt-5-nano-2025-08-07");
    }

    #[test]
    fn resolve_selection_falls_back_to_default_provider_endpoint() {
        let mode = ModelsMode::default();
        let resolved = mode
            .resolve_selection("openrouter/openai/gpt-oss-20b")
            .expect("resolves via default");
        assert_eq!(resolved.endpoint, "https://openrouter.ai/api/v1/responses");
        assert_eq!(resolved.model, "openai/gpt-oss-20b");
    }

    #[test]
    fn resolve_selection_errors_for_unknown_provider_without_default() {
        let mode = ModelsMode::default();
        let err = mode
            .resolve_selection("mystery/model")
            .expect_err("unknown provider");
        assert!(err.contains("mystery"));
    }

    #[test]
    fn allow_custom_is_on_and_empty_state_guides_typed_routes() {
        let mode = ModelsMode::default();
        assert_eq!(mode.allow_custom(), Some("Use typed route"));
        assert!(mode.empty_state().contains("provider/model_id"));
    }
}
