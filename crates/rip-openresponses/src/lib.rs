use jsonschema::JSONSchema;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::BTreeMap;

const SPLIT_COMPONENTS_URI_PREFIX: &str = "https://openresponses.local/components/schemas/";
const SPLIT_PATHS_URI: &str = "https://openresponses.local/paths/responses.json";

static OPENAPI: Lazy<Value> = Lazy::new(|| {
    let raw = include_str!("../../../schemas/openresponses/openapi.json");
    serde_json::from_str(raw).expect("openapi.json valid")
});

static SPLIT_COMPONENTS: Lazy<BTreeMap<String, Value>> = Lazy::new(|| {
    let raw = include_str!("../../../schemas/openresponses/split_components.json");
    serde_json::from_str(raw).expect("split_components.json valid")
});

static SPLIT_PATHS_RESPONSES: Lazy<Value> = Lazy::new(|| {
    let raw = include_str!("../../../schemas/openresponses/paths_responses.json");
    serde_json::from_str(raw).expect("paths_responses.json valid")
});

static STREAM_EVENT_TYPES: Lazy<Vec<String>> = Lazy::new(|| {
    let raw = include_str!("../../../schemas/openresponses/streaming_event_types.json");
    serde_json::from_str(raw).expect("streaming_event_types.json valid")
});

static STREAM_SCHEMA: Lazy<Value> =
    Lazy::new(|| extract_split_streaming_schema().expect("split streaming event schema not found"));

static RESPONSE_SCHEMA: Lazy<Value> = Lazy::new(|| {
    split_component_schema("ResponseResource.json")
        .cloned()
        .expect("ResponseResource schema not found")
});

static CREATE_RESPONSE_SCHEMA: Lazy<Value> = Lazy::new(|| {
    split_component_schema("CreateResponseBody.json")
        .cloned()
        .expect("CreateResponseBody schema not found")
});

static TOOL_PARAM_SCHEMA: Lazy<Value> = Lazy::new(|| {
    split_component_schema("ResponsesToolParam.json")
        .cloned()
        .expect("ResponsesToolParam schema not found")
});

static TOOL_CHOICE_SCHEMA: Lazy<Value> = Lazy::new(|| {
    split_component_schema("ToolChoiceParam.json")
        .cloned()
        .expect("ToolChoiceParam schema not found")
});

static ITEM_PARAM_SCHEMA: Lazy<Value> = Lazy::new(|| {
    let mut schema = split_component_schema("ItemParam.json")
        .cloned()
        .expect("ItemParam schema not found");
    if let Some(obj) = schema.as_object_mut() {
        obj.remove("discriminator");
    }
    schema
});

static STREAM_VALIDATOR: Lazy<JSONSchema> = Lazy::new(compile_split_stream_schema);

static RESPONSE_VALIDATOR: Lazy<JSONSchema> =
    Lazy::new(|| compile_split_schema("ResponseResource.json"));

static CREATE_RESPONSE_VALIDATOR: Lazy<JSONSchema> =
    Lazy::new(|| compile_split_schema("CreateResponseBody.json"));

static TOOL_PARAM_VALIDATOR: Lazy<JSONSchema> =
    Lazy::new(|| compile_split_schema("ResponsesToolParam.json"));

static TOOL_CHOICE_VALIDATOR: Lazy<JSONSchema> =
    Lazy::new(|| compile_split_schema("ToolChoiceParam.json"));

static SPECIFIC_TOOL_CHOICE_VALIDATOR: Lazy<JSONSchema> =
    Lazy::new(|| compile_split_schema("SpecificToolChoiceParam.json"));

const MESSAGE_ROLES: [&str; 4] = ["assistant", "developer", "system", "user"];

pub fn openapi() -> &'static Value {
    &OPENAPI
}

pub fn allowed_stream_event_types() -> &'static [String] {
    &STREAM_EVENT_TYPES
}

pub fn streaming_event_schema() -> &'static Value {
    &STREAM_SCHEMA
}

pub fn response_resource_schema() -> &'static Value {
    &RESPONSE_SCHEMA
}

pub fn create_response_body_schema() -> &'static Value {
    &CREATE_RESPONSE_SCHEMA
}

pub fn tool_param_schema() -> &'static Value {
    &TOOL_PARAM_SCHEMA
}

pub fn tool_choice_param_schema() -> &'static Value {
    &TOOL_CHOICE_SCHEMA
}

pub fn item_param_schema() -> &'static Value {
    &ITEM_PARAM_SCHEMA
}

pub fn validate_stream_event(value: &Value) -> Result<(), Vec<String>> {
    match STREAM_VALIDATOR.validate(value) {
        Ok(_) => Ok(()),
        Err(errors) => Err(errors.map(|e| e.to_string()).collect()),
    }
}

