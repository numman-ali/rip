#[cfg(not(test))]
use std::env;

use rip_provider_openresponses::{
    CreateResponseBuilder, CreateResponsePayload, ItemParam, ToolChoiceParam,
};
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct OpenResponsesConfig {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub model: Option<String>,
}

impl OpenResponsesConfig {
    #[cfg(not(test))]
    pub fn from_env() -> Option<Self> {
        let endpoint = env::var("RIP_OPENRESPONSES_ENDPOINT").ok()?;
        let api_key = env::var("RIP_OPENRESPONSES_API_KEY").ok();
        let model = env::var("RIP_OPENRESPONSES_MODEL").ok();
        Some(Self {
            endpoint,
            api_key,
            model,
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
        .tool_choice(ToolChoiceParam::auto())
        .parallel_tool_calls(false)
        .max_tool_calls(DEFAULT_MAX_TOOL_CALLS);
    builder = builder.insert_raw("stream", Value::Bool(true));
    builder.build()
}

pub fn build_streaming_followup_request(
    config: &OpenResponsesConfig,
    previous_response_id: &str,
    tool_outputs: Vec<ItemParam>,
) -> CreateResponsePayload {
    let builder = base_streaming_builder(config)
        .insert_raw(
            "previous_response_id",
            Value::String(previous_response_id.to_string()),
        )
        .input_items(tool_outputs)
        .tools_raw(builtin_function_tools())
        .tool_choice(ToolChoiceParam::auto())
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
        "strict": true
    })
}
