use rip_provider_openresponses::{
    CreateResponseBuilder, CreateResponsePayload, ItemParam, ToolChoiceParam,
};
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct OpenResponsesConfig {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub tool_choice: ToolChoiceParam,
    pub followup_user_message: Option<String>,
    pub stateless_history: bool,
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
        Some(Self {
            endpoint,
            api_key,
            model,
            tool_choice,
            followup_user_message,
            stateless_history,
        })
    }
}

pub const DEFAULT_MAX_TOOL_CALLS: u64 = 32;

pub fn build_streaming_request(
    config: &OpenResponsesConfig,
    prompt: &str,
) -> CreateResponsePayload {
    let mut builder = base_streaming_builder(config)
        .input_text(prompt)
        .tools_raw(builtin_function_tools())
        .tool_choice(config.tool_choice.clone())
        .parallel_tool_calls(false)
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
        .parallel_tool_calls(false)
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
        .parallel_tool_calls(false)
        .max_tool_calls(DEFAULT_MAX_TOOL_CALLS)
        .insert_raw("stream", Value::Bool(true));
    builder.build()
}

fn base_streaming_builder(config: &OpenResponsesConfig) -> CreateResponseBuilder {
    let builder = CreateResponseBuilder::new();
    if let Some(model) = config.model.as_deref() {
        return builder.model(model.to_string());
    }
    builder
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
            tool_choice: ToolChoiceParam::auto(),
            followup_user_message,
            stateless_history: false,
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
}
