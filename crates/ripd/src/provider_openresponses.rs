use rip_provider_openresponses::{
    CreateResponseBuilder, CreateResponsePayload, ItemParam, ToolChoiceParam,
};
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct OpenResponsesConfig {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub headers: Vec<(String, String)>,
    pub tool_choice: ToolChoiceParam,
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
            endpoint,
            api_key,
            model,
            headers: Vec::new(),
            tool_choice,
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
    let mut builder = base_streaming_builder(config)
        .input_text(prompt)
        .tools_raw(builtin_function_tools())
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
    let mut builder = base_streaming_builder(config)
        .input_items(items)
        .tools_raw(builtin_function_tools())
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
    let mut builder = base_streaming_builder(config);
    if let Some(previous_response_id) = previous_response_id {
        builder = builder.insert_raw(
            "previous_response_id",
            Value::String(previous_response_id.to_string()),
        );
    }
    let builder = builder
        .input_items(input_items)
        .tools_raw(builtin_function_tools())
        .tool_choice(config.tool_choice.clone())
        .parallel_tool_calls(config.parallel_tool_calls)
        .max_tool_calls(DEFAULT_MAX_TOOL_CALLS)
        .insert_raw("stream", Value::Bool(true));
    builder.build()
}

fn base_streaming_builder(config: &OpenResponsesConfig) -> CreateResponseBuilder {
    let builder = CreateResponseBuilder::new();
    if let Some(model) = config.model.as_deref() {
        return builder.model(model.to_string());
    }
    if is_openrouter_responses_endpoint(&config.endpoint) {
        return builder.model(DEFAULT_OPENROUTER_MODEL.to_string());
    }
    builder
}

fn is_openrouter_responses_endpoint(endpoint: &str) -> bool {
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
            endpoint: "http://example.test/v1/responses".to_string(),
            api_key: None,
            model: None,
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
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
            endpoint: "http://example.test/v1/responses".to_string(),
            api_key: None,
            model: Some("gpt-5-nano-2025-08-07".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::required(),
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
            endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
            api_key: None,
            model: None,
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
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
    fn builtin_function_tools_are_strict_false() {
        let tools = builtin_function_tools();
        let tool = tools
            .iter()
            .find(|tool| tool.get("name").and_then(|v| v.as_str()) == Some("read"))
            .expect("read tool");
        assert_eq!(tool.get("strict").and_then(|v| v.as_bool()), Some(false));
    }
}
