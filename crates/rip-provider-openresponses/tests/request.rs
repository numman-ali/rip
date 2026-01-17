use rip_provider_openresponses::{
    CreateResponseBuilder, CreateResponsePayload, ItemParam, ToolChoiceParam, ToolParam,
};
use serde_json::json;

#[test]
fn create_response_builder_minimal_is_valid() {
    let payload = CreateResponseBuilder::new()
        .model("gpt-4.1")
        .input_text("hi")
        .build();

    assert!(payload.errors().is_empty());
    assert_eq!(payload.body().get("model").unwrap(), "gpt-4.1");
    assert_eq!(payload.body().get("input").unwrap(), "hi");
}

#[test]
fn create_response_payload_captures_validation_errors() {
    let payload = CreateResponsePayload::new(json!("nope"));
    assert!(!payload.errors().is_empty());
}

#[test]
fn item_param_reports_validation_errors() {
    let item = ItemParam::new(json!("nope"));
    assert!(!item.errors().is_empty());
}

#[test]
fn item_param_value_roundtrip() {
    let item = ItemParam::new(json!({
        "type": "message",
        "role": "user",
        "content": "hi"
    }));
    assert_eq!(
        item.value(),
        &json!({
            "type": "message",
            "role": "user",
            "content": "hi"
        })
    );
    let value = item.clone().into_value();
    assert_eq!(
        value,
        json!({
            "type": "message",
            "role": "user",
            "content": "hi"
        })
    );
}

#[test]
fn tool_param_function_is_valid() {
    let tool = ToolParam::function("echo");
    assert!(tool.errors().is_empty());
}

#[test]
fn tool_param_invalid_reports_errors() {
    let tool = ToolParam::new(json!({ "type": "bogus" }));
    assert!(!tool.errors().is_empty());
    assert_eq!(tool.value(), &json!({ "type": "bogus" }));
}

#[test]
fn tool_choice_specific_function_is_valid() {
    let choice = ToolChoiceParam::specific_function("echo");
    assert!(choice.errors().is_empty());
}

#[test]
fn tool_choice_value_variants_roundtrip() {
    let none = ToolChoiceParam::none();
    assert_eq!(none.value(), &json!("none"));
    let required = ToolChoiceParam::required();
    assert_eq!(required.value(), &json!("required"));
    let raw = ToolChoiceParam::new(json!(true));
    assert!(!raw.errors().is_empty());
}

#[test]
fn create_response_builder_accepts_tool_fields() {
    let payload = CreateResponseBuilder::new()
        .model("gpt-4.1")
        .input_text("hi")
        .tools(vec![ToolParam::function("echo")])
        .tool_choice(ToolChoiceParam::auto())
        .parallel_tool_calls(true)
        .max_tool_calls(2)
        .build();

    assert!(payload.body().get("tools").is_some());
    assert!(payload.body().get("tool_choice").is_some());
    assert_eq!(
        payload.body().get("parallel_tool_calls").unwrap(),
        &json!(true)
    );
    assert_eq!(payload.body().get("max_tool_calls").unwrap(), &json!(2));
}

#[test]
fn create_response_builder_accepts_raw_inputs() {
    let payload = CreateResponseBuilder::new()
        .input_items(vec![ItemParam::new(json!({
            "type": "message",
            "role": "user",
            "content": "hi"
        }))])
        .input_items_raw(vec![json!({
            "type": "message",
            "role": "assistant",
            "content": "hello"
        })])
        .tools_raw(vec![json!({ "type": "function", "name": "echo" })])
        .tool_choice_raw(json!("none"))
        .insert_raw("metadata", json!({ "trace_id": "t1" }))
        .build();

    assert!(payload.body().get("input").is_some());
    assert!(payload.body().get("tools").is_some());
    assert_eq!(payload.body().get("tool_choice").unwrap(), &json!("none"));
    assert_eq!(
        payload.body().get("metadata").unwrap(),
        &json!({ "trace_id": "t1" })
    );
    let body = payload.clone().into_body();
    assert_eq!(body.get("metadata").unwrap(), &json!({ "trace_id": "t1" }));
}
