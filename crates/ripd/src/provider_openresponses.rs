use rip_provider_openresponses::{
    CreateResponseBuilder, CreateResponsePayload, ItemParam, ToolChoiceParam,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningSummary {
    Concise,
    Detailed,
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchContextSize {
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum OpenResponsesInclude {
    #[serde(rename = "file_search_call.results")]
    FileSearchCallResults,
    #[serde(rename = "web_search_call.results")]
    WebSearchCallResults,
    #[serde(rename = "web_search_call.action.sources")]
    WebSearchCallActionSources,
    #[serde(rename = "message.input_image.image_url")]
    MessageInputImageImageUrl,
    #[serde(rename = "computer_call_output.output.image_url")]
    ComputerCallOutputOutputImageUrl,
    #[serde(rename = "code_interpreter_call.outputs")]
    CodeInterpreterCallOutputs,
    #[serde(rename = "reasoning.encrypted_content")]
    ReasoningEncryptedContent,
    #[serde(rename = "message.output_text.logprobs")]
    MessageOutputTextLogprobs,
}

impl OpenResponsesInclude {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::FileSearchCallResults => "file_search_call.results",
            Self::WebSearchCallResults => "web_search_call.results",
            Self::WebSearchCallActionSources => "web_search_call.action.sources",
            Self::MessageInputImageImageUrl => "message.input_image.image_url",
            Self::ComputerCallOutputOutputImageUrl => "computer_call_output.output.image_url",
            Self::CodeInterpreterCallOutputs => "code_interpreter_call.outputs",
            Self::ReasoningEncryptedContent => "reasoning.encrypted_content",
            Self::MessageOutputTextLogprobs => "message.output_text.logprobs",
        }
    }
}

fn default_enabled_true() -> bool {
    true
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OpenResponsesApproximateLocation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

impl OpenResponsesApproximateLocation {
    pub fn is_empty(&self) -> bool {
        self.country
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
            && self
                .region
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            && self
                .city
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
            && self
                .timezone
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
    }

    pub fn normalized(mut self) -> Option<Self> {
        self.country = self
            .country
            .and_then(|value| (!value.trim().is_empty()).then_some(value.trim().to_string()));
        self.region = self
            .region
            .and_then(|value| (!value.trim().is_empty()).then_some(value.trim().to_string()));
        self.city = self
            .city
            .and_then(|value| (!value.trim().is_empty()).then_some(value.trim().to_string()));
        self.timezone = self
            .timezone
            .and_then(|value| (!value.trim().is_empty()).then_some(value.trim().to_string()));
        (!self.is_empty()).then_some(self)
    }

    fn to_value(&self) -> Option<Value> {
        let normalized = self.clone().normalized()?;
        let mut obj = serde_json::Map::new();
        obj.insert("type".to_string(), Value::String("approximate".to_string()));
        if let Some(country) = normalized.country {
            obj.insert("country".to_string(), Value::String(country));
        }
        if let Some(region) = normalized.region {
            obj.insert("region".to_string(), Value::String(region));
        }
        if let Some(city) = normalized.city {
            obj.insert("city".to_string(), Value::String(city));
        }
        if let Some(timezone) = normalized.timezone {
            obj.insert("timezone".to_string(), Value::String(timezone));
        }
        Some(Value::Object(obj))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OpenResponsesWebSearchConfig {
    #[serde(default = "default_enabled_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<SearchContextSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_web_access: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_location: Option<OpenResponsesApproximateLocation>,
}

impl Default for OpenResponsesWebSearchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            search_context_size: None,
            external_web_access: None,
            user_location: None,
        }
    }
}

impl OpenResponsesWebSearchConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }

    pub fn normalized(mut self) -> Option<Self> {
        self.user_location = self
            .user_location
            .and_then(OpenResponsesApproximateLocation::normalized);
        if self.enabled
            || self.search_context_size.is_some()
            || self.external_web_access.is_some()
            || self.user_location.is_some()
        {
            Some(self)
        } else {
            None
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OpenResponsesWebSearchOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_context_size: Option<SearchContextSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_web_access: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_location: Option<OpenResponsesApproximateLocation>,
}

impl OpenResponsesWebSearchOverride {
    pub fn is_empty(&self) -> bool {
        self.enabled.is_none()
            && self.search_context_size.is_none()
            && self.external_web_access.is_none()
            && self.user_location.is_none()
    }

    pub fn apply_to(&self, target: &mut OpenResponsesWebSearchConfig) {
        if let Some(enabled) = self.enabled {
            target.enabled = enabled;
        }
        if let Some(search_context_size) = self.search_context_size {
            target.search_context_size = Some(search_context_size);
        }
        if let Some(external_web_access) = self.external_web_access {
            target.external_web_access = Some(external_web_access);
        }
        if let Some(user_location) = self.user_location.clone() {
            target.user_location = user_location.normalized();
        }
    }

    pub fn into_config(self) -> Option<OpenResponsesWebSearchConfig> {
        if self.is_empty() {
            return None;
        }
        let mut cfg = OpenResponsesWebSearchConfig::default();
        self.apply_to(&mut cfg);
        cfg.normalized()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct OpenResponsesReasoningConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<ReasoningEffort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<ReasoningSummary>,
}

impl OpenResponsesReasoningConfig {
    pub fn is_empty(&self) -> bool {
        self.effort.is_none() && self.summary.is_none()
    }

    pub fn normalized(self) -> Option<Self> {
        (!self.is_empty()).then_some(self)
    }

    fn to_value(&self) -> Option<Value> {
        if self.is_empty() {
            return None;
        }

        let mut obj = serde_json::Map::new();
        if let Some(effort) = self.effort {
            obj.insert(
                "effort".to_string(),
                serde_json::to_value(effort).expect("reasoning effort serializes"),
            );
        }
        if let Some(summary) = self.summary {
            obj.insert(
                "summary".to_string(),
                serde_json::to_value(summary).expect("reasoning summary serializes"),
            );
        }
        Some(Value::Object(obj))
    }
}

#[derive(Clone, Debug)]
pub struct OpenResponsesConfig {
    pub provider_id: Option<String>,
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub headers: Vec<(String, String)>,
    pub tool_choice: ToolChoiceParam,
    pub include: Vec<OpenResponsesInclude>,
    pub web_search: Option<OpenResponsesWebSearchConfig>,
    pub reasoning: Option<OpenResponsesReasoningConfig>,
    pub followup_user_message: Option<String>,
    pub stateless_history: bool,
    pub parallel_tool_calls: bool,
}

impl OpenResponsesConfig {
    #[cfg(not(test))]
    pub fn from_env() -> Option<Self> {
        let endpoint = std::env::var("RIP_OPENRESPONSES_ENDPOINT").ok()?;
        let api_key = std::env::var("RIP_OPENRESPONSES_API_KEY").ok();
        let model = std::env::var("RIP_OPENRESPONSES_MODEL").ok();
        let tool_choice = match std::env::var("RIP_OPENRESPONSES_TOOL_CHOICE") {
            Ok(value) => match parse_tool_choice_env(&value) {
                Ok(choice) => choice,
                Err(err) => {
                    eprintln!(
                        "invalid RIP_OPENRESPONSES_TOOL_CHOICE={value:?}: {err}; defaulting to auto"
                    );
                    ToolChoiceParam::auto()
                }
            },
            Err(_) => ToolChoiceParam::auto(),
        };
        let include = openresponses_include_from_env();
        let web_search = openresponses_web_search_from_env();
        let reasoning = openresponses_reasoning_from_env();
        let followup_user_message = std::env::var("RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE").ok();
        let stateless_history = std::env::var("RIP_OPENRESPONSES_STATELESS_HISTORY")
            .ok()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);
        let parallel_tool_calls = std::env::var("RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS")
            .ok()
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);
        Some(Self {
            provider_id: None,
            endpoint,
            api_key,
            model,
            headers: Vec::new(),
            tool_choice,
            include,
            web_search,
            reasoning,
            followup_user_message,
            stateless_history,
            parallel_tool_calls,
        })
    }
}

