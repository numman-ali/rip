use jsonschema::JSONSchema;
use once_cell::sync::Lazy;
use serde_json::Value;

const OPENAPI_URI: &str = "urn:openresponses:openapi";

static OPENAPI: Lazy<Value> = Lazy::new(|| {
    let raw = include_str!("../../../schemas/openresponses/openapi.json");
    serde_json::from_str(raw).expect("openapi.json valid")
});

static STREAM_EVENT_TYPES: Lazy<Vec<String>> = Lazy::new(|| {
    let raw = include_str!("../../../schemas/openresponses/streaming_event_types.json");
    serde_json::from_str(raw).expect("streaming_event_types.json valid")
});

static STREAM_SCHEMA: Lazy<Value> =
    Lazy::new(|| extract_streaming_schema().expect("streaming event schema not found"));

static RESPONSE_SCHEMA: Lazy<Value> = Lazy::new(|| {
    extract_component_schema("ResponseResource").expect("ResponseResource schema not found")
});

static CREATE_RESPONSE_SCHEMA: Lazy<Value> = Lazy::new(|| {
    extract_component_schema("CreateResponseBody").expect("CreateResponseBody schema not found")
});

static TOOL_PARAM_SCHEMA: Lazy<Value> = Lazy::new(|| {
    extract_component_schema("ResponsesToolParam").expect("ResponsesToolParam schema not found")
});

static TOOL_CHOICE_SCHEMA: Lazy<Value> = Lazy::new(|| {
    extract_component_schema("ToolChoiceParam").expect("ToolChoiceParam schema not found")
});

static ITEM_PARAM_SCHEMA: Lazy<Value> = Lazy::new(|| {
    let mut schema = extract_component_schema("ItemParam").expect("ItemParam schema not found");
    if let Some(obj) = schema.as_object_mut() {
        obj.remove("discriminator");
    }
    schema
});

static STREAM_VALIDATOR: Lazy<JSONSchema> = Lazy::new(|| {
    JSONSchema::options()
        .with_document(OPENAPI_URI.to_string(), OPENAPI.clone())
        .compile(&STREAM_SCHEMA)
        .expect("compile streaming schema")
});

static RESPONSE_VALIDATOR: Lazy<JSONSchema> = Lazy::new(|| {
    JSONSchema::options()
        .with_document(OPENAPI_URI.to_string(), OPENAPI.clone())
        .compile(&RESPONSE_SCHEMA)
        .expect("compile response schema")
});

static CREATE_RESPONSE_VALIDATOR: Lazy<JSONSchema> = Lazy::new(|| {
    JSONSchema::options()
        .with_document(OPENAPI_URI.to_string(), OPENAPI.clone())
        .compile(&CREATE_RESPONSE_SCHEMA)
        .expect("compile create response schema")
});

// jsonschema rejects ItemParam oneOf with multiple message roles; validate each variant directly.
static ITEM_PARAM_VARIANTS: Lazy<Vec<(String, JSONSchema)>> =
    Lazy::new(|| build_component_variant_validators("ItemParam"));

// ToolChoiceParam includes nested SpecificToolChoiceParam; expand to concrete variants.
static TOOL_CHOICE_VARIANTS: Lazy<Vec<(String, JSONSchema)>> = Lazy::new(|| {
    let mut variants = component_oneof_refs("ToolChoiceParam");
    if let Some(pos) = variants
        .iter()
        .position(|name| name == "SpecificToolChoiceParam")
    {
        variants.remove(pos);
        variants.extend(component_oneof_refs("SpecificToolChoiceParam"));
    }
    compile_component_validators(variants)
});

// ResponsesToolParam is a oneOf; validate each tool variant directly.
static TOOL_PARAM_VARIANTS: Lazy<Vec<(String, JSONSchema)>> =
    Lazy::new(|| build_component_variant_validators("ResponsesToolParam"));

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
    validate_variants("ResponsesToolParam", value, &TOOL_PARAM_VARIANTS)
}

pub fn validate_tool_choice_param(value: &Value) -> Result<(), Vec<String>> {
    validate_variants("ToolChoiceParam", value, &TOOL_CHOICE_VARIANTS)
}

pub fn validate_item_param(value: &Value) -> Result<(), Vec<String>> {
    validate_variants("ItemParam", value, &ITEM_PARAM_VARIANTS)
}

fn validate_variants(
    label: &str,
    value: &Value,
    variants: &[(String, JSONSchema)],
) -> Result<(), Vec<String>> {
    let mut matches = 0;
    let mut errors = Vec::new();
    for (name, validator) in variants.iter() {
        match validator.validate(value) {
            Ok(_) => matches += 1,
            Err(errs) => errors.extend(errs.map(|e| format!("{name}: {e}"))),
        }
    }
    if matches == 1 {
        Ok(())
    } else if matches == 0 {
        Err(errors)
    } else {
        Err(vec![format!(
            "{label} matches multiple schemas ({matches})"
        )])
    }
}

fn component_oneof_refs(component_name: &str) -> Vec<String> {
    let schema = extract_component_schema(component_name)
        .unwrap_or_else(|| panic!("schema missing: {component_name}"));
    schema
        .get("oneOf")
        .and_then(|value| value.as_array())
        .expect("oneOf variants")
        .iter()
        .filter_map(|item| item.get("$ref").and_then(|value| value.as_str()))
        .filter_map(|ref_name| ref_name.rsplit('/').next().map(|name| name.to_string()))
        .collect()
}

fn compile_component_validators(names: Vec<String>) -> Vec<(String, JSONSchema)> {
    names
        .into_iter()
        .map(|name| {
            let schema = serde_json::json!({
                "$ref": format!("{OPENAPI_URI}#/components/schemas/{name}")
            });
            let validator = JSONSchema::options()
                .with_document(OPENAPI_URI.to_string(), OPENAPI.clone())
                .compile(&schema)
                .expect("compile schema variant");
            (name, validator)
        })
        .collect()
}

fn build_component_variant_validators(component_name: &str) -> Vec<(String, JSONSchema)> {
    let variants = component_oneof_refs(component_name);
    compile_component_validators(variants)
}

fn extract_streaming_schema() -> Option<Value> {
    let pointer = "/paths/~1responses/post/responses/200/content/text~1event-stream/schema";
    OPENAPI.pointer(pointer).cloned()
}

fn extract_component_schema(name: &str) -> Option<Value> {
    let pointer = format!("/components/schemas/{name}");
    OPENAPI.pointer(&pointer).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn validate_response_resource_rejects_empty() {
        let value = serde_json::json!({});
        assert!(validate_response_resource(&value).is_err());
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
    fn validate_tool_param_rejects_invalid() {
        let value = serde_json::json!(42);
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
    fn validate_item_param_rejects_invalid_type() {
        let value = serde_json::json!("nope");
        assert!(validate_item_param(&value).is_err());
    }

    #[test]
    fn validate_user_message_item_schema_accepts_simple() {
        let schema =
            extract_component_schema("UserMessageItemParam").expect("UserMessageItemParam schema");
        let validator = JSONSchema::options()
            .with_document(OPENAPI_URI.to_string(), OPENAPI.clone())
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
