use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RipConfig {
    #[serde(rename = "$schema")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,

    #[serde(default)]
    pub provider: BTreeMap<String, ProviderConfig>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small_model: Option<String>,

    #[serde(default)]
    pub roles: BTreeMap<String, ModelRoute>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openresponses: Option<OpenResponsesDefaults>,

    #[serde(default, flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<ApiKeySource>,

    #[serde(default)]
    pub headers: BTreeMap<String, String>,

    #[serde(default)]
    pub models: BTreeMap<String, ModelConfig>,

    #[serde(default, flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    #[serde(default)]
    pub variants: BTreeMap<String, Value>,

    #[serde(default, flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ApiKeySource {
    Inline(String),
    Env { env: String },
}

impl ApiKeySource {
    pub fn resolve(&self) -> Option<String> {
        match self {
            Self::Inline(value) => (!value.trim().is_empty()).then(|| value.clone()),
            Self::Env { env } => env_var(env).filter(|v| !v.trim().is_empty()),
        }
    }

    pub fn description(&self) -> String {
        match self {
            Self::Inline(_) => "inline".to_string(),
            Self::Env { env } => format!("env:{env}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModelRoute {
    String(String),
    Object(ModelRouteObject),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouteObject {
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

impl ModelRoute {
    pub fn to_route_string(&self) -> Option<String> {
        match self {
            Self::String(value) => Some(value.clone()),
            Self::Object(obj) => Some(format!(
                "{}/{}{}",
                obj.provider,
                obj.model,
                obj.variant
                    .as_deref()
                    .filter(|v| !v.trim().is_empty())
                    .map(|v| format!("#{v}"))
                    .unwrap_or_default()
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenResponsesDefaults {
    #[serde(default)]
    pub stateless_history: bool,
    #[serde(default)]
    pub parallel_tool_calls: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub followup_user_message: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct OpenResponsesOverrideInput {
    pub endpoint: Option<String>,
    pub model: Option<String>,
    pub stateless_history: Option<bool>,
    pub parallel_tool_calls: Option<bool>,
    pub followup_user_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OpenResponsesResolvedConfig {
    pub provider_id: Option<String>,
    pub route: Option<String>,
    pub endpoint: String,
    pub model: Option<String>,
    pub headers: Vec<(String, String)>,
    pub api_key: Option<String>,
    pub api_key_source: Option<String>,
    pub stateless_history: bool,
    pub parallel_tool_calls: bool,
    pub followup_user_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub config: RipConfig,
    pub sources: Vec<ConfigSourceReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigSourceReport {
    pub path: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn load_effective_config(workspace_root: &Path) -> LoadedConfig {
    let mut sources: Vec<(PathBuf, String)> = Vec::new();

    #[cfg(not(test))]
    {
        if let Some(global_dir) = global_config_dir() {
            sources.push((global_dir.join("config.jsonc"), "global".to_string()));
            sources.push((global_dir.join("config.json"), "global".to_string()));
        }

        if let Ok(custom) = std::env::var("RIP_CONFIG") {
            if !custom.trim().is_empty() {
                sources.push((PathBuf::from(custom), "custom".to_string()));
            }
        }
    }

    let project_sources = find_project_configs(workspace_root);
    sources.extend(
        project_sources
            .into_iter()
            .map(|path| (path, "project".to_string())),
    );

    let mut merged = Value::Object(serde_json::Map::new());
    let mut reports = Vec::new();
    for (path, kind) in sources {
        if !path.exists() {
            reports.push(ConfigSourceReport {
                path: path.display().to_string(),
                status: "missing".to_string(),
                error: None,
            });
            continue;
        }

        match std::fs::read_to_string(&path) {
            Ok(contents) => match parse_jsonc(&contents) {
                Ok(value) => {
                    merge_json_value(&mut merged, &value);
                    reports.push(ConfigSourceReport {
                        path: path.display().to_string(),
                        status: format!("loaded:{kind}"),
                        error: None,
                    });
                }
                Err(err) => {
                    reports.push(ConfigSourceReport {
                        path: path.display().to_string(),
                        status: format!("invalid:{kind}"),
                        error: Some(err),
                    });
                }
            },
            Err(err) => {
                reports.push(ConfigSourceReport {
                    path: path.display().to_string(),
                    status: format!("unreadable:{kind}"),
                    error: Some(err.to_string()),
                });
            }
        }
    }

    let config = serde_json::from_value::<RipConfig>(merged.clone()).unwrap_or_default();
    LoadedConfig {
        config,
        sources: reports,
    }
}

pub fn resolve_openresponses_config(
    workspace_root: &Path,
    overrides: OpenResponsesOverrideInput,
) -> (Option<OpenResponsesResolvedConfig>, LoadedConfig) {
    let loaded = load_effective_config(workspace_root);
    let config = &loaded.config;

    let env_endpoint = env_var("RIP_OPENRESPONSES_ENDPOINT")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let default_route = config
        .roles
        .get("primary")
        .and_then(|route| route.to_route_string())
        .or_else(|| config.model.clone());

    let parsed_route = default_route
        .as_deref()
        .and_then(|route| parse_route_string(route).ok());

    let default_provider = parsed_route
        .as_ref()
        .and_then(|route| config.provider.get(&route.provider_id));
    let default_endpoint = default_provider
        .and_then(|provider| provider.endpoint.clone())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let endpoint = overrides
        .endpoint
        .clone()
        .or(env_endpoint)
        .or(default_endpoint);
    let Some(endpoint) = endpoint else {
        return (None, loaded);
    };

    let provider_match = if let Some(route) = parsed_route.as_ref() {
        config
            .provider
            .get(&route.provider_id)
            .map(|provider| (route.provider_id.clone(), provider))
    } else {
        find_provider_by_endpoint(config, &endpoint)
    };

    let (provider_id, provider_cfg) = match provider_match {
        Some((id, cfg)) => (Some(id), Some(cfg)),
        None => (None, None),
    };

    let headers = provider_cfg
        .map(|provider| provider.headers.clone())
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>();

    let mut api_key: Option<String> = None;
    let mut api_key_source: Option<String> = None;
    if let Some(provider) = provider_cfg {
        if let Some(source) = provider.api_key.as_ref() {
            api_key = source.resolve();
            api_key_source = Some(source.description());
        }
    }

    if api_key.is_none() {
        let (key, source) = resolve_api_key_from_env(&endpoint);
        api_key = key;
        api_key_source = source;
    }

    let env_model = env_var("RIP_OPENRESPONSES_MODEL")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let model = overrides.model.clone().or(env_model).or_else(|| {
        parsed_route
            .as_ref()
            .map(|route| route.model_id.clone())
            .filter(|value| !value.trim().is_empty())
    });

    let mut defaults = config.openresponses.clone().unwrap_or_default();
    if let Some(stateless) = parse_env_bool("RIP_OPENRESPONSES_STATELESS_HISTORY") {
        defaults.stateless_history = stateless;
    }
    if let Some(parallel) = parse_env_bool("RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS") {
        defaults.parallel_tool_calls = parallel;
    }
    if let Some(value) = env_var("RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE") {
        let trimmed = value.trim().to_string();
        if !trimmed.is_empty() {
            defaults.followup_user_message = Some(trimmed);
        }
    }

    if let Some(stateless_history) = overrides.stateless_history {
        defaults.stateless_history = stateless_history;
    }
    if let Some(parallel_tool_calls) = overrides.parallel_tool_calls {
        defaults.parallel_tool_calls = parallel_tool_calls;
    }
    if overrides.followup_user_message.is_some() {
        defaults.followup_user_message = overrides.followup_user_message.clone();
    }

    (
        Some(OpenResponsesResolvedConfig {
            provider_id: provider_id.clone(),
            route: default_route,
            endpoint,
            model,
            headers,
            api_key,
            api_key_source,
            stateless_history: defaults.stateless_history,
            parallel_tool_calls: defaults.parallel_tool_calls,
            followup_user_message: defaults.followup_user_message,
        }),
        loaded,
    )
}

#[cfg(not(test))]
fn global_config_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("RIP_CONFIG_HOME") {
        if !dir.trim().is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".rip"))
}

fn find_project_configs(start: &Path) -> Vec<PathBuf> {
    let stop = find_git_root(start).unwrap_or_else(|| PathBuf::from("/"));
    let mut candidates = Vec::new();

    let mut current = Some(start);
    while let Some(dir) = current {
        for name in ["rip.jsonc", "rip.json"] {
            let path = dir.join(name);
            if path.exists() {
                candidates.push(path);
            }
        }
        if dir == stop {
            break;
        }
        current = dir.parent();
    }

    candidates.reverse();
    candidates
}

fn find_git_root(start: &Path) -> Option<PathBuf> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        current = dir.parent();
    }
    None
}

#[derive(Debug, Clone)]
struct ParsedRoute {
    provider_id: String,
    model_id: String,
    #[allow(dead_code)]
    variant: Option<String>,
}

fn parse_route_string(raw: &str) -> Result<ParsedRoute, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty route".to_string());
    }

    let (route, variant) = match raw.split_once('#') {
        Some((route, variant)) => (route.trim(), Some(variant.trim().to_string())),
        None => (raw, None),
    };

    let (provider_id, model_id) = route
        .split_once('/')
        .ok_or_else(|| "expected route format provider_id/model_id".to_string())?;
    let provider_id = provider_id.trim();
    if provider_id.is_empty() {
        return Err("missing provider_id".to_string());
    }
    let model_id = model_id.trim();
    if model_id.is_empty() {
        return Err("missing model_id".to_string());
    }

    Ok(ParsedRoute {
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        variant: variant.filter(|v| !v.is_empty()),
    })
}

fn find_provider_by_endpoint<'a>(
    config: &'a RipConfig,
    endpoint: &str,
) -> Option<(String, &'a ProviderConfig)> {
    let endpoint = endpoint.trim();
    config
        .provider
        .iter()
        .find(|(_id, provider)| provider.endpoint.as_deref().map(|v| v.trim()) == Some(endpoint))
        .map(|(id, provider)| (id.clone(), provider))
}

fn resolve_api_key_from_env(endpoint: &str) -> (Option<String>, Option<String>) {
    if let Some(value) = env_var("RIP_OPENRESPONSES_API_KEY") {
        if !value.trim().is_empty() {
            return (
                Some(value),
                Some("env:RIP_OPENRESPONSES_API_KEY".to_string()),
            );
        }
    }

    if endpoint.contains("api.openai.com") || endpoint.contains("openai.com") {
        let key = env_var("OPENAI_API_KEY").filter(|v| !v.trim().is_empty());
        return (key, Some("env:OPENAI_API_KEY".to_string()));
    }
    if endpoint.contains("openrouter.ai") {
        let key = env_var("OPENROUTER_API_KEY").filter(|v| !v.trim().is_empty());
        return (key, Some("env:OPENROUTER_API_KEY".to_string()));
    }

    (None, None)
}

fn parse_env_bool(key: &str) -> Option<bool> {
    let value = env_var(key)?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(matches!(
        trimmed.to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    ))
}

fn env_var(key: &str) -> Option<String> {
    #[cfg(test)]
    {
        let _ = key;
        None
    }

    #[cfg(not(test))]
    {
        std::env::var(key).ok()
    }
}

fn parse_jsonc(raw: &str) -> Result<Value, String> {
    let stripped = strip_jsonc_comments(raw);
    let normalized = strip_trailing_commas(&stripped);
    serde_json::from_str(&normalized).map_err(|err| err.to_string())
}

fn strip_jsonc_comments(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();

    let mut in_string = false;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;

    while let Some(ch) = chars.next() {
        if line_comment {
            if ch == '\n' {
                line_comment = false;
                out.push(ch);
            }
            continue;
        }
        if block_comment {
            if ch == '*' && chars.peek() == Some(&'/') {
                let _ = chars.next();
                block_comment = false;
            }
            continue;
        }

        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == '/' {
            match chars.peek() {
                Some('/') => {
                    let _ = chars.next();
                    line_comment = true;
                    continue;
                }
                Some('*') => {
                    let _ = chars.next();
                    block_comment = true;
                    continue;
                }
                _ => {}
            }
        }

        out.push(ch);
    }

    out
}

fn strip_trailing_commas(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();

    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
            continue;
        }

        if ch == '"' {
            in_string = true;
            out.push(ch);
            continue;
        }

        if ch == ',' {
            let mut lookahead = chars.clone();
            while matches!(lookahead.peek(), Some(next) if next.is_whitespace()) {
                let _ = lookahead.next();
            }
            if matches!(lookahead.peek(), Some(']') | Some('}')) {
                continue;
            }
        }

        out.push(ch);
    }

    out
}

fn merge_json_value(target: &mut Value, overlay: &Value) {
    match (target, overlay) {
        (Value::Object(target_obj), Value::Object(overlay_obj)) => {
            for (key, value) in overlay_obj {
                match target_obj.get_mut(key) {
                    Some(existing) => merge_json_value(existing, value),
                    None => {
                        target_obj.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (target_slot, overlay_value) => {
            *target_slot = overlay_value.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_jsonc_comments_preserves_strings() {
        let raw = r#"
        {
          // comment
          "a": "http://example.com//not-comment",
          "b": "/*not a comment*/",
          /* block
             comment */
          "c": 1,
        }
        "#;
        let parsed = parse_jsonc(raw).expect("parse");
        assert_eq!(parsed["a"], "http://example.com//not-comment");
        assert_eq!(parsed["b"], "/*not a comment*/");
        assert_eq!(parsed["c"], 1);
    }

    #[test]
    fn merge_json_value_is_deep_for_objects() {
        let mut target = serde_json::json!({
            "provider": {
                "openrouter": { "endpoint": "a", "headers": { "x": "1" } }
            }
        });
        let overlay = serde_json::json!({
            "provider": {
                "openrouter": { "headers": { "y": "2" } }
            }
        });
        merge_json_value(&mut target, &overlay);
        assert_eq!(target["provider"]["openrouter"]["endpoint"], "a");
        assert_eq!(target["provider"]["openrouter"]["headers"]["x"], "1");
        assert_eq!(target["provider"]["openrouter"]["headers"]["y"], "2");
    }

    #[test]
    fn parse_route_string_accepts_variant_suffix() {
        let parsed = parse_route_string("openrouter/openai/gpt-oss-20b#fast").expect("route");
        assert_eq!(parsed.provider_id, "openrouter");
        assert_eq!(parsed.model_id, "openai/gpt-oss-20b");
        assert_eq!(parsed.variant.as_deref(), Some("fast"));
    }
}
