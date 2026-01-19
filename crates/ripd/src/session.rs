use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use futures_util::StreamExt;
use rip_kernel::{Event, EventKind, Runtime};
use rip_log::{write_snapshot, EventLog};
use rip_provider_openresponses::{
    CreateResponsePayload, EventFrameMapper, ItemParam, ParsedEvent, ParsedEventKind, SseDecoder,
    ValidationOptions,
};
use rip_tools::{ToolInvocation, ToolRunner};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::provider_openresponses::{
    build_streaming_followup_request, build_streaming_request, build_streaming_request_items,
    OpenResponsesConfig, DEFAULT_MAX_TOOL_CALLS,
};

#[derive(Deserialize)]
struct ToolCommand {
    tool: String,
    #[serde(default)]
    args: Value,
    timeout_ms: Option<u64>,
}

#[derive(Deserialize)]
struct CheckpointEnvelope {
    checkpoint: CheckpointCommand,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum CheckpointCommand {
    Create { label: String, files: Vec<String> },
    Rewind { id: String },
}

enum InputAction {
    Tool(ToolCommand),
    Checkpoint(CheckpointCommand),
    Prompt,
}

pub struct SessionContext {
    pub runtime: Arc<Runtime>,
    pub tool_runner: Arc<ToolRunner>,
    pub http_client: reqwest::Client,
    pub openresponses: Option<OpenResponsesConfig>,
    pub sender: broadcast::Sender<Event>,
    pub events: Arc<Mutex<Vec<Event>>>,
    pub event_log: Arc<EventLog>,
    pub snapshot_dir: Arc<PathBuf>,
    pub server_session_id: String,
    pub input: String,
}

pub async fn run_session(context: SessionContext) {
    let SessionContext {
        runtime,
        tool_runner,
        http_client,
        openresponses,
        sender,
        events,
        event_log,
        snapshot_dir,
        server_session_id,
        input,
    } = context;
    let mut session = runtime.start_session_with_id(server_session_id.clone(), input.clone());
    let action = parse_action(&input);
    let runtime_session_id = session.id().to_string();
    let mut skip_runtime_loop = false;

    if let Some(event) = session.next_event() {
        emit_event(event, &sender, &events, &event_log).await;
    }

    match action {
        InputAction::Tool(command) => {
            let mut seq = session.seq();
            let tool_events = tool_runner
                .run(
                    &runtime_session_id,
                    &mut seq,
                    ToolInvocation {
                        name: command.tool,
                        args: command.args,
                        timeout_ms: command.timeout_ms,
                    },
                )
                .await;
            session.set_seq(seq);
            emit_events(tool_events, &sender, &events, &event_log).await;
        }
        InputAction::Checkpoint(command) => {
            let mut seq = session.seq();
            let checkpoint_events = match command {
                CheckpointCommand::Create { label, files } => tool_runner.create_checkpoint(
                    &runtime_session_id,
                    &mut seq,
                    label,
                    files.into_iter().map(PathBuf::from).collect(),
                ),
                CheckpointCommand::Rewind { id } => {
                    tool_runner.rewind_checkpoint(&runtime_session_id, &mut seq, &id)
                }
            };
            session.set_seq(seq);
            emit_events(checkpoint_events, &sender, &events, &event_log).await;
        }
        InputAction::Prompt => {
            if let Some(config) = &openresponses {
                let mut seq = session.seq();
                let sink = EventSink {
                    sender: &sender,
                    buffer: &events,
                    event_log: event_log.as_ref(),
                };
                let reason = run_openresponses_agent_loop(
                    &http_client,
                    config,
                    tool_runner.as_ref(),
                    &runtime_session_id,
                    &input,
                    &mut seq,
                    sink,
                )
                .await;
                emit_event(
                    Event {
                        id: Uuid::new_v4().to_string(),
                        session_id: runtime_session_id.clone(),
                        timestamp_ms: now_ms(),
                        seq,
                        kind: rip_kernel::EventKind::SessionEnded { reason },
                    },
                    &sender,
                    &events,
                    &event_log,
                )
                .await;
                skip_runtime_loop = true;
            }
        }
    }

    if !skip_runtime_loop {
        while let Some(event) = session.next_event() {
            emit_event(event, &sender, &events, &event_log).await;
        }
    }

    let guard = events.lock().await;
    let _ = write_snapshot(&*snapshot_dir, &server_session_id, &guard);
}

#[derive(Clone, Copy)]
struct EventSink<'a> {
    sender: &'a broadcast::Sender<Event>,
    buffer: &'a Arc<Mutex<Vec<Event>>>,
    event_log: &'a EventLog,
}

