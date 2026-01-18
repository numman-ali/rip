use super::*;

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
