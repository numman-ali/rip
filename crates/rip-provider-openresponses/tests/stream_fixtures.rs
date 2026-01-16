use rip_kernel::{EventKind, ProviderEventStatus};
use rip_openresponses::allowed_stream_event_types;
use rip_provider_openresponses::{EventFrameMapper, SseDecoder};

#[test]
fn stream_fixture_maps_all_events() {
    let sse = include_str!("../fixtures/openresponses/stream_all.sse");
    let mut decoder = SseDecoder::new();
    let mut parsed = decoder.push(sse);
    parsed.extend(decoder.finish());

    let mut mapper = EventFrameMapper::new("session-1");
    let mut frames = Vec::new();
    for event in &parsed {
        frames.push(mapper.map(event).expect("frame"));
    }

    assert_eq!(frames.len(), parsed.len());
    let expected = allowed_stream_event_types().len() + 1;
    assert_eq!(frames.len(), expected);

    for (idx, frame) in frames.iter().enumerate() {
        assert_eq!(frame.seq, idx as u64);
        match &frame.kind {
            EventKind::ProviderEvent {
                status,
                event_name,
                data,
                ..
            } => match status {
                ProviderEventStatus::Event => {
                    let data = data.as_ref().expect("data");
                    let event_type = data.get("type").and_then(|v| v.as_str());
                    assert_eq!(event_name.as_deref(), event_type);
                }
                ProviderEventStatus::Done => {
                    assert!(event_name.is_none());
                    assert!(data.is_none());
                }
                ProviderEventStatus::InvalidJson => {
                    assert!(data.is_none());
                }
            },
            _ => panic!("expected provider_event"),
        }
    }
}