impl EventSink<'_> {
    async fn emit(self, event: Event) {
        emit_event(event, self.sender, self.buffer, self.event_log).await;
    }

    async fn emit_all(self, events: Vec<Event>) {
        emit_events(events, self.sender, self.buffer, self.event_log).await;
    }
}

struct OpenResponsesSsePipe<'a> {
    session_id: String,
    decoder: SseDecoder,
    mapper: EventFrameMapper,
    seq_offset: u64,
    seq: &'a mut u64,
    sink: EventSink<'a>,
    collector: Option<&'a mut ToolCallCollector>,
}

impl<'a> OpenResponsesSsePipe<'a> {
    fn new(
        session_id: &str,
        seq: &'a mut u64,
        sink: EventSink<'a>,
        collector: Option<&'a mut ToolCallCollector>,
        validation: ValidationOptions,
    ) -> Self {
        Self {
            session_id: session_id.to_string(),
            decoder: SseDecoder::new_with_validation(validation),
            mapper: EventFrameMapper::new(session_id.to_string()),
            seq_offset: *seq,
            seq,
            sink,
            collector,
        }
    }

    async fn emit_transport_error(&mut self, error: String) {
        self.sink
            .emit(Event {
                id: Uuid::new_v4().to_string(),
                session_id: self.session_id.clone(),
                timestamp_ms: now_ms(),
                seq: *self.seq,
                kind: rip_kernel::EventKind::ProviderEvent {
                    provider: "openresponses".to_string(),
                    status: rip_kernel::ProviderEventStatus::InvalidJson,
                    event_name: None,
                    data: None,
                    raw: Some(error.clone()),
                    errors: vec![error],
                    response_errors: Vec::new(),
                },
            })
            .await;
        *self.seq += 1;
    }

    async fn push_sse_str(&mut self, chunk: &str) -> bool {
        let parsed = self.decoder.push(chunk);
        if parsed.is_empty() {
            return false;
        }

        let mut frames = Vec::new();
        for event in &parsed {
            if let Some(collector) = self.collector.as_deref_mut() {
                collector.observe(event);
            }
            frames.extend(self.mapper.map(event));
        }
        for frame in &mut frames {
            frame.seq += self.seq_offset;
        }
        let frame_count = frames.len();

        self.sink.emit_all(frames).await;
        *self.seq += frame_count as u64;

        parsed
            .iter()
            .any(|event| event.kind == ParsedEventKind::Done)
    }

    async fn push_bytes(&mut self, utf8_buf: &mut Vec<u8>, bytes: &[u8]) -> bool {
        utf8_buf.extend_from_slice(bytes);
        let mut saw_done = false;

        loop {
            match std::str::from_utf8(utf8_buf) {
                Ok(text) => {
                    saw_done = self.push_sse_str(text).await;
                    utf8_buf.clear();
                    break;
                }
                Err(err) => {
                    let valid = err.valid_up_to();
                    if valid == 0 {
                        if err.error_len().is_none() {
                            break;
                        }
                        utf8_buf.remove(0);
                        saw_done = self.push_sse_str("\u{FFFD}").await;
                        if saw_done {
                            utf8_buf.clear();
                            break;
                        }
                        continue;
                    }

                    let valid_text =
                        std::str::from_utf8(&utf8_buf[..valid]).expect("valid utf8 prefix");
                    saw_done = self.push_sse_str(valid_text).await;
                    utf8_buf.drain(..valid);

                    if saw_done {
                        utf8_buf.clear();
                        break;
                    }

                    if err.error_len().is_none() {
                        break;
                    }

                    let invalid_len = err.error_len().unwrap_or(1);
                    let drain_len = invalid_len.min(utf8_buf.len());
                    utf8_buf.drain(..drain_len);
                    saw_done = self.push_sse_str("\u{FFFD}").await;
                    if saw_done {
                        utf8_buf.clear();
                        break;
                    }
                }
            }
        }

        saw_done
    }

