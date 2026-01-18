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
