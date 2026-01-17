use rip_provider_openresponses::{
    CreateResponseBuilder, CreateResponsePayload, ItemParam, SpecificToolChoiceParam,
    ToolChoiceParam, ToolChoiceValue, ToolParam,
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
fn item_param_variants_are_valid() {
    let items = vec![
        ItemParam::item_reference("item_1"),
        ItemParam::user_message(json!("hi")),
        ItemParam::user_message_text("hi"),
        ItemParam::assistant_message_text("hello"),
        ItemParam::assistant_message(json!("hello")),
        ItemParam::developer_message_text("note"),
        ItemParam::developer_message(json!("note")),
        ItemParam::system_message_text("sys"),
        ItemParam::system_message(json!("sys")),
        ItemParam::function_call("c1", "echo", "{}"),
        ItemParam::function_call_output("c1", json!("ok")),
        ItemParam::reasoning(Vec::new()),
        ItemParam::compaction("enc"),
        ItemParam::code_interpreter_call("ci1", "cntr_1", "print(1)"),
        ItemParam::computer_call("cc1", json!({})),
        ItemParam::computer_call_output("cc1", json!({})),
        ItemParam::custom_tool_call("ct1", "tool", "input"),
        ItemParam::custom_tool_call_output("ct1", "out"),
        ItemParam::file_search_call("fs1", vec!["q1".to_string()]),
        ItemParam::web_search_call(),
        ItemParam::image_generation_call("ig1"),
        ItemParam::local_shell_call("ls1", json!({})),
        ItemParam::local_shell_call_output("ls1", "output"),
        ItemParam::shell_call("sh1", json!({})),
        ItemParam::shell_call_output("sh1", Vec::new()),
        ItemParam::apply_patch_call("ap1", "in_progress", json!({})),
        ItemParam::apply_patch_call_output("ap1", "completed"),
        ItemParam::mcp_approval_request("srv", "tool", "{}"),
        ItemParam::mcp_approval_response("req1", true),
    ];

    for item in items {
        assert!(item.errors().is_empty());
    }
}

#[test]
fn tool_param_function_is_valid() {
    let tool = ToolParam::function("echo");
    assert!(tool.errors().is_empty());
}

#[test]
fn tool_param_variants_are_valid() {
    let tools = vec![
        ToolParam::code_interpreter("cntr_123"),
        ToolParam::code_interpreter_auto(Some(vec!["file_1".to_string()]), Some("4g".to_string())),
        ToolParam::custom("custom"),
        ToolParam::web_search(),
        ToolParam::web_search_2025_08_26(),
        ToolParam::web_search_ga(),
        ToolParam::web_search_preview(),
        ToolParam::web_search_preview_2025_03_11(),
        ToolParam::image_generation(),
        ToolParam::mcp("srv"),
        ToolParam::file_search(vec!["vs_1".to_string()]),
        ToolParam::computer_preview(1024, 768, "linux"),
        ToolParam::computer_use_preview(800, 600, "browser"),
        ToolParam::local_shell(),
        ToolParam::shell(),
        ToolParam::apply_patch(),
    ];

    for tool in tools {
        assert!(tool.errors().is_empty());
    }
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
fn tool_choice_specific_variants_are_valid() {
    let choices = vec![
        ToolChoiceParam::specific_file_search(),
        ToolChoiceParam::specific_web_search(),
        ToolChoiceParam::specific_web_search_preview(),
        ToolChoiceParam::specific_image_generation(),
        ToolChoiceParam::specific_computer_preview(),
        ToolChoiceParam::specific_computer_use_preview(),
        ToolChoiceParam::specific_code_interpreter(),
        ToolChoiceParam::specific_local_shell(),
        ToolChoiceParam::specific_shell(),
        ToolChoiceParam::specific_apply_patch(),
        ToolChoiceParam::specific_custom("custom"),
        ToolChoiceParam::specific_mcp("srv"),
    ];

    for choice in choices {
        assert!(choice.errors().is_empty());
    }
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
fn tool_choice_allowed_tools_with_mode_is_valid() {
    let choice = ToolChoiceParam::allowed_tools_with_mode(
        vec![
            SpecificToolChoiceParam::function("echo"),
            SpecificToolChoiceParam::web_search(),
        ],
        Some(ToolChoiceValue::Required),
    );
    assert!(choice.errors().is_empty());
}

#[test]
fn tool_choice_allowed_tools_without_mode_is_valid() {
    let choice = ToolChoiceParam::allowed_tools(vec![SpecificToolChoiceParam::function("echo")]);
    assert!(choice.errors().is_empty());
}

#[test]
fn tool_choice_value_modes_are_valid() {
    let choice = ToolChoiceParam::allowed_tools_with_mode(
        vec![SpecificToolChoiceParam::function("echo")],
        Some(ToolChoiceValue::Auto),
    );
    assert!(choice.errors().is_empty());

    let choice = ToolChoiceParam::allowed_tools_with_mode(
        vec![SpecificToolChoiceParam::function("echo")],
        Some(ToolChoiceValue::None),
    );
    assert!(choice.errors().is_empty());
}

#[test]
fn specific_tool_choice_roundtrip() {
    let specific = SpecificToolChoiceParam::function("echo");
    assert!(specific.errors().is_empty());
    assert_eq!(
        specific.value(),
        &json!({
            "type": "function",
            "name": "echo"
        })
    );
    let value = specific.clone().into_value();
    assert_eq!(
        value,
        json!({
            "type": "function",
            "name": "echo"
        })
    );
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