pub const DEFAULT_MAX_TOOL_CALLS: u64 = 32;
pub const DEFAULT_OPENROUTER_MODEL: &str = "openai/gpt-oss-20b";

pub fn build_streaming_request(
    config: &OpenResponsesConfig,
    prompt: &str,
) -> CreateResponsePayload {
    let tools = tools_for_request(config);
    let mut builder = base_streaming_builder(config)
        .input_text(prompt)
        .tools_raw(tools)
        .tool_choice(config.tool_choice.clone())
        .parallel_tool_calls(config.parallel_tool_calls)
        .max_tool_calls(DEFAULT_MAX_TOOL_CALLS);
    builder = builder.insert_raw("stream", Value::Bool(true));
    builder.build()
}

pub fn build_streaming_request_items(
    config: &OpenResponsesConfig,
    items: Vec<ItemParam>,
) -> CreateResponsePayload {
    let tools = tools_for_request(config);
    let mut builder = base_streaming_builder(config)
        .input_items(items)
        .tools_raw(tools)
        .tool_choice(config.tool_choice.clone())
        .parallel_tool_calls(config.parallel_tool_calls)
        .max_tool_calls(DEFAULT_MAX_TOOL_CALLS);
    builder = builder.insert_raw("stream", Value::Bool(true));
    builder.build()
}

pub fn build_streaming_followup_request(
    config: &OpenResponsesConfig,
    previous_response_id: Option<&str>,
    mut input_items: Vec<ItemParam>,
) -> CreateResponsePayload {
    if let Some(message) = config.followup_user_message.as_deref() {
        input_items.push(ItemParam::user_message_text(message));
    }
    let tools = tools_for_request(config);
    let mut builder = base_streaming_builder(config);
    if let Some(previous_response_id) = previous_response_id {
        builder = builder.insert_raw(
            "previous_response_id",
            Value::String(previous_response_id.to_string()),
        );
    }
    let builder = builder
        .input_items(input_items)
        .tools_raw(tools)
        .tool_choice(config.tool_choice.clone())
        .parallel_tool_calls(config.parallel_tool_calls)
        .max_tool_calls(DEFAULT_MAX_TOOL_CALLS)
        .insert_raw("stream", Value::Bool(true));
    builder.build()
}

fn base_streaming_builder(config: &OpenResponsesConfig) -> CreateResponseBuilder {
    let mut builder = match config.model.as_deref() {
        Some(model) => CreateResponseBuilder::new().model(model.to_string()),
        None if config.provider_id.as_deref() == Some("openrouter")
            || is_openrouter_responses_endpoint(&config.endpoint) =>
        {
            CreateResponseBuilder::new().model(DEFAULT_OPENROUTER_MODEL.to_string())
        }
        None => CreateResponseBuilder::new(),
    };

    let effective_include = effective_include(config);
    if !effective_include.is_empty() {
        builder = builder.insert_raw(
            "include",
            Value::Array(
                effective_include
                    .into_iter()
                    .map(|value| Value::String(value.as_str().to_string()))
                    .collect(),
            ),
        );
    }

    if let Some(reasoning) = effective_reasoning(config).and_then(|reasoning| reasoning.to_value())
    {
        builder = builder.insert_raw("reasoning", reasoning);
    }

    builder
}

fn tools_for_request(config: &OpenResponsesConfig) -> Vec<Value> {
    let mut tools = builtin_function_tools();
    if let Some(tool) = effective_web_search(config).and_then(|cfg| web_search_tool_value(&cfg)) {
        tools.push(tool);
    }
    tools
}

fn effective_include(config: &OpenResponsesConfig) -> Vec<OpenResponsesInclude> {
    crate::openresponses_compat::resolve_openresponses_compat_profile(
        config.provider_id.as_deref(),
        &config.endpoint,
        config.model.as_deref(),
    )
    .include(&config.include)
    .effective
}

fn effective_reasoning(config: &OpenResponsesConfig) -> Option<OpenResponsesReasoningConfig> {
    crate::openresponses_compat::resolve_openresponses_compat_profile(
        config.provider_id.as_deref(),
        &config.endpoint,
        config.model.as_deref(),
    )
    .reasoning(config.reasoning.as_ref())
    .effective
}

