use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use rip_kernel::{Event, EventKind, ProviderEventStatus};

mod request;
mod stream_transformers;
pub use request::{
    CreateResponseBuilder, CreateResponsePayload, ItemParam, SpecificToolChoiceParam,
    ToolChoiceParam, ToolChoiceValue, ToolParam,
};
use rip_openresponses::{validate_response_resource, validate_stream_event};
pub use stream_transformers::{
    extract_reasoning_deltas, extract_text_deltas, extract_tool_call_argument_deltas,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct ValidationOptions {
    pub normalize_missing_item_ids: bool,
}

impl ValidationOptions {
    pub fn strict() -> Self {
        Self {
            normalize_missing_item_ids: false,
        }
    }

    pub fn compat_missing_item_ids() -> Self {
        Self {
            normalize_missing_item_ids: true,
        }
    }
}

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

    fn event(
        raw: String,
        event: Option<String>,
        data: Value,
        validation: ValidationOptions,
    ) -> Self {
        let mut errors = Vec::new();
        let validation_data = if validation.normalize_missing_item_ids {
            normalize_event_for_validation(&data)
        } else {
            data.clone()
        };
        if let Err(errs) = validate_stream_event(&validation_data) {
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
        if let Some(response) = validation_data.get("response") {
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

    pub fn map(&mut self, parsed: &ParsedEvent) -> Vec<Event> {
        let provider_frame = self.emit_provider_event(parsed);
        let mut frames = vec![provider_frame];

        if let Some(delta) = output_text_delta(parsed) {
            frames.push(self.emit(EventKind::OutputTextDelta { delta }));
        }

        frames
    }

    fn emit_provider_event(&mut self, parsed: &ParsedEvent) -> Event {
        let (status, data, raw) = match parsed.kind {
            ParsedEventKind::Done => (ProviderEventStatus::Done, None, Some(parsed.raw.clone())),
            ParsedEventKind::InvalidJson => (
                ProviderEventStatus::InvalidJson,
                None,
                Some(parsed.raw.clone()),
            ),
            ParsedEventKind::Event => (ProviderEventStatus::Event, parsed.data.clone(), None),
        };

        self.emit(EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status,
            event_name: parsed.event.clone(),
            data,
            raw,
            errors: parsed.errors.clone(),
            response_errors: parsed.response_errors.clone(),
        })
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

fn output_text_delta(parsed: &ParsedEvent) -> Option<String> {
    let data = parsed.data.as_ref()?;
    let obj = data.as_object()?;
    let event_type = obj.get("type").and_then(|value| value.as_str());
    if event_type != Some("response.output_text.delta") {
        return None;
    }
    obj.get("delta")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

fn normalize_event_for_validation(value: &Value) -> Value {
    let mut normalized = value.clone();
    let Some(obj) = normalized.as_object_mut() else {
        return normalized;
    };

    let output_index = obj.get("output_index").and_then(|value| value.as_u64());
    if let Some(item) = obj.get_mut("item") {
        normalize_output_item(item, output_index);
    }

    if let Some(response) = obj.get_mut("response") {
        normalize_response_resource(response);
    }

    if let Some(event_type) = obj.get("type").and_then(|value| value.as_str()) {
        if matches!(
            event_type,
            "response.function_call_arguments.delta" | "response.function_call_arguments.done"
        ) && obj
            .get("item_id")
            .and_then(|value| value.as_str())
            .map(|value| value.is_empty())
            .unwrap_or(true)
        {
            if let Some(output_index) = obj.get("output_index").and_then(|value| value.as_u64()) {
                obj.insert(
                    "item_id".to_string(),
                    Value::String(format!("item_{output_index}")),
                );
            }
        }
    }

    normalized
}

fn normalize_response_resource(response: &mut Value) {
    let Some(obj) = response.as_object_mut() else {
        return;
    };
    let Some(output) = obj.get_mut("output") else {
        return;
    };
    let Some(items) = output.as_array_mut() else {
        return;
    };
    for (idx, item) in items.iter_mut().enumerate() {
        normalize_output_item(item, Some(idx as u64));
    }
}

fn normalize_output_item(item: &mut Value, output_index: Option<u64>) {
    let Some(obj) = item.as_object_mut() else {
        return;
    };
    if obj
        .get("id")
        .and_then(|value| value.as_str())
        .map(|value| !value.is_empty())
        .unwrap_or(false)
    {
        return;
    }

    let item_type = obj.get("type").and_then(|value| value.as_str());
    match item_type {
        Some("function_call") => {
            if let Some(call_id) = obj
                .get("call_id")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
            {
                obj.insert("id".to_string(), Value::String(call_id.to_string()));
                return;
            }
            if let Some(output_index) = output_index {
                obj.insert(
                    "id".to_string(),
                    Value::String(format!("item_{output_index}")),
                );
            }
        }
        Some("function_call_output") => {
            if let Some(call_id) = obj
                .get("call_id")
                .and_then(|value| value.as_str())
                .filter(|value| !value.is_empty())
            {
                obj.insert("id".to_string(), Value::String(format!("output_{call_id}")));
                return;
            }
            if let Some(output_index) = output_index {
                obj.insert(
                    "id".to_string(),
                    Value::String(format!("output_{output_index}")),
                );
            }
        }
        _ => {}
    }
}

#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: String,
    current_event: Option<String>,
    current_data: Vec<String>,
    validation: ValidationOptions,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::new_with_validation(ValidationOptions::strict())
    }

    pub fn new_with_validation(validation: ValidationOptions) -> Self {
        Self {
            buffer: String::new(),
            current_event: None,
            current_data: Vec::new(),
            validation,
        }
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
            Ok(value) => {
                ParsedEvent::event(raw, self.current_event.clone(), value, self.validation)
            }
            Err(err) => ParsedEvent::invalid_json(raw, err.to_string(), self.current_event.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_missing_item_ids_for_validation() {
        let payload = "event: response.output_item.added\n\
data: {\"type\":\"response.output_item.added\",\"sequence_number\":1,\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"ls\",\"arguments\":\"{}\",\"status\":\"in_progress\"}}\n\n";

        let mut strict = SseDecoder::new();
        let events = strict.push(payload);
        assert_eq!(events.len(), 1);
        assert!(events[0].errors.iter().any(|err| err.contains("id")));

        let mut compat =
            SseDecoder::new_with_validation(ValidationOptions::compat_missing_item_ids());
        let events = compat.push(payload);
        assert_eq!(events.len(), 1);
        assert!(events[0].errors.is_empty());
    }

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
    fn invalid_json_constructor_sets_fields() {
        let parsed = ParsedEvent::invalid_json(
            "raw".to_string(),
            "boom".to_string(),
            Some("response.created".to_string()),
        );
        assert_eq!(parsed.kind, ParsedEventKind::InvalidJson);
        assert_eq!(parsed.event.as_deref(), Some("response.created"));
        assert_eq!(parsed.raw, "raw");
        assert_eq!(parsed.errors, vec!["boom".to_string()]);
        assert!(parsed.data.is_none());
        assert!(parsed.response_errors.is_empty());
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
        let frames = mapper.map(&parsed);
        assert_eq!(frames.len(), 2);

        let frame = &frames[0];
        assert_eq!(frame.session_id, "session-1");
        assert_eq!(frame.seq, 0);
        match &frame.kind {
            EventKind::ProviderEvent {
                provider,
                status,
                event_name,
                data,
                raw,
                ..
            } => {
                assert_eq!(provider, "openresponses");
                assert_eq!(*status, ProviderEventStatus::Event);
                assert_eq!(event_name.as_deref(), Some("response.output_text.delta"));
                let data = data.as_ref().expect("data");
                assert_eq!(data.get("delta").and_then(|v| v.as_str()), Some("hi"));
                assert!(raw.is_none());
            }
            _ => panic!("expected provider_event"),
        }

        match &frames[1].kind {
            EventKind::OutputTextDelta { delta } => assert_eq!(delta, "hi"),
            _ => panic!("expected output_text_delta"),
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
        let frames = mapper.map(&parsed);
        assert_eq!(frames.len(), 1);
        match &frames[0].kind {
            EventKind::ProviderEvent {
                status,
                event_name,
                data,
                ..
            } => {
                assert_eq!(*status, ProviderEventStatus::Event);
                assert_eq!(event_name.as_deref(), Some("response.completed"));
                let data = data.as_ref().expect("data");
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
        let frames = mapper.map(&done);
        assert_eq!(frames.len(), 1);
        match &frames[0].kind {
            EventKind::ProviderEvent {
                status, raw, data, ..
            } => {
                assert_eq!(*status, ProviderEventStatus::Done);
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
        let frames = mapper.map(&invalid);
        assert_eq!(frames.len(), 1);
        match &frames[0].kind {
            EventKind::ProviderEvent {
                status,
                raw,
                data,
                errors,
                ..
            } => {
                assert_eq!(*status, ProviderEventStatus::InvalidJson);
                assert_eq!(raw.as_deref(), Some("{not json}"));
                assert!(data.is_none());
                assert_eq!(errors, &vec!["oops".to_string()]);
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
        let errors = &events[0].response_errors;
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|err| err.contains("truncation")));
        assert!(errors
            .iter()
            .any(|err| err.contains("previous_response_id")));
    }

    #[test]
    fn output_text_delta_filters_non_text_events() {
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
        assert!(output_text_delta(&parsed).is_none());
    }

    #[test]
    fn normalize_event_inserts_item_id_for_function_call_arguments() {
        let value = serde_json::json!({
            "type": "response.function_call_arguments.delta",
            "output_index": 2,
            "delta": "{}"
        });
        let normalized = normalize_event_for_validation(&value);
        assert_eq!(
            normalized.get("item_id").and_then(|v| v.as_str()),
            Some("item_2")
        );
    }

    #[test]
    fn normalize_event_preserves_existing_item_id() {
        let value = serde_json::json!({
            "type": "response.function_call_arguments.done",
            "output_index": 1,
            "item_id": "item_custom"
        });
        let normalized = normalize_event_for_validation(&value);
        assert_eq!(
            normalized.get("item_id").and_then(|v| v.as_str()),
            Some("item_custom")
        );
    }

    #[test]
    fn normalize_event_for_validation_passthrough_non_object() {
        let value = serde_json::json!("raw");
        let normalized = normalize_event_for_validation(&value);
        assert_eq!(normalized, value);
    }

    #[test]
    fn normalize_output_item_prefers_call_id() {
        let mut value = serde_json::json!({
            "type": "function_call",
            "call_id": "call_9"
        });
        normalize_output_item(&mut value, Some(3));
        assert_eq!(value.get("id").and_then(|v| v.as_str()), Some("call_9"));
    }

    #[test]
    fn normalize_output_item_sets_item_id_when_missing() {
        let mut value = serde_json::json!({
            "type": "function_call",
            "call_id": ""
        });
        normalize_output_item(&mut value, Some(2));
        assert_eq!(value.get("id").and_then(|v| v.as_str()), Some("item_2"));
    }

    #[test]
    fn normalize_output_item_sets_output_id_when_missing() {
        let mut value = serde_json::json!({
            "type": "function_call_output",
            "call_id": ""
        });
        normalize_output_item(&mut value, Some(0));
        assert_eq!(value.get("id").and_then(|v| v.as_str()), Some("output_0"));
    }

    #[test]
    fn normalize_response_resource_sets_missing_ids() {
        let mut value = serde_json::json!({
            "output": [
                {"type": "function_call", "call_id": "call_a"},
                {"type": "function_call_output", "call_id": "call_b"}
            ]
        });
        normalize_response_resource(&mut value);
        let output = value
            .get("output")
            .and_then(|v| v.as_array())
            .expect("output");
        assert_eq!(output[0].get("id").and_then(|v| v.as_str()), Some("call_a"));
        assert_eq!(
            output[1].get("id").and_then(|v| v.as_str()),
            Some("output_call_b")
        );
    }
}
