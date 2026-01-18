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
mod tests;
