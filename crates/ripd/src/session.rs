use std::path::PathBuf;
use std::sync::Arc;

use futures_util::StreamExt;
use rip_kernel::{Event, Runtime};
use rip_log::{write_snapshot, EventLog};
use rip_provider_openresponses::{EventFrameMapper, ParsedEventKind, SseDecoder};
use rip_tools::{ToolInvocation, ToolRunner};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::provider_openresponses::{build_streaming_request, OpenResponsesConfig};

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
                let reason = stream_openresponses_prompt(
                    &http_client,
                    config,
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
}

impl<'a> OpenResponsesSsePipe<'a> {
    fn new(session_id: &str, seq: &'a mut u64, sink: EventSink<'a>) -> Self {
        Self {
            session_id: session_id.to_string(),
            decoder: SseDecoder::new(),
            mapper: EventFrameMapper::new(session_id.to_string()),
            seq_offset: *seq,
            seq,
            sink,
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

async fn stream_openresponses_prompt(
    http: &reqwest::Client,
    config: &OpenResponsesConfig,
    session_id: &str,
    prompt: &str,
    seq: &mut u64,
    sink: EventSink<'_>,
) -> String {
    let payload = build_streaming_request(config, prompt);
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
        return "invalid_request".to_string();
    }

    let mut request = http.post(&config.endpoint).json(payload.body());
    if let Some(key) = config.api_key.as_deref() {
        request = request.bearer_auth(key);
    }

    let response = match request.send().await {
        Ok(response) => response,
        Err(err) => {
            let mut pipe = OpenResponsesSsePipe::new(session_id, seq, sink);
            pipe.emit_transport_error(err.to_string()).await;
            return "provider_error".to_string();
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let mut pipe = OpenResponsesSsePipe::new(session_id, seq, sink);
        pipe.emit_transport_error(format!("provider http error: {status}: {body}"))
            .await;
        return "provider_error".to_string();
    }

    let mut utf8_buf = Vec::new();
    let mut pipe = OpenResponsesSsePipe::new(session_id, seq, sink);
    let mut saw_done = false;

    let mut stream = response.bytes_stream();
    while let Some(next) = stream.next().await {
        let chunk = match next {
            Ok(chunk) => chunk,
            Err(err) => {
                pipe.emit_transport_error(err.to_string()).await;
                return "provider_error".to_string();
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

    "completed".to_string()
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