    async fn finish(&mut self) -> bool {
        let parsed = self.decoder.finish();
        if parsed.is_empty() {
            return false;
        }

        let mut frames = Vec::new();
        for event in &parsed {
            if let Some(collector) = self.collector.as_deref_mut() {
                collector.observe(event);
            }
            frames.extend(self.mapper.map(event));
        }
        for frame in &mut frames {
            frame.seq += self.seq_offset;
        }
        let frame_count = frames.len();
        let saw_done = parsed
            .iter()
            .any(|event| event.kind == ParsedEventKind::Done);

        self.sink.emit_all(frames).await;
        *self.seq += frame_count as u64;

        saw_done
    }
}

#[derive(Debug, Clone)]
struct FunctionCallItem {
    output_index: u64,
    call_id: String,
    item_id: Option<String>,
    name: String,
    arguments: String,
}

#[derive(Debug, Clone, Default)]
struct FunctionCallBuffer {
    output_index: u64,
    call_id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Default)]
struct ToolCallCollector {
    response_id: Option<String>,
    function_call_by_item_id: HashMap<String, FunctionCallBuffer>,
    item_id_by_call_id: HashMap<String, String>,
    completed_function_calls: Vec<FunctionCallItem>,
}

impl ToolCallCollector {
    fn observe(&mut self, parsed: &ParsedEvent) {
        if parsed.kind != ParsedEventKind::Event {
            return;
        }
        let Some(data) = parsed.data.as_ref() else {
            return;
        };
        let Some(obj) = data.as_object() else {
            return;
        };

        if let Some(id) = obj
            .get("response")
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_str())
        {
            if !id.is_empty() {
                self.response_id = Some(id.to_string());
            }
        }

        let Some(event_type) = obj.get("type").and_then(|value| value.as_str()) else {
            return;
        };

        match event_type {
            "response.output_item.added" | "response.output_item.done" => {
                let output_index = obj
                    .get("output_index")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                let Some(item) = obj.get("item").and_then(|value| value.as_object()) else {
                    return;
                };
                if item.get("type").and_then(|value| value.as_str()) != Some("function_call") {
                    return;
                }

                let call_id = item
                    .get("call_id")
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string());
                let item_id = item
                    .get("id")
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_string())
                    .or_else(|| {
                        call_id
                            .as_deref()
                            .and_then(|call_id| self.item_id_by_call_id.get(call_id).cloned())
                    })
                    .or_else(|| call_id.clone())
                    .unwrap_or_default();
                if item_id.is_empty() {
                    return;
                }
                if let Some(call_id) = call_id.as_deref() {
                    self.item_id_by_call_id
                        .entry(call_id.to_string())
                        .or_insert_with(|| item_id.clone());
                }

                let entry = self
                    .function_call_by_item_id
                    .entry(item_id.clone())
                    .or_default();
                entry.output_index = output_index;
                entry.call_id = call_id.clone().or(entry.call_id.take());
                entry.name = item
                    .get("name")
                    .and_then(|value| value.as_str())
                    .map(|value| value.to_string())
                    .or(entry.name.take());
                if let Some(arguments) = item.get("arguments").and_then(|value| value.as_str()) {
                    if !arguments.is_empty() {
                        entry.arguments = arguments.to_string();
                    }
                }

