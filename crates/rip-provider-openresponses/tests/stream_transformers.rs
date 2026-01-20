use rip_kernel::{Event, EventKind, ProviderEventStatus};
use rip_provider_openresponses::{
    extract_reasoning_deltas, extract_text_deltas, extract_tool_call_argument_deltas,
    EventFrameMapper, SseDecoder,
};
use serde_json::json;

fn load_events() -> Vec<rip_kernel::Event> {
    let sse = include_str!("../fixtures/openresponses/stream_all.sse");
    let mut decoder = SseDecoder::new();
    let mut parsed = decoder.push(sse);
    parsed.extend(decoder.finish());

    let mut mapper = EventFrameMapper::new("session-1");
    parsed.iter().flat_map(|event| mapper.map(event)).collect()
}

#[test]
fn extracts_text_deltas() {
    let events = load_events();
    let deltas = extract_text_deltas(&events);
    assert_eq!(deltas, vec!["".to_string()]);
}

#[test]
fn extracts_reasoning_deltas() {
    let events = load_events();
    let deltas = extract_reasoning_deltas(&events);
    assert_eq!(deltas, vec!["".to_string()]);
}

#[test]
fn extracts_tool_call_argument_deltas() {
    let events = load_events();
    let deltas = extract_tool_call_argument_deltas(&events);
    assert_eq!(deltas, vec!["".to_string()]);
}

#[test]
fn extract_text_deltas_prefers_type_field() {
    let event = Event {
        id: "e1".to_string(),
        session_id: "s1".to_string(),
        timestamp_ms: 0,
        seq: 0,
        kind: EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Event,
            event_name: Some("response.output_text.delta".to_string()),
            data: Some(json!({
                "type": "response.output_text.delta",
                "delta": "hi"
            })),
            raw: None,
            errors: Vec::new(),
            response_errors: Vec::new(),
        },
    };

    let deltas = extract_text_deltas(&[event]);
    assert_eq!(deltas, vec!["hi".to_string()]);
}

#[test]
fn extract_text_deltas_skips_non_object_data() {
    let event = Event {
        id: "e1".to_string(),
        session_id: "s1".to_string(),
        timestamp_ms: 0,
        seq: 0,
        kind: EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Event,
            event_name: Some("response.output_text.delta".to_string()),
            data: Some(json!("not-object")),
            raw: None,
            errors: Vec::new(),
            response_errors: Vec::new(),
        },
    };

    let deltas = extract_text_deltas(&[event]);
    assert!(deltas.is_empty());
}
