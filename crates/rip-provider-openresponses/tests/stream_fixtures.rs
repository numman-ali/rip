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
        frames.extend(mapper.map(event));
    }

    let expected_output_text = parsed
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                rip_provider_openresponses::ParsedEventKind::Event
            ) && event
                .data
                .as_ref()
                .and_then(|value| value.get("type"))
                .and_then(|value| value.as_str())
                == Some("response.output_text.delta")
        })
        .count();
    let expected = allowed_stream_event_types().len() + 1 + expected_output_text;
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
            EventKind::OutputTextDelta { .. } => {}
            _ => panic!("unexpected frame type"),
        }
    }

    let provider_frames = frames
        .iter()
        .filter(|frame| matches!(frame.kind, EventKind::ProviderEvent { .. }))
        .count();
    assert_eq!(provider_frames, parsed.len());

    let output_text_frames = frames
        .iter()
        .filter(|frame| matches!(frame.kind, EventKind::OutputTextDelta { .. }))
        .count();
    assert_eq!(output_text_frames, expected_output_text);
}