fn effective_web_search(config: &OpenResponsesConfig) -> Option<OpenResponsesWebSearchConfig> {
    crate::openresponses_compat::resolve_openresponses_compat_profile(
        config.provider_id.as_deref(),
        &config.endpoint,
        config.model.as_deref(),
    )
    .web_search(config.web_search.as_ref())
    .effective
    .filter(OpenResponsesWebSearchConfig::is_enabled)
}

fn web_search_tool_value(web_search: &OpenResponsesWebSearchConfig) -> Option<Value> {
    let mut obj = serde_json::Map::new();
    obj.insert("type".to_string(), Value::String("web_search".to_string()));
    if let Some(search_context_size) = web_search.search_context_size {
        obj.insert(
            "search_context_size".to_string(),
            serde_json::to_value(search_context_size).expect("search_context_size serializes"),
        );
    }
    if let Some(external_web_access) = web_search.external_web_access {
        obj.insert(
            "external_web_access".to_string(),
            Value::Bool(external_web_access),
        );
    }
    if let Some(user_location) = web_search
        .user_location
        .as_ref()
        .and_then(OpenResponsesApproximateLocation::to_value)
    {
        obj.insert("user_location".to_string(), user_location);
    }

    Some(Value::Object(obj))
}

pub(crate) fn is_openrouter_responses_endpoint(endpoint: &str) -> bool {
    // NOTE: This is intentionally a strict-ish heuristic: we only apply this default to the
    // canonical OpenRouter OpenAI-compatible Responses endpoint.
    let raw = endpoint.trim();
    raw == "https://openrouter.ai/api/v1/responses"
        || raw == "https://openrouter.ai/api/v1/responses/"
}

pub(crate) fn parse_tool_choice_env(value: &str) -> Result<ToolChoiceParam, String> {
    let raw = value.trim();
    if raw.is_empty() {
        return Ok(ToolChoiceParam::auto());
    }

    match raw {
        "auto" => Ok(ToolChoiceParam::auto()),
        "none" => Ok(ToolChoiceParam::none()),
        "required" => Ok(ToolChoiceParam::required()),
        _ => {
            if let Some(rest) = raw.strip_prefix("function:") {
                let name = rest.trim();
                if name.is_empty() {
                    return Err("function name missing (expected function:<name>)".to_string());
                }
                return Ok(ToolChoiceParam::specific_function(name.to_string()));
            }

            let (is_json, json) = match raw.strip_prefix("json:") {
                Some(json) => (true, json.trim()),
                None => (
                    raw.starts_with('{') || raw.starts_with('[') || raw.starts_with('"'),
                    raw,
                ),
            };

            if is_json {
                let parsed: Value = serde_json::from_str(json)
                    .map_err(|err| format!("invalid json tool_choice: {err}"))?;
                let param = ToolChoiceParam::new(parsed);
                if !param.errors().is_empty() {
                    return Err(format!(
                        "invalid tool_choice: {}",
                        param.errors().join("; ")
                    ));
                }
                return Ok(param);
            }

            Err("unsupported value (expected auto|none|required|function:<name>|json:<tool_choice_json>)"
                .to_string())
        }
    }
}

pub fn parse_reasoning_effort(value: &str) -> Result<ReasoningEffort, String> {
    match value.trim() {
        "none" => Ok(ReasoningEffort::None),
        "minimal" => Ok(ReasoningEffort::Minimal),
        "low" => Ok(ReasoningEffort::Low),
        "medium" => Ok(ReasoningEffort::Medium),
        "high" => Ok(ReasoningEffort::High),
        "xhigh" => Ok(ReasoningEffort::Xhigh),
        _ => Err("unsupported value (expected none|minimal|low|medium|high|xhigh)".to_string()),
    }
}

pub fn parse_reasoning_summary(value: &str) -> Result<ReasoningSummary, String> {
    match value.trim() {
        "concise" => Ok(ReasoningSummary::Concise),
        "detailed" => Ok(ReasoningSummary::Detailed),
        "auto" => Ok(ReasoningSummary::Auto),
        _ => Err("unsupported value (expected concise|detailed|auto)".to_string()),
    }
}

pub fn parse_search_context_size(value: &str) -> Result<SearchContextSize, String> {
    match value.trim() {
        "low" => Ok(SearchContextSize::Low),
        "medium" => Ok(SearchContextSize::Medium),
        "high" => Ok(SearchContextSize::High),
        _ => Err("unsupported value (expected low|medium|high)".to_string()),
    }
}

pub fn parse_openresponses_include(value: &str) -> Result<OpenResponsesInclude, String> {
    match value.trim() {
        "file_search_call.results" => Ok(OpenResponsesInclude::FileSearchCallResults),
        "web_search_call.results" => Ok(OpenResponsesInclude::WebSearchCallResults),
        "web_search_call.action.sources" => Ok(OpenResponsesInclude::WebSearchCallActionSources),
        "message.input_image.image_url" => Ok(OpenResponsesInclude::MessageInputImageImageUrl),
        "computer_call_output.output.image_url" => {
            Ok(OpenResponsesInclude::ComputerCallOutputOutputImageUrl)
        }
        "code_interpreter_call.outputs" => Ok(OpenResponsesInclude::CodeInterpreterCallOutputs),
        "reasoning.encrypted_content" => Ok(OpenResponsesInclude::ReasoningEncryptedContent),
        "message.output_text.logprobs" => Ok(OpenResponsesInclude::MessageOutputTextLogprobs),
        _ => Err("unsupported value (expected a canonical OpenResponses include path)".to_string()),
    }
}

pub fn parse_openresponses_include_list(value: &str) -> Result<Vec<OpenResponsesInclude>, String> {
    let mut out = Vec::new();
    for raw in value.split(',') {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let include = parse_openresponses_include(trimmed)?;
        if !out.contains(&include) {
            out.push(include);
        }
    }
    Ok(out)
}