                if event_type == "response.output_item.done" {
                    let buffer = self.function_call_by_item_id.remove(&item_id);
                    let call_id = item
                        .get("call_id")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())
                        .or_else(|| buffer.as_ref().and_then(|buffer| buffer.call_id.clone()));
                    let name = item
                        .get("name")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string())
                        .or_else(|| buffer.as_ref().and_then(|buffer| buffer.name.clone()));
                    let arguments = item
                        .get("arguments")
                        .and_then(|value| value.as_str())
                        .filter(|value| !value.is_empty())
                        .map(|value| value.to_string())
                        .or_else(|| buffer.as_ref().map(|buffer| buffer.arguments.clone()))
                        .unwrap_or_default();

                    if let (Some(call_id), Some(name)) = (call_id, name) {
                        self.completed_function_calls.push(FunctionCallItem {
                            output_index,
                            call_id,
                            item_id: Some(item_id.clone()),
                            name,
                            arguments,
                        });
                    }
                }
            }
            "response.function_call_arguments.delta" => {
                let Some(item_id) = obj.get("item_id").and_then(|value| value.as_str()) else {
                    return;
                };
                let output_index = obj
                    .get("output_index")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                let delta = obj
                    .get("delta")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");

                let entry = self
                    .function_call_by_item_id
                    .entry(item_id.to_string())
                    .or_default();
                entry.output_index = output_index;
                entry.arguments.push_str(delta);
            }
            "response.function_call_arguments.done" => {
                let Some(item_id) = obj.get("item_id").and_then(|value| value.as_str()) else {
                    return;
                };
                let output_index = obj
                    .get("output_index")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0);
                let arguments = obj
                    .get("arguments")
                    .and_then(|value| value.as_str())
                    .unwrap_or("");

                let entry = self
                    .function_call_by_item_id
                    .entry(item_id.to_string())
                    .or_default();
                entry.output_index = output_index;
                entry.arguments = arguments.to_string();
            }
            _ => {}
        }
    }

    fn drain_function_calls(&mut self) -> Vec<FunctionCallItem> {
        let mut calls = std::mem::take(&mut self.completed_function_calls);
        calls.sort_by_key(|call| call.output_index);
        calls
    }
}

fn tool_events_to_function_call_output(tool_name: &str, events: &[Event]) -> Value {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut exit_code: i32 = 1;
    let mut artifacts: Option<Value> = None;
    let mut tool_error: Option<String> = None;

    for event in events {
        match &event.kind {
            EventKind::ToolStdout { chunk, .. } => stdout.push_str(chunk),
            EventKind::ToolStderr { chunk, .. } => stderr.push_str(chunk),
            EventKind::ToolEnded {
                exit_code: code,
                artifacts: tool_artifacts,
                ..
            } => {
                exit_code = *code;
                artifacts = tool_artifacts.clone();
            }
            EventKind::ToolFailed { error, .. } => tool_error = Some(error.clone()),
            _ => {}
        }
    }

    let ok = exit_code == 0 && tool_error.is_none();
    let mut obj = serde_json::Map::new();
    obj.insert("tool".to_string(), Value::String(tool_name.to_string()));
    obj.insert("ok".to_string(), Value::Bool(ok));
    obj.insert(
        "exit_code".to_string(),
        Value::Number(serde_json::Number::from(exit_code as i64)),
    );
    obj.insert("stdout".to_string(), Value::String(stdout));
    obj.insert("stderr".to_string(), Value::String(stderr));
    if let Some(artifacts) = artifacts {
        obj.insert("artifacts".to_string(), artifacts);
    }
    if let Some(error) = tool_error {
        obj.insert("error".to_string(), Value::String(error));
    }
    Value::Object(obj)
}

fn function_call_item_from_call(call: &FunctionCallItem, include_id: bool) -> ItemParam {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "type".to_string(),
        Value::String("function_call".to_string()),
    );
    if include_id {
        let id = call
            .item_id
            .clone()
            .unwrap_or_else(|| format!("fc_{}", call.call_id));
        obj.insert("id".to_string(), Value::String(id));
    }
    obj.insert("call_id".to_string(), Value::String(call.call_id.clone()));
    obj.insert("name".to_string(), Value::String(call.name.clone()));
    obj.insert(
        "arguments".to_string(),
        Value::String(call.arguments.clone()),
    );
    ItemParam::new(Value::Object(obj))
}

