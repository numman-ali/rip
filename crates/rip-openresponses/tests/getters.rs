use rip_openresponses::{
    allowed_stream_event_types, create_response_body_schema, item_param_schema, openapi,
    response_resource_schema, streaming_event_schema, tool_choice_param_schema, tool_param_schema,
};
use std::collections::BTreeSet;

#[test]
fn schema_getters_return_values() {
    assert!(openapi().get("paths").is_some());
    assert!(!allowed_stream_event_types().is_empty());
    assert!(streaming_event_schema().is_object());
    assert!(response_resource_schema().is_object());
    assert!(create_response_body_schema().is_object());
    assert!(tool_param_schema().is_object());
    assert!(tool_choice_param_schema().is_object());
    assert!(item_param_schema().is_object());
}

#[test]
fn allowed_stream_event_types_match_generated_type_map() {
    let allowed = allowed_stream_event_types()
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let type_map = serde_json::from_str::<Vec<(String, String)>>(include_str!(
        "../../../schemas/openresponses/streaming_event_type_map.json"
    ))
    .expect("streaming_event_type_map.json valid");
    let mapped = type_map
        .into_iter()
        .map(|(_, event_type)| event_type)
        .collect::<BTreeSet<_>>();

    assert_eq!(allowed, mapped);
}
