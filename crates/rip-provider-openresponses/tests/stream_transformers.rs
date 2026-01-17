use rip_provider_openresponses::{
    extract_reasoning_deltas, extract_text_deltas, extract_tool_call_argument_deltas,
    EventFrameMapper, SseDecoder,
};

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
