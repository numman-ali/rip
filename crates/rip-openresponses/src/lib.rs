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

    fn schema_errors(name: &str, value: Value) -> Vec<String> {
        let schema = compile_split_schema(name);
        let errors = match schema.validate(&value) {
            Ok(_) => Vec::new(),
            Err(errors) => errors.map(|err| err.to_string()).collect(),
        };
        errors
    }

    fn openapi_schema_errors(name: &str, value: Value) -> Vec<String> {
        let root_ref = serde_json::json!({
            "$ref": format!("urn:openresponses:openapi#/components/schemas/{name}")
        });
        let validator = JSONSchema::options()
            .with_document("urn:openresponses:openapi".to_string(), OPENAPI.clone())
            .compile(&root_ref)
            .unwrap_or_else(|_| panic!("compile openapi schema {name}"));
        let errors = match validator.validate(&value) {
            Ok(_) => Vec::new(),
            Err(errors) => errors.map(|err| err.to_string()).collect(),
        };
        errors
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
        let value = serde_json::json!({
            "type": "memory",
            "memory": "remember this",
            "environment": {
                "type": "local_file",
                "root": "/tmp"
            }
        });
        let errors = schema_errors("MemoryToolParam.json", value);
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_response_resource_accepts_mcp_approval_items() {
        let request = serde_json::json!({
            "type": "mcp_approval_request",
            "id": "req_1",
            "server_label": "srv",
            "name": "tool",
            "arguments": "{}"
        });
        let response = serde_json::json!({
            "type": "mcp_approval_response",
            "id": "resp_1",
            "approval_request_id": "req_1",
            "approve": true,
            "reason": null
        });
        for item in [request, response] {
            let value = response_with_output(vec![item.clone()]);
            let errors = validate_response_resource(&value).err().unwrap_or_default();
            assert!(errors.is_empty(), "errors: {errors:?} for {item}");
        }
    }

    #[test]
    fn validate_response_resource_accepts_mcp_tool_calls() {
        let base = serde_json::json!({
            "type": "mcp_call",
            "id": "call_1",
            "status": "completed",
            "approval_request_id": null,
            "server_label": "srv",
            "name": "tool",
            "arguments": "{}",
            "output": null,
            "error": null
        });
        let value = response_with_output(vec![base]);
        let errors = validate_response_resource(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?}");

        let error_variants = vec![
            serde_json::json!({
                "type": "mcp_protocol_error",
                "code": 1,
                "message": "oops"
            }),
            serde_json::json!({
                "type": "mcp_tool_execution_error",
                "content": { "detail": "failed" }
            }),
            serde_json::json!({
                "type": "http_error",
                "code": 500,
                "message": "server"
            }),
        ];

        for error in error_variants {
            let item = serde_json::json!({
                "type": "mcp_call",
                "id": "call_2",
                "status": "failed",
                "approval_request_id": null,
                "server_label": "srv",
                "name": "tool",
                "arguments": "{}",
                "output": null,
                "error": error
            });
            let value = response_with_output(vec![item.clone()]);
            let errors = validate_response_resource(&value).err().unwrap_or_default();
            assert!(errors.is_empty(), "errors: {errors:?} for {item}");
        }
    }

    #[test]
    fn validate_mcp_filter_and_require_approval_schemas() {
        let errors = schema_errors(
            "MCPToolFilterField.json",
            serde_json::json!({
                "tool_names": ["tool_a"],
                "read_only": null
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "MCPToolFilterParam.json",
            serde_json::json!({
                "tool_names": ["tool_a"],
                "read_only": true
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "MCPRequireApprovalApiEnum.json",
            serde_json::json!("always"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "MCPRequireApprovalFieldEnum.json",
            serde_json::json!("never"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "MCPRequireApprovalFilterField.json",
            serde_json::json!({
                "always": null,
                "never": null
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "MCPRequireApprovalFilterParam.json",
            serde_json::json!({
                "always": { "tool_names": ["tool_b"], "read_only": false }
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("MCPToolCallStatus.json", serde_json::json!("completed"));
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_mcp_error_schemas() {
        let errors = schema_errors(
            "MCPProtocolError.json",
            serde_json::json!({
                "type": "mcp_protocol_error",
                "code": 400,
                "message": "bad request"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "MCPToolExecutionError.json",
            serde_json::json!({
                "type": "mcp_tool_execution_error",
                "content": { "detail": "fail" }
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "HTTPError.json",
            serde_json::json!({
                "type": "http_error",
                "code": 500,
                "message": "server error"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_response_resource_accepts_shell_items() {
        let local_call = serde_json::json!({
            "type": "local_shell_call",
            "id": "ls_1",
            "call_id": "call_1",
            "action": {
                "type": "exec",
                "command": ["echo", "hi"],
                "env": {}
            },
            "status": "in_progress"
        });
        let local_output = serde_json::json!({
            "type": "local_shell_call_output",
            "id": "ls_out_1",
            "call_id": "call_1",
            "output": "{\"stdout\":\"hi\"}",
            "status": "completed"
        });
        let shell_call = serde_json::json!({
            "type": "shell_call",
            "id": "sh_1",
            "call_id": "call_2",
            "action": {
                "commands": ["ls"],
                "timeout_ms": null,
                "max_output_length": null
            },
            "status": "completed"
        });
        let shell_output = serde_json::json!({
            "type": "shell_call_output",
            "id": "sh_out_1",
            "call_id": "call_2",
            "output": [
                {
                    "stdout": "",
                    "stderr": "",
                    "outcome": {
                        "type": "exit",
                        "exit_code": 0
                    }
                }
            ],
            "max_output_length": null
        });

        for item in [local_call, local_output, shell_call, shell_output] {
            let value = response_with_output(vec![item.clone()]);
            let errors = validate_response_resource(&value).err().unwrap_or_default();
            assert!(errors.is_empty(), "errors: {errors:?} for {item}");
        }
    }

    #[test]
    fn validate_shell_param_schemas() {
        let errors = schema_errors(
            "LocalShellExecActionParam.json",
            serde_json::json!({
                "type": "exec",
                "command": ["echo", "hi"],
                "env": {}
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "LocalShellCallItemStatus.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("LocalShellCallStatus.json", serde_json::json!("completed"));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "LocalShellCallOutputStatusEnum.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FunctionShellActionParam.json",
            serde_json::json!({
                "commands": ["ls"],
                "timeout_ms": null,
                "max_output_length": null
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FunctionShellCallItemStatus.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FunctionShellCallOutputContentParam.json",
            serde_json::json!({
                "stdout": "",
                "stderr": "",
                "outcome": {
                    "type": "exit",
                    "exit_code": 0
                }
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FunctionShellCallOutputOutcomeParam.json",
            serde_json::json!({ "type": "timeout" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FunctionShellCallOutputExitOutcomeParam.json",
            serde_json::json!({ "type": "exit", "exit_code": 0 }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FunctionShellCallOutputTimeoutOutcomeParam.json",
            serde_json::json!({ "type": "timeout" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_file_search_and_status_schemas() {
        let errors = schema_errors("RankerVersionType.json", serde_json::json!("auto"));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "HybridSearchOptionsParam.json",
            serde_json::json!({ "embedding_weight": 0.4, "text_weight": 0.6 }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "HybridSearchOptions.json",
            serde_json::json!({ "embedding_weight": 0.4, "text_weight": 0.6 }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FileSearchRankingOptionsParam.json",
            serde_json::json!({
                "ranker": "auto",
                "score_threshold": 0.2,
                "hybrid_search": { "embedding_weight": 0.4, "text_weight": 0.6 }
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FileSearchRetrievedChunksParam.json",
            serde_json::json!({
                "file_id": "file_1",
                "filename": "notes.txt",
                "text": "chunk",
                "attributes": {},
                "score": 0.1,
                "vector_store_id": null
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FileSearchResult.json",
            serde_json::json!({
                "file_id": "file_1",
                "filename": "notes.txt",
                "text": "chunk",
                "attributes": {},
                "score": 0.1,
                "vector_store_id": null
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FileSearchToolCallStatusEnum.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("FunctionCallStatus.json", serde_json::json!("completed"));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FunctionCallOutputStatusEnum.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FunctionCallItemStatus.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FunctionShellCallItemStatus.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FunctionShellCallOutputExitOutcome.json",
            serde_json::json!({ "type": "exit", "exit_code": 0 }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FunctionShellCallOutputTimeoutOutcome.json",
            serde_json::json!({ "type": "timeout" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "FunctionShellCallOutputContent.json",
            serde_json::json!({
                "stdout": "",
                "stderr": "",
                "outcome": { "type": "exit", "exit_code": 0 }
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_response_resource_accepts_search_and_tool_calls() {
        let file_search = serde_json::json!({
            "type": "file_search_call",
            "id": "fs_1",
            "status": "completed",
            "queries": ["query"],
            "results": [
                {
                    "file_id": "file_1",
                    "filename": "notes.txt",
                    "text": "hello",
                    "attributes": {},
                    "score": 0.1,
                    "vector_store_id": null
                }
            ]
        });
        let web_search = serde_json::json!({
            "type": "web_search_call",
            "id": "ws_1",
            "status": "completed",
            "action": {
                "type": "search",
                "query": null,
                "queries": ["query"]
            }
        });
        let image_gen = serde_json::json!({
            "type": "image_generation_call",
            "id": "ig_1",
            "status": "completed"
        });
        let computer_call = serde_json::json!({
            "type": "computer_call",
            "id": "cc_1",
            "call_id": "call_1",
            "pending_safety_checks": []
        });
        let computer_output = serde_json::json!({
            "type": "computer_call_output",
            "id": "cc_out_1",
            "call_id": "call_1",
            "output": { "type": "input_text", "text": "ok" },
            "status": "completed",
            "current_url": null
        });
        let apply_patch_call = serde_json::json!({
            "type": "apply_patch_call",
            "id": "ap_1",
            "call_id": "call_2",
            "status": "completed",
            "operation": {
                "type": "create_file",
                "path": "notes.txt",
                "diff": "@@ -0,0 +1 @@\\n+hello\\n"
            }
        });
        let apply_patch_output = serde_json::json!({
            "type": "apply_patch_call_output",
            "id": "ap_out_1",
            "call_id": "call_2",
            "status": "completed",
            "output": null
        });

        for item in [
            file_search,
            web_search,
            image_gen,
            computer_call,
            computer_output,
            apply_patch_call,
            apply_patch_output,
        ] {
            let value = response_with_output(vec![item.clone()]);
            let errors = validate_response_resource(&value).err().unwrap_or_default();
            assert!(errors.is_empty(), "errors: {errors:?} for {item}");
        }
    }

    #[test]
    fn validate_search_and_tool_param_schemas() {
        let errors = schema_errors(
            "FileSearchToolCallStatusEnum.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("WebSearchCallStatus.json", serde_json::json!("completed"));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "WebSearchCallActionSearchParam.json",
            serde_json::json!({
                "type": "search",
                "query": null,
                "queries": ["q"],
                "sources": [
                    { "type": "url", "url": "https://example.com" },
                    { "type": "api", "name": "internal" }
                ]
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "WebSearchCallActionOpenPageParam.json",
            serde_json::json!({
                "type": "open_page",
                "url": null
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "WebSearchCallActionFindInPageParam.json",
            serde_json::json!({
                "type": "find_in_page",
                "url": null,
                "pattern": null
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComputerCallOutputStatus.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComputerCallSafetyCheckParam.json",
            serde_json::json!({ "id": "sc_1" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("ImageGenCallStatus.json", serde_json::json!("completed"));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("ImageGenAction.json", serde_json::json!("generate"));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("ApplyPatchCallStatus.json", serde_json::json!("completed"));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ApplyPatchCallStatusParam.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ApplyPatchCallOutputStatus.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ApplyPatchCallOutputStatusParam.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ApplyPatchOperationParam.json",
            serde_json::json!({
                "type": "update_file",
                "path": "notes.txt",
                "diff": "@@ -1 +1 @@\\n-hello\\n+hi\\n"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ApplyPatchCreateFileOperationParam.json",
            serde_json::json!({
                "type": "create_file",
                "path": "notes.txt",
                "diff": "@@ -0,0 +1 @@\\n+hello\\n"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ApplyPatchDeleteFileOperationParam.json",
            serde_json::json!({
                "type": "delete_file",
                "path": "notes.txt"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ApplyPatchUpdateFileOperationParam.json",
            serde_json::json!({
                "type": "update_file",
                "path": "notes.txt",
                "diff": "@@ -1 +1 @@\\n-hello\\n+hi\\n"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_response_resource_accepts_code_interpreter_call() {
        let item = serde_json::json!({
            "type": "code_interpreter_call",
            "id": "ci_1",
            "status": "completed",
            "container_id": "cntr_1",
            "code": null,
            "outputs": [
                {
                    "type": "logs",
                    "logs": "ok"
                }
            ]
        });
        let value = response_with_output(vec![item.clone()]);
        let errors = validate_response_resource(&value).err().unwrap_or_default();
        assert!(errors.is_empty(), "errors: {errors:?} for {item}");
    }

    #[test]
    fn validate_code_interpreter_param_schemas() {
        let errors = schema_errors(
            "CodeInterpreterCallStatus.json",
            serde_json::json!("completed"),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "CodeInterpreterOutputLogs.json",
            serde_json::json!({
                "type": "logs",
                "logs": "ok"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "CodeInterpreterOutputImage.json",
            serde_json::json!({
                "type": "image",
                "url": "https://example.com/img.png"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "CodeInterpreterToolCallOutputLogsParam.json",
            serde_json::json!({
                "type": "logs",
                "logs": "ok"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "CodeInterpreterToolCallOutputImageParam.json",
            serde_json::json!({
                "type": "image",
                "url": "https://example.com/img.png"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_content_block_schemas() {
        let errors = schema_errors(
            "InputTextContent.json",
            serde_json::json!({ "type": "input_text", "text": "hi" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "InputTextContentParam.json",
            serde_json::json!({ "type": "input_text", "text": "hi" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "InputImageContent.json",
            serde_json::json!({
                "type": "input_image",
                "image_url": "https://example.com/image.png",
                "file_id": null,
                "detail": "auto"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "InputImageContentParamAutoParam.json",
            serde_json::json!({
                "type": "input_image",
                "image_url": "https://example.com/image.png",
                "file_id": null,
                "detail": "auto"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("InputImageMaskContentParam.json", serde_json::json!({}));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "InputFileContent.json",
            serde_json::json!({
                "type": "input_file",
                "file_id": "file_1",
                "filename": "notes.txt"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "InputFileContentParam.json",
            serde_json::json!({
                "type": "input_file",
                "file_id": "file_1"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = openapi_schema_errors(
            "InputVideoContent",
            serde_json::json!({
                "type": "input_video",
                "video_url": "https://example.com/video.mp4"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "OutputTextContent.json",
            serde_json::json!({
                "type": "output_text",
                "text": "hi",
                "annotations": [],
                "logprobs": []
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "OutputTextContentParam.json",
            serde_json::json!({ "type": "output_text", "text": "hi" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "TextContent.json",
            serde_json::json!({ "type": "text", "text": "hi" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "SummaryTextContent.json",
            serde_json::json!({ "type": "summary_text", "text": "ok" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ReasoningTextContent.json",
            serde_json::json!({ "type": "reasoning_text", "text": "ok" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "RefusalContent.json",
            serde_json::json!({ "type": "refusal", "refusal": "nope" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ReasoningSummaryContentParam.json",
            serde_json::json!({ "type": "summary_text", "text": "ok" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_annotation_schemas() {
        let file_citation = serde_json::json!({
            "type": "file_citation",
            "file_id": "file_1",
            "index": 0,
            "filename": "notes.txt"
        });
        let errors = schema_errors("FileCitationBody.json", file_citation.clone());
        assert!(errors.is_empty(), "errors: {errors:?}");

        let url_citation = serde_json::json!({
            "type": "url_citation",
            "url": "https://example.com",
            "start_index": 0,
            "end_index": 10,
            "title": "Example"
        });
        let errors = schema_errors("UrlCitationBody.json", url_citation.clone());
        assert!(errors.is_empty(), "errors: {errors:?}");

        let container_citation = serde_json::json!({
            "type": "container_file_citation",
            "container_id": "cntr_1",
            "file_id": "file_1",
            "start_index": 0,
            "end_index": 4,
            "filename": "doc.txt"
        });
        let errors = schema_errors("ContainerFileCitationBody.json", container_citation.clone());
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("Annotation.json", file_citation);
        assert!(errors.is_empty(), "errors: {errors:?}");

        let file_citation_param = serde_json::json!({
            "type": "file_citation",
            "index": 0,
            "file_id": "file_1",
            "filename": "notes.txt"
        });
        let errors = schema_errors("FileCitationParam.json", file_citation_param.clone());
        assert!(errors.is_empty(), "errors: {errors:?}");

        let url_citation_param = serde_json::json!({
            "type": "url_citation",
            "start_index": 0,
            "end_index": 10,
            "url": "https://example.com",
            "title": "Example"
        });
        let errors = schema_errors("UrlCitationParam.json", url_citation_param.clone());
        assert!(errors.is_empty(), "errors: {errors:?}");

        let container_citation_param = serde_json::json!({
            "type": "container_file_citation",
            "start_index": 0,
            "end_index": 4,
            "container_id": "cntr_1",
            "file_id": "file_1",
            "filename": "doc.txt"
        });
        let errors = schema_errors("ContainerFileCitationParam.json", container_citation_param);
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "OutputTextContentParam.json",
            serde_json::json!({
                "type": "output_text",
                "text": "hi",
                "annotations": [file_citation_param]
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "OutputTextContent.json",
            serde_json::json!({
                "type": "output_text",
                "text": "hi",
                "annotations": [url_citation],
                "logprobs": []
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_filter_schemas() {
        let eq_field = serde_json::json!({
            "type": "eq",
            "key": "tag",
            "value": "alpha"
        });
        let errors = schema_errors("ComparisonFilterFieldEQ.json", eq_field.clone());
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterFieldNE.json",
            serde_json::json!({ "type": "ne", "key": "tag", "value": "beta" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterFieldLT.json",
            serde_json::json!({ "type": "lt", "key": "score", "value": "10" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterFieldLTE.json",
            serde_json::json!({ "type": "lte", "key": "score", "value": "10" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterFieldGT.json",
            serde_json::json!({ "type": "gt", "key": "score", "value": "5" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterFieldGTE.json",
            serde_json::json!({ "type": "gte", "key": "score", "value": "5" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterFieldIN.json",
            serde_json::json!({ "type": "in", "key": "tag", "value": ["a", "b"] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterFieldNIN.json",
            serde_json::json!({ "type": "nin", "key": "tag", "value": ["x", "y"] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterFieldCONTAINS.json",
            serde_json::json!({ "type": "contains", "key": "tag", "value": "foo" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterFieldNCONTAINS.json",
            serde_json::json!({ "type": "ncontains", "key": "tag", "value": "bar" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterFieldCONTAINSANY.json",
            serde_json::json!({ "type": "containsany", "key": "tag", "value": ["foo", "bar"] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterFieldNCONTAINSANY.json",
            serde_json::json!({ "type": "ncontainsany", "key": "tag", "value": ["baz"] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "CompoundFilterFieldAND.json",
            serde_json::json!({ "type": "and", "filters": [eq_field.clone()] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "CompoundFilterFieldOR.json",
            serde_json::json!({ "type": "or", "filters": [eq_field.clone()] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("Filters.json", eq_field);
        assert!(errors.is_empty(), "errors: {errors:?}");

        let eq_param = serde_json::json!({
            "type": "eq",
            "key": "tag",
            "value": "alpha"
        });
        let errors = schema_errors("ComparisonFilterParamEQParam.json", eq_param.clone());
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterParamNEParam.json",
            serde_json::json!({ "type": "ne", "key": "tag", "value": "beta" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterParamLTParam.json",
            serde_json::json!({ "type": "lt", "key": "score", "value": "10" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterParamLTEParam.json",
            serde_json::json!({ "type": "lte", "key": "score", "value": "10" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterParamGTParam.json",
            serde_json::json!({ "type": "gt", "key": "score", "value": "5" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterParamGTEParam.json",
            serde_json::json!({ "type": "gte", "key": "score", "value": "5" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterParamINParam.json",
            serde_json::json!({ "type": "in", "key": "tag", "value": ["a", "b"] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterParamNINParam.json",
            serde_json::json!({ "type": "nin", "key": "tag", "value": ["x", "y"] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterParamContainsParam.json",
            serde_json::json!({ "type": "contains", "key": "tag", "value": "foo" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterParamNContainsParam.json",
            serde_json::json!({ "type": "ncontains", "key": "tag", "value": "bar" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterParamContainsAnyParam.json",
            serde_json::json!({ "type": "containsany", "key": "tag", "value": ["foo", "bar"] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComparisonFilterParamNContainsAnyParam.json",
            serde_json::json!({ "type": "ncontainsany", "key": "tag", "value": ["baz"] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "CompoundFilterParamAndParam.json",
            serde_json::json!({ "type": "and", "filters": [eq_param.clone()] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "CompoundFilterParamOrParam.json",
            serde_json::json!({ "type": "or", "filters": [eq_param] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_computer_action_schemas() {
        let errors = schema_errors("ComputerEnvironment.json", serde_json::json!("browser"));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("ComputerEnvironment1.json", serde_json::json!("ubuntu"));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComputerScreenshotContent.json",
            serde_json::json!({
                "type": "computer_screenshot",
                "image_url": "https://example.com/s.png",
                "file_id": null
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ComputerScreenshotParam.json",
            serde_json::json!({
                "type": "computer_screenshot",
                "image_url": "https://example.com/s.png",
                "file_id": null,
                "detail": "auto"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("DetailEnum.json", serde_json::json!("auto"));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("ClickButtonType.json", serde_json::json!("left"));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ClickAction.json",
            serde_json::json!({ "type": "click", "button": "left", "x": 10, "y": 20 }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ClickParam.json",
            serde_json::json!({ "type": "click", "button": "left", "x": 10, "y": 20 }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "DoubleClickAction.json",
            serde_json::json!({ "type": "double_click", "x": 10, "y": 20 }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "DoubleClickParam.json",
            serde_json::json!({ "type": "double_click", "x": 10, "y": 20 }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let coord = serde_json::json!({ "x": 1, "y": 2 });
        let errors = schema_errors("CoordParam.json", coord.clone());
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("DragPoint.json", coord.clone());
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "DragAction.json",
            serde_json::json!({ "type": "drag", "path": [coord.clone()] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "DragParam.json",
            serde_json::json!({ "type": "drag", "path": [coord] }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("EmptyAction.json", serde_json::json!({}));
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_context_and_billing_schemas() {
        let errors = schema_errors(
            "ApiSourceParam.json",
            serde_json::json!({ "type": "api", "name": "source" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("Payer.json", serde_json::json!("developer"));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("Billing.json", serde_json::json!({ "payer": "developer" }));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors("Conversation.json", serde_json::json!({ "id": "conv_1" }));
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ConversationParam.json",
            serde_json::json!({ "id": "conv_1" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ContextEditDetails.json",
            serde_json::json!({
                "cleared_input_tokens": 10,
                "cleared_tool_call_ids": ["call_1"]
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ContextEdit.json",
            serde_json::json!({
                "type": "truncate",
                "summary": "trimmed",
                "details": {
                    "cleared_input_tokens": 10,
                    "cleared_tool_call_ids": []
                }
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ApproximateLocation.json",
            serde_json::json!({
                "type": "approximate",
                "country": "US",
                "region": null,
                "city": null,
                "timezone": "America/Los_Angeles"
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "ApproximateLocationParam.json",
            serde_json::json!({ "type": "approximate", "country": "US" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");
    }

    #[test]
    fn validate_response_format_schemas() {
        let errors = schema_errors(
            "TextResponseFormat.json",
            serde_json::json!({ "type": "text" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "JsonObjectResponseFormat.json",
            serde_json::json!({ "type": "json_object" }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = schema_errors(
            "JsonSchemaResponseFormat.json",
            serde_json::json!({
                "type": "json_schema",
                "name": "schema",
                "description": null,
                "schema": {},
                "strict": false
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors = openapi_schema_errors(
            "JsonSchemaResponseFormatParam",
            serde_json::json!({
                "type": "json_schema",
                "name": "schema",
                "description": "desc",
                "schema": {},
                "strict": false
            }),
        );
        assert!(errors.is_empty(), "errors: {errors:?}");

        let errors =
            openapi_schema_errors("TextFormatParam", serde_json::json!({ "type": "text" }));
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