#[cfg(not(test))]
fn openresponses_include_from_env() -> Vec<OpenResponsesInclude> {
    std::env::var("RIP_OPENRESPONSES_INCLUDE")
        .ok()
        .and_then(|value| match parse_openresponses_include_list(&value) {
            Ok(include) => Some(include),
            Err(err) => {
                eprintln!("invalid RIP_OPENRESPONSES_INCLUDE={value:?}: {err}; ignoring");
                None
            }
        })
        .unwrap_or_default()
}

#[cfg(not(test))]
fn openresponses_web_search_from_env() -> Option<OpenResponsesWebSearchConfig> {
    let mut web_search = OpenResponsesWebSearchConfig::default();
    let mut seen = false;

    if let Ok(value) = std::env::var("RIP_OPENRESPONSES_WEB_SEARCH") {
        web_search.enabled = matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        );
        seen = true;
    }

    if let Ok(value) = std::env::var("RIP_OPENRESPONSES_WEB_SEARCH_CONTEXT_SIZE") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            match parse_search_context_size(trimmed) {
                Ok(search_context_size) => {
                    web_search.search_context_size = Some(search_context_size);
                    seen = true;
                }
                Err(err) => eprintln!(
                    "invalid RIP_OPENRESPONSES_WEB_SEARCH_CONTEXT_SIZE={trimmed:?}: {err}; ignoring"
                ),
            }
        }
    }

    if let Ok(value) = std::env::var("RIP_OPENRESPONSES_WEB_SEARCH_EXTERNAL_WEB_ACCESS") {
        web_search.external_web_access = Some(matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ));
        seen = true;
    }

    if !seen {
        return None;
    }

    web_search.normalized()
}

#[cfg(not(test))]
fn openresponses_reasoning_from_env() -> Option<OpenResponsesReasoningConfig> {
    let mut reasoning = OpenResponsesReasoningConfig::default();

    if let Ok(value) = std::env::var("RIP_OPENRESPONSES_REASONING_EFFORT") {
        match parse_reasoning_effort(&value) {
            Ok(effort) => reasoning.effort = Some(effort),
            Err(err) => {
                eprintln!("invalid RIP_OPENRESPONSES_REASONING_EFFORT={value:?}: {err}; ignoring")
            }
        }
    }

    if let Ok(value) = std::env::var("RIP_OPENRESPONSES_REASONING_SUMMARY") {
        match parse_reasoning_summary(&value) {
            Ok(summary) => reasoning.summary = Some(summary),
            Err(err) => {
                eprintln!("invalid RIP_OPENRESPONSES_REASONING_SUMMARY={value:?}: {err}; ignoring")
            }
        }
    }

    reasoning.normalized()
}

fn builtin_function_tools() -> Vec<Value> {
    vec![
        function_tool(
            "read",
            "Read a file from the workspace (optionally with line ranges).",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "start_line": { "type": "integer", "minimum": 1 },
                    "end_line": { "type": "integer", "minimum": 1 },
                    "max_bytes": { "type": "integer", "minimum": 1 }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        ),
        function_tool(
            "write",
            "Write a file in the workspace (atomic by default).",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" },
                    "append": { "type": "boolean" },
                    "create": { "type": "boolean" },
                    "atomic": { "type": "boolean" }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
        ),
        function_tool(
            "apply_patch",
            "Apply a patch to the workspace using the Codex apply_patch envelope.",
            json!({
                "type": "object",
                "properties": {
                    "patch": { "type": "string" }
                },
                "required": ["patch"],
                "additionalProperties": false
            }),
        ),
        function_tool(
            "ls",
            "List files under a path in the workspace (supports globs, recursion, and excludes).",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "recursive": { "type": "boolean" },
                    "max_depth": { "type": "integer", "minimum": 1 },
                    "include": { "type": "array", "items": { "type": "string" } },
                    "exclude": { "type": "array", "items": { "type": "string" } },
                    "include_hidden": { "type": "boolean" },
                    "follow_symlinks": { "type": "boolean" }
                },
                "additionalProperties": false
            }),
        ),
        function_tool(
            "grep",
            "Search text in workspace files (supports regex and globs).",
            json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" },
                    "regex": { "type": "boolean" },
                    "case_sensitive": { "type": "boolean" },
                    "include": { "type": "array", "items": { "type": "string" } },
                    "exclude": { "type": "array", "items": { "type": "string" } },
                    "max_results": { "type": "integer", "minimum": 1 },
                    "max_bytes": { "type": "integer", "minimum": 1 },
                    "max_depth": { "type": "integer", "minimum": 1 },
                    "include_hidden": { "type": "boolean" },
                    "follow_symlinks": { "type": "boolean" }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
        ),
        function_tool(
            "artifact_fetch",
            "Fetch a stored artifact by id (sha256). Tool outputs may reference artifacts when output is too large to inline.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "offset_bytes": { "type": "integer", "minimum": 0 },
                    "max_bytes": { "type": "integer", "minimum": 1 }
                },
                "required": ["id"],
                "additionalProperties": false
            }),
        ),
        function_tool(
            "bash",
            "Run a shell command (bash -c) with optional cwd and env (paths are workspace-relative).",
            json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "cwd": { "type": "string" },
                    "env": { "type": "object", "additionalProperties": { "type": "string" } },
                    "max_bytes": { "type": "integer", "minimum": 1 }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        ),
        function_tool(
            "shell",
            "Alias of bash (bash -c).",
            json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "cwd": { "type": "string" },
                    "env": { "type": "object", "additionalProperties": { "type": "string" } },
                    "max_bytes": { "type": "integer", "minimum": 1 }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
        ),
    ]
}