pub fn validate_response_resource(value: &Value) -> Result<(), Vec<String>> {
    match RESPONSE_VALIDATOR.validate(value) {
        Ok(_) => Ok(()),
        Err(errors) => Err(errors.map(|e| e.to_string()).collect()),
    }
}

pub fn validate_create_response_body(value: &Value) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let mut stripped = value.clone();
    if let Value::Object(map) = &mut stripped {
        // Validate tool fields separately; jsonschema rejects the oneOf tool variants.
        if let Some(tools) = map.remove("tools") {
            match tools {
                Value::Null => {}
                Value::Array(items) => {
                    for (idx, item) in items.iter().enumerate() {
                        if let Err(errs) = validate_responses_tool_param(item) {
                            errors
                                .extend(errs.into_iter().map(|err| format!("tools[{idx}]: {err}")));
                        }
                    }
                }
                _ => errors.push("tools must be an array or null".to_string()),
            }
        }
        if let Some(choice) = map.remove("tool_choice") {
            match choice {
                Value::Null => {}
                _ => {
                    if let Err(errs) = validate_tool_choice_param(&choice) {
                        errors.extend(errs.into_iter().map(|err| format!("tool_choice: {err}")));
                    }
                }
            }
        }
    }

    if let Err(errs) = CREATE_RESPONSE_VALIDATOR.validate(&stripped) {
        errors.extend(errs.map(|e| e.to_string()));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_responses_tool_param(value: &Value) -> Result<(), Vec<String>> {
    match TOOL_PARAM_VALIDATOR.validate(value) {
        Ok(_) => Ok(()),
        Err(errors) => Err(errors.map(|e| e.to_string()).collect()),
    }
}

pub fn validate_tool_choice_param(value: &Value) -> Result<(), Vec<String>> {
    match TOOL_CHOICE_VALIDATOR.validate(value) {
        Ok(_) => Ok(()),
        Err(errors) => Err(errors.map(|e| e.to_string()).collect()),
    }
}

pub fn validate_specific_tool_choice_param(value: &Value) -> Result<(), Vec<String>> {
    match SPECIFIC_TOOL_CHOICE_VALIDATOR.validate(value) {
        Ok(_) => Ok(()),
        Err(errors) => Err(errors.map(|e| e.to_string()).collect()),
    }
}

pub fn validate_item_param(value: &Value) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let map = match value.as_object() {
        Some(map) => map,
        None => return Err(vec!["ItemParam must be an object".to_string()]),
    };

    let type_value = map.get("type");
    let item_type = type_value.and_then(|value| value.as_str());

    if item_type.is_none() || item_type == Some("item_reference") {
        return validate_item_reference(map, type_value);
    }

    match item_type.unwrap() {
        "message" => {
            let context = "ItemParam(message)";
            match require_field(map, "role", context, &mut errors) {
                Some(Value::String(role)) => {
                    if !MESSAGE_ROLES.contains(&role.as_str()) {
                        errors.push(format!(
                            "{context}.role must be one of {}",
                            MESSAGE_ROLES.join(", ")
                        ));
                    }
                }
                Some(_) => errors.push(format!("{context}.role must be a string")),
                None => {}
            }
            match require_field(map, "content", context, &mut errors) {
                Some(Value::String(_)) | Some(Value::Array(_)) => {}
                Some(_) => errors.push(format!("{context}.content must be a string or array")),
                None => {}
            }
        }
        "function_call" => {
            let context = "ItemParam(function_call)";
            require_string_field(map, "call_id", context, &mut errors);
            require_string_field(map, "name", context, &mut errors);
            require_string_field(map, "arguments", context, &mut errors);
        }
        "function_call_output" => {
            let context = "ItemParam(function_call_output)";
            require_string_field(map, "call_id", context, &mut errors);
            match require_field(map, "output", context, &mut errors) {
                Some(Value::String(_)) | Some(Value::Array(_)) => {}
                Some(_) => errors.push(format!("{context}.output must be a string or array")),
                None => {}
            }
        }
        "reasoning" => {
            let context = "ItemParam(reasoning)";
            require_array_field(map, "summary", context, &mut errors);
        }
        "compaction" => {
            let context = "ItemParam(compaction)";
            require_string_field(map, "encrypted_content", context, &mut errors);
        }
        "code_interpreter_call" => {
            let context = "ItemParam(code_interpreter_call)";
            require_string_field(map, "id", context, &mut errors);
            require_string_field(map, "container_id", context, &mut errors);
            require_string_field(map, "code", context, &mut errors);
        }
        "computer_call" => {
            let context = "ItemParam(computer_call)";
            require_string_field(map, "call_id", context, &mut errors);
            require_object_field(map, "action", context, &mut errors);
        }
        "computer_call_output" => {
            let context = "ItemParam(computer_call_output)";
            require_string_field(map, "call_id", context, &mut errors);
            require_object_field(map, "output", context, &mut errors);
        }
        "custom_tool_call" => {
            let context = "ItemParam(custom_tool_call)";
            require_string_field(map, "call_id", context, &mut errors);
            require_string_field(map, "name", context, &mut errors);
            require_string_field(map, "input", context, &mut errors);
        }
        "custom_tool_call_output" => {
            let context = "ItemParam(custom_tool_call_output)";
            require_string_field(map, "call_id", context, &mut errors);
            require_string_field(map, "output", context, &mut errors);
        }
        "file_search_call" => {
            let context = "ItemParam(file_search_call)";
            require_string_field(map, "id", context, &mut errors);
            match require_field(map, "queries", context, &mut errors) {
                Some(Value::Array(items)) => {
                    if items.is_empty() {
                        errors.push(format!("{context}.queries must not be empty"));
                    }
                    for (idx, item) in items.iter().enumerate() {
                        if !item.is_string() {
                            errors.push(format!("{context}.queries[{idx}] must be a string"));
                        }
                    }
                }
                Some(_) => errors.push(format!("{context}.queries must be an array")),
                None => {}
            }
        }
        "web_search_call" => {}
        "image_generation_call" => {
            let context = "ItemParam(image_generation_call)";
            require_string_field(map, "id", context, &mut errors);
        }
        "local_shell_call" => {
            let context = "ItemParam(local_shell_call)";
            require_string_field(map, "call_id", context, &mut errors);
            require_object_field(map, "action", context, &mut errors);
        }
        "local_shell_call_output" => {
            let context = "ItemParam(local_shell_call_output)";
            require_string_field(map, "call_id", context, &mut errors);
            require_string_field(map, "output", context, &mut errors);
        }
        "shell_call" => {
            let context = "ItemParam(shell_call)";
            require_string_field(map, "call_id", context, &mut errors);
            require_object_field(map, "action", context, &mut errors);
        }
        "shell_call_output" => {
            let context = "ItemParam(shell_call_output)";
            require_string_field(map, "call_id", context, &mut errors);
            require_array_field(map, "output", context, &mut errors);
        }
        "apply_patch_call" => {
            let context = "ItemParam(apply_patch_call)";
            require_string_field(map, "call_id", context, &mut errors);
            require_string_field(map, "status", context, &mut errors);
            require_object_field(map, "operation", context, &mut errors);
        }
        "apply_patch_call_output" => {
            let context = "ItemParam(apply_patch_call_output)";
            require_string_field(map, "call_id", context, &mut errors);
            require_string_field(map, "status", context, &mut errors);
        }
        "mcp_approval_request" => {
            let context = "ItemParam(mcp_approval_request)";
            require_string_field(map, "server_label", context, &mut errors);
            require_string_field(map, "name", context, &mut errors);
            require_string_field(map, "arguments", context, &mut errors);
        }
        "mcp_approval_response" => {
            let context = "ItemParam(mcp_approval_response)";
            require_string_field(map, "approval_request_id", context, &mut errors);
            require_bool_field(map, "approve", context, &mut errors);
        }
        other => errors.push(format!("ItemParam.type has unsupported value \"{other}\"")),
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn require_field<'a>(
    map: &'a serde_json::Map<String, Value>,
    field: &str,
    context: &str,
    errors: &mut Vec<String>,
) -> Option<&'a Value> {
    match map.get(field) {
        Some(value) => Some(value),
        None => {
            errors.push(format!("{context} missing required field `{field}`"));
            None
        }
    }
}

