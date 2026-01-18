use super::*;

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

    let errors = schema_errors(
        "KeyPressAction.json",
        serde_json::json!({ "type": "keypress", "keys": ["Enter"] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "KeyPressParam.json",
        serde_json::json!({ "type": "keypress", "keys": ["Enter"] }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "MoveAction.json",
        serde_json::json!({ "type": "move", "x": 10, "y": 20 }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "MoveParam.json",
        serde_json::json!({ "type": "move", "x": 10, "y": 20 }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ScrollAction.json",
        serde_json::json!({
            "type": "scroll",
            "x": 10,
            "y": 20,
            "scroll_x": 0,
            "scroll_y": 200
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ScrollParam.json",
        serde_json::json!({
            "type": "scroll",
            "x": 10,
            "y": 20,
            "scroll_x": 0,
            "scroll_y": 200
        }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ScreenshotAction.json",
        serde_json::json!({ "type": "screenshot" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "ScreenshotParam.json",
        serde_json::json!({ "type": "screenshot" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "TypeAction.json",
        serde_json::json!({ "type": "type", "text": "hello" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "TypeParam.json",
        serde_json::json!({ "type": "type", "text": "hello" }),
    );
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("WaitAction.json", serde_json::json!({ "type": "wait" }));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors("WaitParam.json", serde_json::json!({ "type": "wait" }));
    assert!(errors.is_empty(), "errors: {errors:?}");

    let errors = schema_errors(
        "SafetyCheck.json",
        serde_json::json!({ "id": "sc_1", "code": "safe", "message": "ok" }),
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
