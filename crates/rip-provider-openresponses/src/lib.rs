use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use rip_kernel::{Event, EventKind, ProviderEventStatus};

mod request;
mod stream_transformers;
pub use request::{
    CreateResponseBuilder, CreateResponsePayload, ItemParam, ToolChoiceParam, ToolParam,
};
use rip_openresponses::{validate_response_resource, validate_stream_event};
pub use stream_transformers::{
    extract_reasoning_deltas, extract_text_deltas, extract_tool_call_argument_deltas,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedEventKind {
    Done,
    InvalidJson,
    Event,
}

#[derive(Debug, Clone)]
pub struct ParsedEvent {
    pub kind: ParsedEventKind,
    pub event: Option<String>,
    pub raw: String,
    pub data: Option<Value>,
    pub errors: Vec<String>,
    pub response_errors: Vec<String>,
}

impl ParsedEvent {
    fn done(raw: String) -> Self {
        Self {
            kind: ParsedEventKind::Done,
            event: None,
            raw,
            data: None,
            errors: Vec::new(),
            response_errors: Vec::new(),
        }
    }

    fn invalid_json(raw: String, err: String, event: Option<String>) -> Self {
        Self {
            kind: ParsedEventKind::InvalidJson,
            event,
            raw,
            data: None,
            errors: vec![err],
            response_errors: Vec::new(),
        }
    }

    fn event(raw: String, event: Option<String>, data: Value) -> Self {
        let mut errors = Vec::new();
        if let Err(errs) = validate_stream_event(&data) {
            errors.extend(errs);
        }

        if let Some(event_name) = event.as_ref() {
            if let Some(type_name) = data.get("type").and_then(|v| v.as_str()) {
                if event_name != type_name {
                    errors.push(format!(
                        "event name '{event_name}' does not match type '{type_name}'"
                    ));
                }
            }
        }

        let mut response_errors = Vec::new();
        if let Some(response) = data.get("response") {
            if let Err(errs) = validate_response_resource(response) {
                response_errors.extend(errs);
            }
        }

        Self {
            kind: ParsedEventKind::Event,
            event,
            raw,
            data: Some(data),
            errors,
            response_errors,
        }
    }
}

#[derive(Debug)]
pub struct EventFrameMapper {
    session_id: String,
    seq: u64,
}

impl EventFrameMapper {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            seq: 0,
        }
    }

    pub fn map(&mut self, parsed: &ParsedEvent) -> Option<Event> {
        let (status, data, raw) = match parsed.kind {
            ParsedEventKind::Done => (ProviderEventStatus::Done, None, Some(parsed.raw.clone())),
            ParsedEventKind::InvalidJson => (
                ProviderEventStatus::InvalidJson,
                None,
                Some(parsed.raw.clone()),
            ),
            ParsedEventKind::Event => (ProviderEventStatus::Event, parsed.data.clone(), None),
        };

        Some(self.emit(EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status,
            event_name: parsed.event.clone(),
            data,
            raw,
            errors: parsed.errors.clone(),
            response_errors: parsed.response_errors.clone(),
        }))
    }

    fn emit(&mut self, kind: EventKind) -> Event {
        let event = Event {
            id: Uuid::new_v4().to_string(),
            session_id: self.session_id.clone(),
            timestamp_ms: now_ms(),
            seq: self.seq,
            kind,
        };
        self.seq += 1;
        event
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: String,
    current_event: Option<String>,
    current_data: Vec<String>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, chunk: &str) -> Vec<ParsedEvent> {
        self.buffer.push_str(chunk);
        let mut events = Vec::new();
        let mut lines = self.buffer.split('\n').peekable();
        let mut pending_tail = None;

        while let Some(line) = lines.next() {
            let is_last = lines.peek().is_none();
            if is_last && !self.buffer.ends_with('\n') {
                pending_tail = Some(line.to_string());
                break;
            }

            let line = line.trim_end_matches('\r');
            if let Some(rest) = line.strip_prefix("event:") {
                let value = rest.trim();
                self.current_event = if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                };
            } else if let Some(rest) = line.strip_prefix("data:") {
                let value = rest.trim_start();
                self.current_data.push(value.to_string());
            } else if line.is_empty() {
                if is_last {
                    pending_tail = Some(String::new());
                    break;
                }
                if !self.current_data.is_empty() {
                    let data = self.current_data.join("\n");
                    let raw = data.clone();
                    events.push(self.parse_event(raw));
                    self.current_data.clear();
                    self.current_event = None;
                }
            } else if line.starts_with(':') {
                continue;
            }
        }

        self.buffer = pending_tail.unwrap_or_default();
        events
    }

    pub fn finish(&mut self) -> Vec<ParsedEvent> {
        if self.buffer.is_empty() {
            return Vec::new();
        }
        let chunk = format!("{}\n", self.buffer);
        self.buffer.clear();
        self.push(&chunk)
    }

    fn parse_event(&self, raw: String) -> ParsedEvent {
        if raw == "[DONE]" {
            return ParsedEvent::done(raw);
        }

        match serde_json::from_str::<Value>(&raw) {
            Ok(value) => ParsedEvent::event(raw, self.current_event.clone(), value),
            Err(err) => ParsedEvent::invalid_json(raw, err.to_string(), self.current_event.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_done_sentinel() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push("data: [DONE]\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, ParsedEventKind::Done);
    }

    #[test]
    fn parses_invalid_json() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push("data: {not json}\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, ParsedEventKind::InvalidJson);
    }

    #[test]
    fn captures_event_name_mismatch() {
        let mut decoder = SseDecoder::new();
        let payload = "event: response.created\n\
                      data: {\"type\":\"response.completed\",\"sequence_number\":1,\"response\":{}}\n\n";
        let events = decoder.push(payload);
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event.kind, ParsedEventKind::Event);
        assert!(event
            .errors
            .iter()
            .any(|e| e.contains("does not match type")));
    }

    #[test]
    fn handles_split_chunks() {
        let mut decoder = SseDecoder::new();
        let part1 = "data: {\"type\":\"response.created\",\"sequence_number\":1,\n";
        let part2 = "data: \"response\":{}}\n\n";
        let mut events = decoder.push(part1);
        assert!(events.is_empty());
        events.extend(decoder.push(part2));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, ParsedEventKind::Event);
    }

    #[test]
    fn ignores_comment_lines() {
        let mut decoder = SseDecoder::new();
        let payload = ": keep-alive\n\
                       data: {\"type\":\"response.created\",\"sequence_number\":1,\"response\":{}}\n\n";
        let events = decoder.push(payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, ParsedEventKind::Event);
    }

    #[test]
    fn empty_event_name_sets_none() {
        let mut decoder = SseDecoder::new();
        let payload = "event:\n\
                      data: {\"type\":\"response.created\",\"sequence_number\":1,\"response\":{}}\n\n";
        let events = decoder.push(payload);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, None);
    }

    #[test]
    fn maps_output_text_delta_to_provider_frame() {
        let parsed = ParsedEvent {
            kind: ParsedEventKind::Event,
            event: Some("response.output_text.delta".to_string()),
            raw: "{\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}".to_string(),
            data: Some(serde_json::json!({
                "type": "response.output_text.delta",
                "delta": "hi"
            })),
            errors: Vec::new(),
            response_errors: Vec::new(),
        };

        let mut mapper = EventFrameMapper::new("session-1");
        let frame = mapper.map(&parsed).expect("frame");
        assert_eq!(frame.session_id, "session-1");
        assert_eq!(frame.seq, 0);
        match frame.kind {
            EventKind::ProviderEvent {
                provider,
                status,
                event_name,
                data,
                raw,
                ..
            } => {
                assert_eq!(provider, "openresponses");
                assert_eq!(status, ProviderEventStatus::Event);
                assert_eq!(event_name.as_deref(), Some("response.output_text.delta"));
                let data = data.expect("data");
                assert_eq!(data.get("delta").and_then(|v| v.as_str()), Some("hi"));
                assert!(raw.is_none());
            }
            _ => panic!("expected provider_event"),
        }
    }

    #[test]
    fn maps_completed_to_provider_frame() {
        let parsed = ParsedEvent {
            kind: ParsedEventKind::Event,
            event: Some("response.completed".to_string()),
            raw: "{\"type\":\"response.completed\"}".to_string(),
            data: Some(serde_json::json!({
                "type": "response.completed"
            })),
            errors: Vec::new(),
            response_errors: Vec::new(),
        };

        let mut mapper = EventFrameMapper::new("session-1");
        let frame = mapper.map(&parsed).expect("frame");
        match frame.kind {
            EventKind::ProviderEvent {
                status,
                event_name,
                data,
                ..
            } => {
                assert_eq!(status, ProviderEventStatus::Event);
                assert_eq!(event_name.as_deref(), Some("response.completed"));
                let data = data.expect("data");
                assert_eq!(
                    data.get("type").and_then(|v| v.as_str()),
                    Some("response.completed")
                );
            }
            _ => panic!("expected provider_event"),
        }
    }

    #[test]
    fn done_sentinel_maps_to_provider_frame() {
        let done = ParsedEvent {
            kind: ParsedEventKind::Done,
            event: None,
            raw: "[DONE]".to_string(),
            data: None,
            errors: Vec::new(),
            response_errors: Vec::new(),
        };

        let mut mapper = EventFrameMapper::new("session-1");
        let frame = mapper.map(&done).expect("frame");
        match frame.kind {
            EventKind::ProviderEvent {
                status, raw, data, ..
            } => {
                assert_eq!(status, ProviderEventStatus::Done);
                assert_eq!(raw.as_deref(), Some("[DONE]"));
                assert!(data.is_none());
            }
            _ => panic!("expected provider_event"),
        }
    }

    #[test]
    fn invalid_json_maps_to_provider_frame() {
        let invalid = ParsedEvent {
            kind: ParsedEventKind::InvalidJson,
            event: Some("response.created".to_string()),
            raw: "{not json}".to_string(),
            data: None,
            errors: vec!["oops".to_string()],
            response_errors: Vec::new(),
        };

        let mut mapper = EventFrameMapper::new("session-1");
        let frame = mapper.map(&invalid).expect("frame");
        match frame.kind {
            EventKind::ProviderEvent {
                status,
                raw,
                data,
                errors,
                ..
            } => {
                assert_eq!(status, ProviderEventStatus::InvalidJson);
                assert_eq!(raw.as_deref(), Some("{not json}"));
                assert!(data.is_none());
                assert_eq!(errors, vec!["oops".to_string()]);
            }
            _ => panic!("expected provider_event"),
        }
    }

    #[test]
    fn finish_flushes_buffer() {
        let mut decoder = SseDecoder::new();
        let events = decoder
            .push("data: {\"type\":\"response.created\",\"sequence_number\":1,\"response\":{}}");
        assert!(events.is_empty());
        let flushed = decoder.finish();
        assert!(flushed.is_empty());
    }

    #[test]
    fn captures_response_validation_errors() {
        let mut decoder = SseDecoder::new();
        let payload = "event: response.completed\n\
                      data: {\"type\":\"response.completed\",\"sequence_number\":1,\"response\":{}}\n\n";
        let events = decoder.push(payload);
        assert_eq!(events.len(), 1);
        assert!(!events[0].response_errors.is_empty());
    }
}