fn function_call_output_item(call_id: &str, output_json: String, include_id: bool) -> ItemParam {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "type".to_string(),
        Value::String("function_call_output".to_string()),
    );
    if include_id {
        obj.insert("id".to_string(), Value::String(format!("output_{call_id}")));
    }
    obj.insert("call_id".to_string(), Value::String(call_id.to_string()));
    obj.insert("output".to_string(), Value::String(output_json));
    ItemParam::new(Value::Object(obj))
}

async fn run_openresponses_agent_loop(
    http: &reqwest::Client,
    config: &OpenResponsesConfig,
    tool_runner: &ToolRunner,
    session_id: &str,
    prompt: &str,
    seq: &mut u64,
    sink: EventSink<'_>,
) -> String {
    let mut previous_response_id: Option<String> = None;
    let mut followup_tool_outputs: Option<Vec<ItemParam>> = None;
    let mut tool_call_count: u64 = 0;
    let stateless_history = config.stateless_history;
    let mut history_items = if stateless_history {
        vec![ItemParam::user_message_text(prompt)]
    } else {
        Vec::new()
    };

    loop {
        if tool_call_count >= DEFAULT_MAX_TOOL_CALLS {
            return "max_tool_calls_exceeded".to_string();
        }

        let payload = if let Some(tool_outputs) = followup_tool_outputs.take() {
            if stateless_history {
                build_streaming_followup_request(config, None, history_items.clone())
            } else {
                let Some(prev) = previous_response_id.as_deref() else {
                    return "provider_error".to_string();
                };
                build_streaming_followup_request(config, Some(prev), tool_outputs)
            }
        } else if stateless_history {
            build_streaming_request_items(config, history_items.clone())
        } else {
            build_streaming_request(config, prompt)
        };

        let mut collector = ToolCallCollector::default();
        let stream_result = stream_openresponses_request(
            http,
            config,
            session_id,
            payload,
            seq,
            sink,
            &mut collector,
        )
        .await;
        if let Err(reason) = stream_result {
            return reason;
        }

        if let Some(id) = collector.response_id.clone() {
            previous_response_id = Some(id);
        }

        let tool_calls = collector.drain_function_calls();
        if tool_calls.is_empty() {
            return "completed".to_string();
        }
        if previous_response_id.is_none() && !stateless_history {
            return "provider_error".to_string();
        }

        let mut tool_outputs = Vec::new();
        if stateless_history {
            for call in &tool_calls {
                history_items.push(function_call_item_from_call(call, true));
            }
        }
        for call in tool_calls {
            if tool_call_count >= DEFAULT_MAX_TOOL_CALLS {
                return "max_tool_calls_exceeded".to_string();
            }
            tool_call_count += 1;
            let args_value = match serde_json::from_str::<Value>(&call.arguments) {
                Ok(value) => value,
                Err(_) => Value::String(call.arguments.clone()),
            };
            let tool_events = tool_runner
                .run(
                    session_id,
                    seq,
                    ToolInvocation {
                        name: call.name.clone(),
                        args: args_value,
                        timeout_ms: None,
                    },
                )
                .await;
            sink.emit_all(tool_events.clone()).await;

            let output_value = tool_events_to_function_call_output(&call.name, &tool_events);
            let output_json = serde_json::to_string(&output_value)
                .unwrap_or_else(|_| "{\"ok\":false}".to_string());
            tool_outputs.push(function_call_output_item(
                &call.call_id,
                output_json,
                stateless_history,
            ));
        }

        if stateless_history {
            history_items.extend(tool_outputs.clone());
        }
        followup_tool_outputs = Some(tool_outputs);
    }
}