fn require_string_field(
    map: &serde_json::Map<String, Value>,
    field: &str,
    context: &str,
    errors: &mut Vec<String>,
) {
    match require_field(map, field, context, errors) {
        Some(Value::String(_)) => {}
        Some(_) => errors.push(format!("{context}.{field} must be a string")),
        None => {}
    }
}

fn require_array_field(
    map: &serde_json::Map<String, Value>,
    field: &str,
    context: &str,
    errors: &mut Vec<String>,
) {
    match require_field(map, field, context, errors) {
        Some(Value::Array(_)) => {}
        Some(_) => errors.push(format!("{context}.{field} must be an array")),
        None => {}
    }
}

fn require_object_field(
    map: &serde_json::Map<String, Value>,
    field: &str,
    context: &str,
    errors: &mut Vec<String>,
) {
    match require_field(map, field, context, errors) {
        Some(Value::Object(_)) => {}
        Some(_) => errors.push(format!("{context}.{field} must be an object")),
        None => {}
    }
}

fn require_bool_field(
    map: &serde_json::Map<String, Value>,
    field: &str,
    context: &str,
    errors: &mut Vec<String>,
) {
    match require_field(map, field, context, errors) {
        Some(Value::Bool(_)) => {}
        Some(_) => errors.push(format!("{context}.{field} must be a boolean")),
        None => {}
    }
}

