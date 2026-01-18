use super::*;

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
