use super::*;

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