fn validate_item_reference(
    map: &serde_json::Map<String, Value>,
    type_value: Option<&Value>,
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if let Some(type_value) = type_value {
        match type_value {
            Value::Null => {}
            Value::String(value) => {
                if value != "item_reference" {
                    errors.push(
                        "ItemReferenceParam.type must be \"item_reference\" when provided"
                            .to_string(),
                    );
                }
            }
            _ => errors.push("ItemReferenceParam.type must be a string or null".to_string()),
        }
    }
    require_string_field(map, "id", "ItemReferenceParam", &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn split_component_schema(name: &str) -> Option<&'static Value> {
    SPLIT_COMPONENTS.get(name)
}

fn compile_split_schema(name: &str) -> JSONSchema {
    let mut options = JSONSchema::options();
    for (schema_name, schema) in SPLIT_COMPONENTS.iter() {
        let uri = format!("{SPLIT_COMPONENTS_URI_PREFIX}{schema_name}");
        options.with_document(uri, schema.clone());
    }
    let root_ref = serde_json::json!({
        "$ref": format!("{SPLIT_COMPONENTS_URI_PREFIX}{name}")
    });
    options
        .compile(&root_ref)
        .unwrap_or_else(|_| panic!("compile split schema {name}"))
}

fn compile_split_stream_schema() -> JSONSchema {
    let mut options = JSONSchema::options();
    for (schema_name, schema) in SPLIT_COMPONENTS.iter() {
        let uri = format!("{SPLIT_COMPONENTS_URI_PREFIX}{schema_name}");
        options.with_document(uri, schema.clone());
    }
    options.with_document(SPLIT_PATHS_URI.to_string(), SPLIT_PATHS_RESPONSES.clone());
    let root_ref = serde_json::json!({
        "$ref": format!("{SPLIT_PATHS_URI}#/post/responses/200/content/text~1event-stream/schema")
    });
    options
        .compile(&root_ref)
        .expect("compile split streaming schema")
}

fn extract_split_streaming_schema() -> Option<Value> {
    let pointer = "/post/responses/200/content/text~1event-stream/schema";
    SPLIT_PATHS_RESPONSES.pointer(pointer).cloned()
}

#[cfg(test)]
fn extract_component_schema(name: &str) -> Option<Value> {
    let pointer = format!("/components/schemas/{name}");
    OPENAPI.pointer(&pointer).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_response_resource() -> Value {
        let raw = include_str!(
            "../../rip-provider-openresponses/fixtures/openresponses/stream_all.jsonl"
        );
        for line in raw.lines() {
            let value: Value = serde_json::from_str(line).expect("fixture line must be valid json");
            if let Some(response) = value.get("response") {
                return response.clone();
            }
        }
        panic!("stream fixture missing response resource");
    }

    fn response_with_tool_choice(choice: Value) -> Value {
        let mut response = fixture_response_resource();
        if let Value::Object(map) = &mut response {
            map.insert("tool_choice".to_string(), choice);
        }
        response
    }

    fn response_with_tools(tools: Vec<Value>) -> Value {
        let mut response = fixture_response_resource();
        if let Value::Object(map) = &mut response {
            map.insert("tools".to_string(), Value::Array(tools));
        }
        response
    }

    fn response_with_output(items: Vec<Value>) -> Value {
        let mut response = fixture_response_resource();
        if let Value::Object(map) = &mut response {
            map.insert("output".to_string(), Value::Array(items));
        }
        response
    }

    #[test]
    fn types_list_is_non_empty() {
        assert!(!allowed_stream_event_types().is_empty());
    }

    #[test]
    fn openapi_loads() {
        assert!(openapi().get("openapi").is_some());
    }

    #[test]
    fn streaming_schema_is_present() {
        assert!(streaming_event_schema().get("oneOf").is_some());
    }

    #[test]
    fn response_schema_is_present() {
        assert!(response_resource_schema().get("properties").is_some());
    }

    #[test]
    fn create_response_schema_is_present() {
        assert!(create_response_body_schema().get("properties").is_some());
    }

    #[test]
    fn tool_param_schema_is_present() {
        assert!(tool_param_schema().get("oneOf").is_some());
    }

    #[test]
    fn tool_choice_param_schema_is_present() {
        assert!(tool_choice_param_schema().get("oneOf").is_some());
    }

    #[test]
    fn item_param_schema_is_present() {
        assert!(item_param_schema().get("oneOf").is_some());
    }

    #[test]
    fn validate_stream_event_rejects_empty() {
        let value = serde_json::json!({});
        assert!(validate_stream_event(&value).is_err());
    }

    #[test]
    fn validate_stream_event_accepts_output_text_delta() {
        let value = serde_json::json!({
            "type": "response.output_text.delta",
            "sequence_number": 1,
            "item_id": "item_1",
            "output_index": 0,
            "content_index": 0,
            "delta": "hi",
            "logprobs": []
        });
        let errors = validate_stream_event(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_response_resource_rejects_empty() {
        let value = serde_json::json!({});
        assert!(validate_response_resource(&value).is_err());
    }

    #[test]
    fn validate_response_resource_accepts_fixture() {
        let value = fixture_response_resource();
        let errors = validate_response_resource(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_response_resource_accepts_tool_choice_variants() {
        let variants = vec![
            serde_json::json!({ "type": "code_interpreter" }),
            serde_json::json!({ "type": "function" }),
            serde_json::json!({ "type": "mcp", "server_label": "srv", "name": null }),
            serde_json::json!({ "type": "file_search" }),
            serde_json::json!({ "type": "web_search_preview" }),
            serde_json::json!({ "type": "image_generation" }),
            serde_json::json!({ "type": "computer_use_preview" }),
            serde_json::json!({ "type": "local_shell" }),
            serde_json::json!({ "type": "shell" }),
            serde_json::json!({ "type": "apply_patch" }),
            serde_json::json!({ "type": "custom" }),
            serde_json::json!({
                "type": "allowed_tools",
                "tools": [
                    { "type": "function" }
                ],
                "mode": "auto"
            }),
            serde_json::json!("auto"),
            serde_json::json!("required"),
            serde_json::json!("none"),
        ];

        for choice in variants {
            let value = response_with_tool_choice(choice.clone());
            let errors = validate_response_resource(&value).err().unwrap_or_default();
            assert!(errors.is_empty(), "errors: {errors:?} for {choice}");
        }
    }

    #[test]
    fn validate_response_resource_accepts_tool_variants() {
        let tools = vec![
            serde_json::json!({
                "type": "file_search",
                "vector_store_ids": ["vs_1"],
                "max_num_results": 1,
                "ranking_options": {
                    "ranker": "auto",
                    "score_threshold": 0.0
                },
                "filters": null
            }),
            serde_json::json!({
                "type": "function",
                "name": "echo",
                "description": null,
                "parameters": null,
                "strict": null
            }),
            serde_json::json!({
                "type": "web_search_preview",
                "user_location": null,
                "search_context_size": "medium"
            }),
            serde_json::json!({
                "type": "mcp",
                "server_label": "srv",
                "server_description": null,
                "server_url": null,
                "headers": null,
                "allowed_tools": null,
                "require_approval": "always"
            }),
            serde_json::json!({
                "type": "computer_use_preview",
                "environment": "browser",
                "display_width": 800,
                "display_height": 600
            }),
            serde_json::json!({
                "type": "image_generation",
                "model": null,
                "n": 1,
                "quality": null,
                "size": null,
                "output_format": null,
                "output_compression": 100,
                "moderation": null,
                "background": null
            }),
            serde_json::json!({ "type": "shell" }),
            serde_json::json!({
                "type": "custom",
                "name": "custom_tool",
                "description": null,
                "format": null
            }),
            serde_json::json!({ "type": "apply_patch" }),
        ];

        for tool in tools {
            let value = response_with_tools(vec![tool.clone()]);
            let errors = validate_response_resource(&value).err().unwrap_or_default();
            assert!(errors.is_empty(), "errors: {errors:?} for {tool}");
        }
    }

    #[test]
    fn validate_response_resource_accepts_mcp_list_tools_output() {
        let item = serde_json::json!({
            "type": "mcp_list_tools",
            "id": "list_1",
            "server_label": "srv",
            "tools": [
                {
                    "name": "tool_a",
                    "description": null,
                    "input_schema": {},
                    "annotations": null
                }
            ]
        });
        let value = response_with_output(vec![item]);
        let errors = validate_response_resource(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_memory_tool_param_schema() {
        let schema = compile_split_schema("MemoryToolParam.json");
        let value = serde_json::json!({
            "type": "memory",
            "memory": "remember this",
            "environment": {
                "type": "local_file",
                "root": "/tmp"
            }
        });
        let errors = match schema.validate(&value) {
            Ok(_) => Vec::new(),
            Err(errors) => errors.map(|err| err.to_string()).collect(),
        };
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_create_response_body_accepts_minimal() {
        let value = serde_json::json!({
            "model": "gpt-4.1",
            "input": "hi"
        });
        assert!(validate_create_response_body(&value).is_ok());
    }

    #[test]
    fn validate_create_response_body_rejects_invalid_type() {
        let value = serde_json::json!("nope");
        assert!(validate_create_response_body(&value).is_err());
    }

    #[test]
    fn validate_create_response_body_accepts_tools_and_choice() {
        let value = serde_json::json!({
            "model": "gpt-4.1",
            "input": "hi",
            "tools": [
                { "type": "function", "name": "echo" }
            ],
            "tool_choice": "auto"
        });
        let errors = validate_create_response_body(&value)
            .err()
            .unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_create_response_body_reports_invalid_tool_fields() {
        let value = serde_json::json!({
            "model": "gpt-4.1",
            "input": "hi",
            "tools": "nope",
            "tool_choice": { "type": "unknown" }
        });
        let errors = validate_create_response_body(&value)
            .err()
            .unwrap_or_default();
        assert!(
            errors.iter().any(|err| err.contains("tools must be")),
            "errors: {errors:?}"
        );
        assert!(
            errors.iter().any(|err| err.contains("tool_choice")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn validate_tool_param_accepts_function_tool() {
        let value = serde_json::json!({
            "type": "function",
            "name": "echo"
        });
        let errors = validate_responses_tool_param(&value)
            .err()
            .unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_tool_param_accepts_all_variants() {
        let variants = vec![
            serde_json::json!({ "type": "code_interpreter", "container": "cntr_123" }),
            serde_json::json!({ "type": "code_interpreter", "container": { "type": "auto" } }),
            serde_json::json!({ "type": "custom", "name": "custom_tool" }),
            serde_json::json!({ "type": "web_search" }),
            serde_json::json!({ "type": "web_search_2025_08_26" }),
            serde_json::json!({ "type": "web_search_ga" }),
            serde_json::json!({ "type": "web_search_preview" }),
            serde_json::json!({ "type": "web_search_preview_2025_03_11" }),
            serde_json::json!({ "type": "image_generation" }),
            serde_json::json!({ "type": "mcp", "server_label": "srv" }),
            serde_json::json!({ "type": "file_search", "vector_store_ids": ["vs_1"] }),
            serde_json::json!({
                "type": "computer-preview",
                "display_width": 1024,
                "display_height": 768,
                "environment": "linux"
            }),
            serde_json::json!({
                "type": "computer_use_preview",
                "display_width": 800,
                "display_height": 600,
                "environment": "browser"
            }),
            serde_json::json!({ "type": "local_shell" }),
            serde_json::json!({ "type": "shell" }),
            serde_json::json!({ "type": "apply_patch" }),
        ];

        for value in variants {
            let errors = validate_responses_tool_param(&value)
                .err()
                .unwrap_or_default();
            assert!(errors.is_empty(), "errors: {errors:?} for {value}");
        }
    }

    #[test]
    fn validate_tool_param_rejects_invalid() {
        let value = serde_json::json!(42);
        assert!(validate_responses_tool_param(&value).is_err());
    }

    #[test]
    fn validate_tool_param_rejects_invalid_optional_fields() {
        let value = serde_json::json!({
            "type": "file_search",
            "vector_store_ids": ["vs_1"],
            "max_num_results": "nope"
        });
        assert!(validate_responses_tool_param(&value).is_err());

        let value = serde_json::json!({
            "type": "custom",
            "name": "custom_tool",
            "format": {
                "type": "grammar",
                "syntax": "bad",
                "definition": "start: /[a-z]+/"
            }
        });
        assert!(validate_responses_tool_param(&value).is_err());

        let value = serde_json::json!({
            "type": "web_search",
            "user_location": {
                "type": "approximate",
                "country": 123
            }
        });
        assert!(validate_responses_tool_param(&value).is_err());
    }

    #[test]
    fn validate_tool_choice_param_accepts_auto() {
        let value = serde_json::json!("auto");
        let errors = validate_tool_choice_param(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_tool_choice_param_accepts_specific_function() {
        let value = serde_json::json!({
            "type": "function",
            "name": "echo"
        });
        let errors = validate_tool_choice_param(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_tool_choice_param_accepts_specific_variants() {
        let variants = vec![
            serde_json::json!({ "type": "file_search" }),
            serde_json::json!({ "type": "web_search" }),
            serde_json::json!({ "type": "web_search_preview" }),
            serde_json::json!({ "type": "image_generation" }),
            serde_json::json!({ "type": "computer-preview" }),
            serde_json::json!({ "type": "computer_use_preview" }),
            serde_json::json!({ "type": "code_interpreter" }),
            serde_json::json!({ "type": "local_shell" }),
            serde_json::json!({ "type": "shell" }),
            serde_json::json!({ "type": "apply_patch" }),
            serde_json::json!({ "type": "custom", "name": "custom_tool" }),
            serde_json::json!({ "type": "mcp", "server_label": "srv" }),
        ];

        for value in variants {
            let errors = validate_tool_choice_param(&value).err().unwrap_or_default();
            assert!(errors.is_empty(), "errors: {errors:?} for {value}");
        }
    }

    #[test]
    fn validate_tool_choice_param_accepts_allowed_tools() {
        let value = serde_json::json!({
            "type": "allowed_tools",
            "tools": [
                { "type": "function", "name": "echo" }
            ]
        });
        let errors = validate_tool_choice_param(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_tool_choice_param_rejects_invalid() {
        let value = serde_json::json!(false);
        assert!(validate_tool_choice_param(&value).is_err());
    }

    #[test]
    fn validate_item_param_accepts_user_message() {
        let value = serde_json::json!({
            "type": "message",
            "role": "user",
            "content": "hi"
        });
        let errors = validate_item_param(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_item_param_accepts_all_variants() {
        let variants = vec![
            serde_json::json!({ "type": "message", "role": "assistant", "content": "hi" }),
            serde_json::json!({ "type": "message", "role": "developer", "content": "hi" }),
            serde_json::json!({ "type": "message", "role": "system", "content": "hi" }),
            serde_json::json!({ "type": "message", "role": "user", "content": "hi" }),
            serde_json::json!({ "type": "function_call", "call_id": "c1", "name": "echo", "arguments": "{}" }),
            serde_json::json!({ "type": "function_call_output", "call_id": "c1", "output": "ok" }),
            serde_json::json!({ "type": "reasoning", "summary": [] }),
            serde_json::json!({ "type": "compaction", "encrypted_content": "enc" }),
            serde_json::json!({ "type": "code_interpreter_call", "id": "ci1", "container_id": "cntr_1", "code": "print(1)" }),
            serde_json::json!({ "type": "computer_call", "call_id": "cc1", "action": {} }),
            serde_json::json!({ "type": "computer_call_output", "call_id": "cc1", "output": {} }),
            serde_json::json!({ "type": "custom_tool_call", "call_id": "ct1", "name": "tool", "input": "in" }),
            serde_json::json!({ "type": "custom_tool_call_output", "call_id": "ct1", "output": "out" }),
            serde_json::json!({ "type": "file_search_call", "id": "fs1", "queries": ["q1"] }),
            serde_json::json!({ "type": "web_search_call" }),
            serde_json::json!({ "type": "image_generation_call", "id": "ig1" }),
            serde_json::json!({ "type": "local_shell_call", "call_id": "ls1", "action": {} }),
            serde_json::json!({ "type": "local_shell_call_output", "call_id": "ls1", "output": "ok" }),
            serde_json::json!({ "type": "shell_call", "call_id": "sh1", "action": {} }),
            serde_json::json!({ "type": "shell_call_output", "call_id": "sh1", "output": [] }),
            serde_json::json!({ "type": "apply_patch_call", "call_id": "ap1", "status": "in_progress", "operation": {} }),
            serde_json::json!({ "type": "apply_patch_call_output", "call_id": "ap1", "status": "completed" }),
            serde_json::json!({ "type": "mcp_approval_request", "server_label": "srv", "name": "tool", "arguments": "{}" }),
            serde_json::json!({ "type": "mcp_approval_response", "approval_request_id": "ar1", "approve": true }),
            serde_json::json!({ "id": "item_1" }),
            serde_json::json!({ "type": "item_reference", "id": "item_2" }),
        ];

        for value in variants {
            let errors = validate_item_param(&value).err().unwrap_or_default();
            assert!(errors.is_empty(), "errors: {errors:?} for {value}");
        }
    }

    #[test]
    fn validate_item_param_rejects_invalid_type() {
        let value = serde_json::json!("nope");
        assert!(validate_item_param(&value).is_err());
    }

    #[test]
    fn validate_item_param_reports_missing_required_fields() {
        let value = serde_json::json!({ "type": "function_call" });
        let errors = validate_item_param(&value).err().unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("missing required field `call_id`")),
            "errors: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.contains("missing required field `name`")),
            "errors: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.contains("missing required field `arguments`")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn validate_item_param_reports_invalid_field_types() {
        let message = serde_json::json!({
            "type": "message",
            "role": 123,
            "content": 456
        });
        let errors = validate_item_param(&message).err().unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("role must be a string")),
            "errors: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.contains("content must be a string or array")),
            "errors: {errors:?}"
        );

        let invalid_role = serde_json::json!({
            "type": "message",
            "role": "invalid",
            "content": "hi"
        });
        let errors = validate_item_param(&invalid_role).err().unwrap_or_default();
        assert!(
            errors.iter().any(|err| err.contains("role must be one of")),
            "errors: {errors:?}"
        );

        let reasoning = serde_json::json!({
            "type": "reasoning",
            "summary": "nope"
        });
        let errors = validate_item_param(&reasoning).err().unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("summary must be an array")),
            "errors: {errors:?}"
        );

        let computer_call = serde_json::json!({
            "type": "computer_call",
            "call_id": "cc1",
            "action": "nope"
        });
        let errors = validate_item_param(&computer_call)
            .err()
            .unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("action must be an object")),
            "errors: {errors:?}"
        );

        let approval = serde_json::json!({
            "type": "mcp_approval_response",
            "approval_request_id": "ar1",
            "approve": "yes"
        });
        let errors = validate_item_param(&approval).err().unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("approve must be a boolean")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn validate_item_param_reports_file_search_query_errors() {
        let empty_queries = serde_json::json!({
            "type": "file_search_call",
            "id": "fs1",
            "queries": []
        });
        let errors = validate_item_param(&empty_queries)
            .err()
            .unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("queries must not be empty")),
            "errors: {errors:?}"
        );

        let bad_query = serde_json::json!({
            "type": "file_search_call",
            "id": "fs1",
            "queries": ["ok", 1]
        });
        let errors = validate_item_param(&bad_query).err().unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("queries[1] must be a string")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn validate_item_param_reports_item_reference_errors() {
        let value = serde_json::json!({
            "type": 123,
            "id": "item_1"
        });
        let errors = validate_item_param(&value).err().unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("ItemReferenceParam.type must be a string or null")),
            "errors: {errors:?}"
        );

        let missing_id = serde_json::json!({
            "type": null
        });
        let errors = validate_item_param(&missing_id).err().unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("missing required field `id`")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn validate_item_param_reports_unknown_type() {
        let value = serde_json::json!({
            "type": "unknown"
        });
        let errors = validate_item_param(&value).err().unwrap_or_default();
        assert!(
            errors.iter().any(|err| err.contains("unsupported value")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn validate_item_param_reports_additional_type_errors() {
        let function_call = serde_json::json!({
            "type": "function_call",
            "call_id": 1,
            "name": 2,
            "arguments": 3
        });
        let errors = validate_item_param(&function_call)
            .err()
            .unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("call_id must be a string")),
            "errors: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.contains("name must be a string")),
            "errors: {errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|err| err.contains("arguments must be a string")),
            "errors: {errors:?}"
        );

        let function_call_output = serde_json::json!({
            "type": "function_call_output",
            "call_id": "c1",
            "output": { "ok": true }
        });
        let errors = validate_item_param(&function_call_output)
            .err()
            .unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("output must be a string or array")),
            "errors: {errors:?}"
        );

        let file_search_call = serde_json::json!({
            "type": "file_search_call",
            "id": "fs1",
            "queries": "nope"
        });
        let errors = validate_item_param(&file_search_call)
            .err()
            .unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("queries must be an array")),
            "errors: {errors:?}"
        );

        let custom_tool_call_output = serde_json::json!({
            "type": "custom_tool_call_output",
            "call_id": "ct1",
            "output": 1
        });
        let errors = validate_item_param(&custom_tool_call_output)
            .err()
            .unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("output must be a string")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn validate_helper_error_paths() {
        let reference = serde_json::json!({
            "type": "not_item_reference",
            "id": "item_1"
        });
        let errors = validate_item_reference(reference.as_object().unwrap(), reference.get("type"))
            .err()
            .unwrap_or_default();
        assert!(
            errors
                .iter()
                .any(|err| err.contains("ItemReferenceParam.type must be")),
            "errors: {errors:?}"
        );

        let errors = validate_specific_tool_choice_param(&serde_json::json!({ "name": "tool" }))
            .err()
            .unwrap_or_default();
        assert!(!errors.is_empty(), "errors: {errors:?}");

        let errors = validate_specific_tool_choice_param(&serde_json::json!({ "type": "unknown" }))
            .err()
            .unwrap_or_default();
        assert!(!errors.is_empty(), "errors: {errors:?}");

        let errors = validate_specific_tool_choice_param(&serde_json::json!("nope"))
            .err()
            .unwrap_or_default();
        assert!(!errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_user_message_item_schema_accepts_simple() {
        let schema =
            extract_component_schema("UserMessageItemParam").expect("UserMessageItemParam schema");
        let validator = JSONSchema::options()
            .with_document("urn:openresponses:openapi".to_string(), OPENAPI.clone())
            .compile(&schema)
            .expect("compile user message schema");
        let value = serde_json::json!({
            "type": "message",
            "role": "user",
            "content": "hi"
        });
        let errors = match validator.validate(&value) {
            Ok(_) => Vec::new(),
            Err(errs) => errs.map(|e| e.to_string()).collect(),
        };
        assert!(errors.is_empty(), "errors: {errors:?}");
    }
}