async fn stream_openresponses_request<'a>(
    http: &reqwest::Client,
    config: &OpenResponsesConfig,
    session_id: &str,
    payload: CreateResponsePayload,
    seq: &'a mut u64,
    sink: EventSink<'a>,
    collector: &mut ToolCallCollector,
) -> Result<(), String> {
    let validation = if config.stateless_history {
        ValidationOptions::compat_missing_item_ids()
    } else {
        ValidationOptions::strict()
    };

    if !payload.errors().is_empty() {
        sink.emit(Event {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            timestamp_ms: now_ms(),
            seq: *seq,
            kind: rip_kernel::EventKind::ProviderEvent {
                provider: "openresponses".to_string(),
                status: rip_kernel::ProviderEventStatus::InvalidJson,
                event_name: None,
                data: None,
                raw: Some(payload.body().to_string()),
                errors: payload.errors().to_vec(),
                response_errors: Vec::new(),
            },
        })
        .await;
        *seq += 1;
        return Err("invalid_request".to_string());
    }

    let mut request = http.post(&config.endpoint).json(payload.body());
    if let Some(key) = config.api_key.as_deref() {
        request = request.bearer_auth(key);
    }

    let response = match request.send().await {
        Ok(response) => response,
        Err(err) => {
            let mut pipe = OpenResponsesSsePipe::new(session_id, seq, sink, None, validation);
            pipe.emit_transport_error(err.to_string()).await;
            return Err("provider_error".to_string());
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let mut pipe = OpenResponsesSsePipe::new(session_id, seq, sink, None, validation);
        pipe.emit_transport_error(format!("provider http error: {status}: {body}"))
            .await;
        return Err("provider_error".to_string());
    }

    let mut utf8_buf = Vec::new();
    let mut pipe = OpenResponsesSsePipe::new(session_id, seq, sink, Some(collector), validation);
    let mut saw_done = false;

    let mut stream = response.bytes_stream();
    while let Some(next) = stream.next().await {
        let chunk = match next {
            Ok(chunk) => chunk,
            Err(err) => {
                pipe.emit_transport_error(err.to_string()).await;
                return Err("provider_error".to_string());
            }
        };
        saw_done = pipe.push_bytes(&mut utf8_buf, &chunk).await;
        if saw_done {
            break;
        }
    }

    if !saw_done {
        let _ = pipe.finish().await;
    }

    Ok(())
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn parse_action(input: &str) -> InputAction {
    let trimmed = input.trim();
    if trimmed.starts_with('{') {
        if let Ok(envelope) = serde_json::from_str::<CheckpointEnvelope>(trimmed) {
            return InputAction::Checkpoint(envelope.checkpoint);
        }
        if let Ok(command) = serde_json::from_str::<ToolCommand>(trimmed) {
            return InputAction::Tool(command);
        }
    }

    InputAction::Prompt
}

async fn emit_events(
    events: Vec<Event>,
    sender: &broadcast::Sender<Event>,
    buffer: &Arc<Mutex<Vec<Event>>>,
    event_log: &EventLog,
) {
    for event in events {
        emit_event(event, sender, buffer, event_log).await;
    }
}

async fn emit_event(
    event: Event,
    sender: &broadcast::Sender<Event>,
    buffer: &Arc<Mutex<Vec<Event>>>,
    event_log: &EventLog,
) {
    let _ = sender.send(event.clone());
    let mut guard = buffer.lock().await;
    guard.push(event.clone());
    let _ = event_log.append(&event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_action_accepts_tool_command() {
        let input = r#"{"tool":"write","args":{"path":"a.txt","content":"hi"}}"#;
        match parse_action(input) {
            InputAction::Tool(command) => {
                assert_eq!(command.tool, "write");
                assert!(command.args.get("path").is_some());
            }
            _ => panic!("expected tool action"),
        }
    }

    #[test]
    fn parse_action_accepts_checkpoint_command() {
        let input = r#"{"checkpoint":{"action":"create","label":"snap","files":["a.txt"]}}"#;
        match parse_action(input) {
            InputAction::Checkpoint(CheckpointCommand::Create { label, files }) => {
                assert_eq!(label, "snap");
                assert_eq!(files, vec!["a.txt".to_string()]);
            }
            _ => panic!("expected checkpoint create"),
        }
    }

    #[test]
    fn parse_action_defaults_to_prompt() {
        match parse_action("hello") {
            InputAction::Prompt => {}
            _ => panic!("expected prompt"),
        }
    }
}