fn function_tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({
        "type": "function",
        "name": name,
        "description": description,
        "parameters": parameters,
        "strict": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_followup(followup_user_message: Option<String>) -> OpenResponsesConfig {
        OpenResponsesConfig {
            provider_id: None,
            endpoint: "http://example.test/v1/responses".to_string(),
            api_key: None,
            model: None,
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: None,
            web_search: None,
            followup_user_message,
            stateless_history: false,
            parallel_tool_calls: false,
        }
    }

    #[test]
    fn followup_request_without_user_message() {
        let config = config_with_followup(None);
        let payload = build_streaming_followup_request(
            &config,
            Some("resp_1"),
            vec![ItemParam::function_call_output(
                "call_1",
                Value::String("ok".to_string()),
            )],
        );
        let input = payload
            .body()
            .get("input")
            .and_then(|value| value.as_array())
            .expect("input array");
        assert_eq!(input.len(), 1);
        assert_eq!(
            input[0].get("type").and_then(|value| value.as_str()),
            Some("function_call_output")
        );
    }

    #[test]
    fn followup_request_appends_user_message_when_configured() {
        let config = config_with_followup(Some("continue".to_string()));
        let payload = build_streaming_followup_request(
            &config,
            Some("resp_1"),
            vec![ItemParam::function_call_output(
                "call_1",
                Value::String("ok".to_string()),
            )],
        );
        let input = payload
            .body()
            .get("input")
            .and_then(|value| value.as_array())
            .expect("input array");
        assert_eq!(input.len(), 2);
        assert_eq!(
            input[1].get("type").and_then(|value| value.as_str()),
            Some("message")
        );
        assert_eq!(
            input[1].get("role").and_then(|value| value.as_str()),
            Some("user")
        );
        assert_eq!(
            input[1].get("content").and_then(|value| value.as_str()),
            Some("continue")
        );
    }

    #[test]
    fn followup_request_omits_previous_response_id_when_none() {
        let config = config_with_followup(None);
        let payload = build_streaming_followup_request(
            &config,
            None,
            vec![ItemParam::function_call_output(
                "call_1",
                Value::String("ok".to_string()),
            )],
        );
        assert!(payload.body().get("previous_response_id").is_none());
    }

    #[test]
    fn followup_request_includes_previous_response_id_when_present() {
        let config = config_with_followup(None);
        let payload = build_streaming_followup_request(
            &config,
            Some("resp_prev"),
            vec![ItemParam::function_call_output(
                "call_1",
                Value::String("ok".to_string()),
            )],
        );
        assert_eq!(
            payload
                .body()
                .get("previous_response_id")
                .and_then(|value| value.as_str()),
            Some("resp_prev")
        );
    }

    #[test]
    fn parse_tool_choice_env_defaults_to_auto() {
        let parsed = parse_tool_choice_env("   ").expect("auto");
        assert_eq!(parsed.value(), ToolChoiceParam::auto().value());
    }

    #[test]
    fn parse_tool_choice_env_accepts_named_function() {
        let parsed = parse_tool_choice_env("function:ls").expect("function");
        let value = parsed.value();
        assert_eq!(value.get("type").and_then(|v| v.as_str()), Some("function"));
        assert_eq!(value.get("name").and_then(|v| v.as_str()), Some("ls"));
    }

    #[test]
    fn parse_tool_choice_env_accepts_builtin_modes() {
        assert_eq!(
            parse_tool_choice_env("none").expect("none").value(),
            ToolChoiceParam::none().value()
        );
        assert_eq!(
            parse_tool_choice_env("required").expect("required").value(),
            ToolChoiceParam::required().value()
        );
    }

    #[test]
    fn parse_tool_choice_env_rejects_missing_function_name() {
        let err = parse_tool_choice_env("function:   ").unwrap_err();
        assert!(err.contains("function name missing"));
    }

    #[test]
    fn parse_tool_choice_env_accepts_json() {
        let parsed =
            parse_tool_choice_env(r#"json:{"type":"function","name":"ls"}"#).expect("json");
        assert_eq!(
            parsed.value().get("type").and_then(|v| v.as_str()),
            Some("function")
        );
        assert_eq!(
            parsed.value().get("name").and_then(|v| v.as_str()),
            Some("ls")
        );
    }

    #[test]
    fn parse_tool_choice_env_rejects_invalid_json() {
        let err = parse_tool_choice_env("json:{bad}").unwrap_err();
        assert!(err.contains("invalid json tool_choice"));
    }

    #[test]
    fn parse_tool_choice_env_rejects_invalid_param() {
        let err = parse_tool_choice_env("json:{}").unwrap_err();
        assert!(err.contains("invalid tool_choice"));
    }

    #[test]
    fn parse_tool_choice_env_rejects_unknown_value() {
        let err = parse_tool_choice_env("maybe").unwrap_err();
        assert!(err.contains("unsupported value"));
    }

    #[test]
    fn build_streaming_request_includes_model_and_stream() {
        let config = OpenResponsesConfig {
            provider_id: None,
            endpoint: "http://example.test/v1/responses".to_string(),
            api_key: None,
            model: Some("gpt-5-nano-2025-08-07".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::required(),
            include: Vec::new(),
            reasoning: None,
            web_search: None,
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: true,
        };
        let payload = build_streaming_request(&config, "hi");
        let body = payload.body();
        assert_eq!(
            body.get("model").and_then(|v| v.as_str()),
            Some("gpt-5-nano-2025-08-07")
        );
        assert_eq!(body.get("stream").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            body.get("max_tool_calls").and_then(|v| v.as_u64()),
            Some(DEFAULT_MAX_TOOL_CALLS)
        );
        assert_eq!(
            body.get("parallel_tool_calls").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn build_streaming_request_defaults_model_for_openrouter_when_unset() {
        let config = OpenResponsesConfig {
            provider_id: None,
            endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
            api_key: None,
            model: None,
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: None,
            web_search: None,
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "hi");
        assert_eq!(
            payload.body().get("model").and_then(|v| v.as_str()),
            Some(DEFAULT_OPENROUTER_MODEL)
        );
    }

    #[test]
    fn build_streaming_request_defaults_model_for_openrouter_provider_id_when_unset() {
        let config = OpenResponsesConfig {
            provider_id: Some("openrouter".to_string()),
            endpoint: "http://127.0.0.1:4010/v1/responses".to_string(),
            api_key: None,
            model: None,
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: None,
            web_search: None,
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "hi");
        assert_eq!(
            payload.body().get("model").and_then(|v| v.as_str()),
            Some(DEFAULT_OPENROUTER_MODEL)
        );
    }

    #[test]
    fn build_streaming_request_items_includes_items() {
        let config = config_with_followup(None);
        let payload =
            build_streaming_request_items(&config, vec![ItemParam::user_message_text("hello")]);
        let input = payload
            .body()
            .get("input")
            .and_then(|value| value.as_array())
            .expect("input array");
        assert_eq!(input.len(), 1);
        assert_eq!(
            input[0].get("type").and_then(|v| v.as_str()),
            Some("message")
        );
    }

    #[test]
    fn parse_reasoning_helpers_accept_known_values() {
        assert_eq!(parse_reasoning_effort("none"), Ok(ReasoningEffort::None));
        assert_eq!(
            parse_reasoning_effort("minimal"),
            Ok(ReasoningEffort::Minimal)
        );
        assert_eq!(parse_reasoning_effort("low"), Ok(ReasoningEffort::Low));
        assert_eq!(
            parse_reasoning_effort("medium"),
            Ok(ReasoningEffort::Medium)
        );
        assert_eq!(parse_reasoning_effort("high"), Ok(ReasoningEffort::High));
        assert_eq!(parse_reasoning_effort("xhigh"), Ok(ReasoningEffort::Xhigh));
        assert_eq!(parse_reasoning_summary("auto"), Ok(ReasoningSummary::Auto));
        assert_eq!(
            parse_reasoning_summary("concise"),
            Ok(ReasoningSummary::Concise)
        );
        assert_eq!(
            parse_reasoning_summary("detailed"),
            Ok(ReasoningSummary::Detailed)
        );
    }

    #[test]
    fn parse_reasoning_helpers_reject_unknown_values() {
        assert!(parse_reasoning_effort("turbo").is_err());
        assert!(parse_reasoning_summary("full").is_err());
    }

    #[test]
    fn build_streaming_request_includes_reasoning_when_configured() {
        let config = OpenResponsesConfig {
            provider_id: Some("openai".to_string()),
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: None,
            model: Some("gpt-5.4-nano".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: Some(OpenResponsesReasoningConfig {
                effort: Some(ReasoningEffort::High),
                summary: Some(ReasoningSummary::Detailed),
            }),
            web_search: None,
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "hi");
        let body = payload.body();
        assert_eq!(
            body.get("reasoning")
                .and_then(|value| value.get("effort"))
                .and_then(|value| value.as_str()),
            Some("high")
        );
        assert_eq!(
            body.get("reasoning")
                .and_then(|value| value.get("summary"))
                .and_then(|value| value.as_str()),
            Some("detailed")
        );
    }

    #[test]
    fn build_streaming_request_drops_reasoning_effort_when_route_does_not_support_it() {
        let config = OpenResponsesConfig {
            provider_id: Some("openai".to_string()),
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: None,
            model: Some("gpt-5.4-nano".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: Some(OpenResponsesReasoningConfig {
                effort: Some(ReasoningEffort::Minimal),
                summary: Some(ReasoningSummary::Detailed),
            }),
            web_search: None,
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "hi");
        let body = payload.body();
        assert!(body
            .get("reasoning")
            .and_then(|value| value.get("effort"))
            .is_none());
        assert_eq!(
            body.get("reasoning")
                .and_then(|value| value.get("summary"))
                .and_then(|value| value.as_str()),
            Some("detailed")
        );
    }

    #[test]
    fn build_streaming_request_uses_openrouter_effort_subset() {
        let config = OpenResponsesConfig {
            provider_id: Some("openrouter".to_string()),
            endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
            api_key: None,
            model: Some("nvidia/nemotron-3-super-120b-a12b:free".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: Some(OpenResponsesReasoningConfig {
                effort: Some(ReasoningEffort::Xhigh),
                summary: Some(ReasoningSummary::Detailed),
            }),
            web_search: None,
            followup_user_message: None,
            stateless_history: true,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "hi");
        let body = payload.body();
        assert!(body
            .get("reasoning")
            .and_then(|value| value.get("effort"))
            .is_none());
        assert_eq!(
            body.get("reasoning")
                .and_then(|value| value.get("summary"))
                .and_then(|value| value.as_str()),
            Some("detailed")
        );
    }

    #[test]
    fn build_streaming_request_includes_canonical_web_search_for_openai() {
        let config = OpenResponsesConfig {
            provider_id: Some("openai".to_string()),
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: None,
            model: Some("gpt-5.4-nano".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: None,
            web_search: Some(OpenResponsesWebSearchConfig {
                enabled: true,
                search_context_size: Some(SearchContextSize::High),
                external_web_access: Some(true),
                user_location: Some(OpenResponsesApproximateLocation {
                    country: Some("US".to_string()),
                    region: Some("California".to_string()),
                    city: Some("San Francisco".to_string()),
                    timezone: Some("America/Los_Angeles".to_string()),
                }),
            }),
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "what happened today?");
        let tools = payload
            .body()
            .get("tools")
            .and_then(|value| value.as_array())
            .expect("tools array");
        let web_search = tools
            .iter()
            .find(|tool| tool.get("type").and_then(|value| value.as_str()) == Some("web_search"))
            .expect("canonical web_search tool");
        assert_eq!(
            web_search
                .get("search_context_size")
                .and_then(|value| value.as_str()),
            Some("high")
        );
        assert_eq!(
            web_search
                .get("external_web_access")
                .and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            web_search
                .get("user_location")
                .and_then(|value| value.get("type"))
                .and_then(|value| value.as_str()),
            Some("approximate")
        );
        assert_eq!(
            web_search
                .get("user_location")
                .and_then(|value| value.get("city"))
                .and_then(|value| value.as_str()),
            Some("San Francisco")
        );
    }

    #[test]
    fn build_streaming_request_does_not_enable_web_search_by_default() {
        let config = OpenResponsesConfig {
            provider_id: Some("openai".to_string()),
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: None,
            model: Some("gpt-5.4-nano".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: None,
            web_search: None,
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "plain run");
        let tools = payload
            .body()
            .get("tools")
            .and_then(|value| value.as_array())
            .expect("tools array");
        assert!(tools
            .iter()
            .all(|tool| tool.get("type").and_then(|value| value.as_str()) != Some("web_search")));
    }

    #[test]
    fn build_streaming_request_omits_disabled_web_search_for_openai() {
        let config = OpenResponsesConfig {
            provider_id: Some("openai".to_string()),
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: None,
            model: Some("gpt-5.4-nano".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: None,
            web_search: Some(OpenResponsesWebSearchConfig::disabled()),
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "plain run");
        let tools = payload
            .body()
            .get("tools")
            .and_then(|value| value.as_array())
            .expect("tools array");
        assert!(tools
            .iter()
            .all(|tool| tool.get("type").and_then(|value| value.as_str()) != Some("web_search")));
    }

    #[test]
    fn build_streaming_request_sends_minimal_web_search_tool_for_openai() {
        let config = OpenResponsesConfig {
            provider_id: Some("openai".to_string()),
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: None,
            model: Some("gpt-5.4-mini".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: None,
            web_search: Some(OpenResponsesWebSearchConfig::default()),
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "search");
        let web_search = payload
            .body()
            .get("tools")
            .and_then(|value| value.as_array())
            .and_then(|tools| {
                tools.iter().find(|tool| {
                    tool.get("type").and_then(|value| value.as_str()) == Some("web_search")
                })
            })
            .expect("web_search tool");

        assert!(web_search.get("search_context_size").is_none());
        assert!(web_search.get("external_web_access").is_none());
        assert!(web_search.get("user_location").is_none());
    }

    #[test]
    fn build_streaming_request_trims_partial_web_search_location() {
        let config = OpenResponsesConfig {
            provider_id: Some("openai".to_string()),
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: None,
            model: Some("gpt-5.4-mini".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: None,
            web_search: Some(OpenResponsesWebSearchConfig {
                enabled: true,
                search_context_size: None,
                external_web_access: None,
                user_location: Some(OpenResponsesApproximateLocation {
                    country: Some(" GB ".to_string()),
                    region: Some("   ".to_string()),
                    city: Some(" London ".to_string()),
                    timezone: None,
                }),
            }),
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "search");
        let location = payload
            .body()
            .get("tools")
            .and_then(|value| value.as_array())
            .and_then(|tools| {
                tools.iter().find(|tool| {
                    tool.get("type").and_then(|value| value.as_str()) == Some("web_search")
                })
            })
            .and_then(|tool| tool.get("user_location"))
            .expect("user_location");

        assert_eq!(
            location.get("type").and_then(|value| value.as_str()),
            Some("approximate")
        );
        assert_eq!(
            location.get("country").and_then(|value| value.as_str()),
            Some("GB")
        );
        assert_eq!(
            location.get("city").and_then(|value| value.as_str()),
            Some("London")
        );
        assert!(location.get("region").is_none());
        assert!(location.get("timezone").is_none());
    }

    #[test]
    fn build_streaming_request_omits_disabled_web_search_even_with_fields() {
        let config = OpenResponsesConfig {
            provider_id: Some("openai".to_string()),
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: None,
            model: Some("gpt-5.4-mini".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: None,
            web_search: Some(OpenResponsesWebSearchConfig {
                enabled: false,
                search_context_size: Some(SearchContextSize::High),
                external_web_access: Some(false),
                user_location: Some(OpenResponsesApproximateLocation {
                    country: Some("US".to_string()),
                    region: None,
                    city: None,
                    timezone: None,
                }),
            }),
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "plain run");
        let tools = payload
            .body()
            .get("tools")
            .and_then(|value| value.as_array())
            .expect("tools array");
        assert!(tools
            .iter()
            .all(|tool| tool.get("type").and_then(|value| value.as_str()) != Some("web_search")));
    }

    #[test]
    fn build_streaming_request_omits_canonical_web_search_for_openrouter() {
        let config = OpenResponsesConfig {
            provider_id: Some("openrouter".to_string()),
            endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
            api_key: None,
            model: Some("google/gemma-4-26b-a4b-it".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: None,
            web_search: Some(OpenResponsesWebSearchConfig {
                enabled: true,
                search_context_size: Some(SearchContextSize::Medium),
                external_web_access: Some(true),
                user_location: Some(OpenResponsesApproximateLocation {
                    country: Some("US".to_string()),
                    region: None,
                    city: None,
                    timezone: None,
                }),
            }),
            followup_user_message: None,
            stateless_history: true,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "what happened today?");
        let tools = payload
            .body()
            .get("tools")
            .and_then(|value| value.as_array())
            .expect("tools array");
        assert!(tools
            .iter()
            .all(|tool| tool.get("type").and_then(|value| value.as_str()) != Some("web_search")));
    }

    #[test]
    fn builtin_function_tools_are_strict_false() {
        let tools = builtin_function_tools();
        let tool = tools
            .iter()
            .find(|tool| tool.get("name").and_then(|v| v.as_str()) == Some("read"))
            .expect("read tool");
        assert_eq!(tool.get("strict").and_then(|v| v.as_bool()), Some(false));
    }

    #[test]
    fn parse_openresponses_include_helpers_accept_canonical_values() {
        assert_eq!(
            parse_openresponses_include("file_search_call.results"),
            Ok(OpenResponsesInclude::FileSearchCallResults)
        );
        assert_eq!(
            parse_openresponses_include("web_search_call.results"),
            Ok(OpenResponsesInclude::WebSearchCallResults)
        );
        assert_eq!(
            parse_openresponses_include("web_search_call.action.sources"),
            Ok(OpenResponsesInclude::WebSearchCallActionSources)
        );
        assert_eq!(
            parse_openresponses_include("message.input_image.image_url"),
            Ok(OpenResponsesInclude::MessageInputImageImageUrl)
        );
        assert_eq!(
            parse_openresponses_include("computer_call_output.output.image_url"),
            Ok(OpenResponsesInclude::ComputerCallOutputOutputImageUrl)
        );
        assert_eq!(
            parse_openresponses_include("code_interpreter_call.outputs"),
            Ok(OpenResponsesInclude::CodeInterpreterCallOutputs)
        );
        assert_eq!(
            parse_openresponses_include("reasoning.encrypted_content"),
            Ok(OpenResponsesInclude::ReasoningEncryptedContent)
        );
        assert_eq!(
            parse_openresponses_include("message.output_text.logprobs"),
            Ok(OpenResponsesInclude::MessageOutputTextLogprobs)
        );
        assert_eq!(
            parse_openresponses_include_list(
                "reasoning.encrypted_content, message.output_text.logprobs , reasoning.encrypted_content"
            ),
            Ok(vec![
                OpenResponsesInclude::ReasoningEncryptedContent,
                OpenResponsesInclude::MessageOutputTextLogprobs,
            ])
        );
    }

    #[test]
    fn parse_openresponses_include_helpers_reject_unknown_values() {
        assert!(parse_openresponses_include("reasoning.summary").is_err());
        assert!(parse_openresponses_include_list("reasoning.encrypted_content,unknown").is_err());
    }

    #[test]
    fn parse_search_context_size_accepts_values_and_rejects_unknown() {
        assert_eq!(parse_search_context_size("low"), Ok(SearchContextSize::Low));
        assert_eq!(
            parse_search_context_size("medium"),
            Ok(SearchContextSize::Medium)
        );
        assert_eq!(
            parse_search_context_size("high"),
            Ok(SearchContextSize::High)
        );
        assert!(parse_search_context_size("large").is_err());
    }

    #[test]
    fn openresponses_include_as_str_covers_all_values() {
        assert_eq!(
            OpenResponsesInclude::FileSearchCallResults.as_str(),
            "file_search_call.results"
        );
        assert_eq!(
            OpenResponsesInclude::WebSearchCallResults.as_str(),
            "web_search_call.results"
        );
        assert_eq!(
            OpenResponsesInclude::WebSearchCallActionSources.as_str(),
            "web_search_call.action.sources"
        );
        assert_eq!(
            OpenResponsesInclude::MessageInputImageImageUrl.as_str(),
            "message.input_image.image_url"
        );
        assert_eq!(
            OpenResponsesInclude::ComputerCallOutputOutputImageUrl.as_str(),
            "computer_call_output.output.image_url"
        );
        assert_eq!(
            OpenResponsesInclude::CodeInterpreterCallOutputs.as_str(),
            "code_interpreter_call.outputs"
        );
        assert_eq!(
            OpenResponsesInclude::ReasoningEncryptedContent.as_str(),
            "reasoning.encrypted_content"
        );
        assert_eq!(
            OpenResponsesInclude::MessageOutputTextLogprobs.as_str(),
            "message.output_text.logprobs"
        );
    }

    #[test]
    fn web_search_deserialize_defaults_enabled_and_preserves_region_timezone() {
        let web_search: OpenResponsesWebSearchConfig = serde_json::from_value(serde_json::json!({
            "search_context_size": "medium",
            "user_location": {
                "region": " Ontario ",
                "timezone": " America/Toronto "
            }
        }))
        .expect("web search config");

        assert!(web_search.enabled);
        assert_eq!(
            web_search.search_context_size,
            Some(SearchContextSize::Medium)
        );
        assert_eq!(web_search.external_web_access, None);

        let config = OpenResponsesConfig {
            provider_id: Some("openai".to_string()),
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: None,
            model: Some("gpt-5.4-mini".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: None,
            web_search: Some(web_search),
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "search");
        let location = payload
            .body()
            .get("tools")
            .and_then(|value| value.as_array())
            .and_then(|tools| {
                tools.iter().find(|tool| {
                    tool.get("type").and_then(|value| value.as_str()) == Some("web_search")
                })
            })
            .and_then(|tool| tool.get("user_location"))
            .expect("user_location");

        assert_eq!(location.get("country"), None);
        assert_eq!(
            location.get("region").and_then(|value| value.as_str()),
            Some("Ontario")
        );
        assert_eq!(location.get("city"), None);
        assert_eq!(
            location.get("timezone").and_then(|value| value.as_str()),
            Some("America/Toronto")
        );
    }

    #[test]
    fn web_search_override_normalizes_trimmed_location_and_empty_disabled_config() {
        assert_eq!(OpenResponsesWebSearchConfig::disabled().normalized(), None);
        assert_eq!(
            OpenResponsesApproximateLocation {
                country: Some(" ".to_string()),
                region: None,
                city: Some("\t".to_string()),
                timezone: None,
            }
            .normalized(),
            None
        );

        let override_cfg = OpenResponsesWebSearchOverride {
            enabled: Some(true),
            search_context_size: Some(SearchContextSize::Low),
            external_web_access: Some(false),
            user_location: Some(OpenResponsesApproximateLocation {
                country: Some(" GB ".to_string()),
                region: Some(" England ".to_string()),
                city: Some(" London ".to_string()),
                timezone: Some(" Europe/London ".to_string()),
            }),
        };
        assert!(!override_cfg.is_empty());

        let cfg = override_cfg
            .into_config()
            .expect("non-empty web search config");
        assert!(cfg.enabled);
        assert_eq!(cfg.search_context_size, Some(SearchContextSize::Low));
        assert_eq!(cfg.external_web_access, Some(false));
        let location = cfg.user_location.expect("location");
        assert_eq!(location.country.as_deref(), Some("GB"));
        assert_eq!(location.region.as_deref(), Some("England"));
        assert_eq!(location.city.as_deref(), Some("London"));
        assert_eq!(location.timezone.as_deref(), Some("Europe/London"));

        assert!(OpenResponsesWebSearchOverride::default().is_empty());
        assert_eq!(
            OpenResponsesWebSearchOverride::default().into_config(),
            None
        );
    }

    #[test]
    fn build_streaming_request_includes_effective_include_values() {
        let config = OpenResponsesConfig {
            provider_id: Some("openai".to_string()),
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            api_key: None,
            model: Some("gpt-5.4-nano".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: vec![
                OpenResponsesInclude::ReasoningEncryptedContent,
                OpenResponsesInclude::MessageOutputTextLogprobs,
            ],
            reasoning: None,
            web_search: None,
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "hi");
        assert_eq!(
            payload.body().get("include"),
            Some(&json!([
                "reasoning.encrypted_content",
                "message.output_text.logprobs"
            ]))
        );
    }
}
