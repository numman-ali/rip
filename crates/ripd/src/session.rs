use std::collections::HashMap;
use std::path::{Path, PathBuf};
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

use crate::context_bundle::{write_bundle_v1, ContextBundleItemV1, ContextBundleV1};
use crate::context_compiler::{
    compile_recent_messages_v1, CompileRecentMessagesV1Request, CONTEXT_COMPILER_ID_V1,
    CONTEXT_COMPILER_STRATEGY_RECENT_MESSAGES_V1,
};
use crate::continuities::{
    ContextCompiledPayload, ContinuityRunLink, ContinuityStore, ToolSideEffects,
};
use crate::provider_openresponses::{
    build_streaming_followup_request, build_streaming_request, build_streaming_request_items,
    OpenResponsesConfig, DEFAULT_MAX_TOOL_CALLS,
};
use crate::workspace_lock::{requires_workspace_lock, WorkspaceLock};

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
    pub workspace_lock: Arc<WorkspaceLock>,
    pub http_client: reqwest::Client,
    pub openresponses: Option<OpenResponsesConfig>,
    pub sender: broadcast::Sender<Event>,
    pub events: Arc<Mutex<Vec<Event>>>,
    pub event_log: Arc<EventLog>,
    pub snapshot_dir: Arc<PathBuf>,
    pub continuities: Arc<ContinuityStore>,
    pub continuity_run: Option<ContinuityRunLink>,
    pub server_session_id: String,
    pub input: String,
}

pub async fn run_session(context: SessionContext) {
    let SessionContext {
        runtime,
        tool_runner,
        workspace_lock,
        http_client,
        openresponses,
        sender,
        events,
        event_log,
        snapshot_dir,
        continuities,
        continuity_run,
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
            let invocation = ToolInvocation {
                name: command.tool,
                args: command.args,
                timeout_ms: command.timeout_ms,
            };
            if requires_workspace_lock(&invocation.name) {
                let _guard = workspace_lock.acquire().await;
                let tool_events = tool_runner
                    .run(&runtime_session_id, &mut seq, invocation)
                    .await;
                let side_effects = summarize_continuity_tool_side_effects(&tool_events);
                session.set_seq(seq);
                emit_events(tool_events, &sender, &events, &event_log).await;
                if let (Some(link), Some(side_effects)) = (continuity_run.as_ref(), side_effects) {
                    let _ = continuities.append_tool_side_effects(
                        link,
                        &runtime_session_id,
                        side_effects,
                    );
                }
            } else {
                let tool_events = tool_runner
                    .run(&runtime_session_id, &mut seq, invocation)
                    .await;
                session.set_seq(seq);
                emit_events(tool_events, &sender, &events, &event_log).await;
            }
        }
        InputAction::Checkpoint(command) => {
            let mut seq = session.seq();
            let _guard = workspace_lock.acquire().await;
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
                let mut initial_items: Option<Vec<ItemParam>> = None;
                if let Some(link) = continuity_run.as_ref() {
                    match compile_context_bundle_for_run(
                        continuities.as_ref(),
                        event_log.as_ref(),
                        snapshot_dir.as_ref().as_path(),
                        link,
                        &runtime_session_id,
                    ) {
                        Ok((bundle_artifact_id, items, from_seq, from_message_id)) => {
                            let _ = continuities.append_context_compiled(
                                &link.continuity_id,
                                ContextCompiledPayload {
                                    run_session_id: runtime_session_id.clone(),
                                    bundle_artifact_id,
                                    compiler_id: CONTEXT_COMPILER_ID_V1.to_string(),
                                    compiler_strategy: CONTEXT_COMPILER_STRATEGY_RECENT_MESSAGES_V1
                                        .to_string(),
                                    from_seq,
                                    from_message_id,
                                    actor_id: link.actor_id.clone(),
                                    origin: link.origin.clone(),
                                },
                            );
                            initial_items = Some(items);
                        }
                        Err(_) => {
                            emit_event(
                                Event {
                                    id: Uuid::new_v4().to_string(),
                                    session_id: runtime_session_id.clone(),
                                    timestamp_ms: now_ms(),
                                    seq,
                                    kind: rip_kernel::EventKind::SessionEnded {
                                        reason: "context_compile_failed".to_string(),
                                    },
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
                    let reason = run_openresponses_agent_loop(OpenResponsesRunContext {
                        http: &http_client,
                        config,
                        tool_runner: tool_runner.as_ref(),
                        workspace_lock: workspace_lock.as_ref(),
                        continuities: continuities.as_ref(),
                        continuity_run: continuity_run.as_ref(),
                        session_id: &runtime_session_id,
                        initial_items,
                        prompt: &input,
                        seq: &mut seq,
                        sink,
                    })
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
    }

    if !skip_runtime_loop {
        while let Some(event) = session.next_event() {
            emit_event(event, &sender, &events, &event_log).await;
        }
    }

    let guard = events.lock().await;
    let reason = guard
        .iter()
        .rev()
        .find_map(|event| match &event.kind {
            EventKind::SessionEnded { reason } => Some(reason.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "unknown".to_string());
    let _ = write_snapshot(&*snapshot_dir, &server_session_id, &guard);
    drop(guard);

    if let Some(link) = continuity_run {
        let _ = continuities.append_run_ended(
            &link.continuity_id,
            &link.message_id,
            &runtime_session_id,
            reason,
            link.actor_id,
            link.origin,
        );
    }
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

fn summarize_continuity_tool_side_effects(events: &[Event]) -> Option<ToolSideEffects> {
    let (tool_id, tool_name) = events.iter().find_map(|event| match &event.kind {
        EventKind::ToolStarted { tool_id, name, .. } => Some((tool_id.clone(), name.clone())),
        _ => None,
    })?;

    let mut checkpoint_id: Option<String> = None;
    let mut checkpoint_files: Option<Vec<String>> = None;
    for event in events {
        if let EventKind::CheckpointCreated {
            checkpoint_id: id,
            files,
            auto: true,
            ..
        } = &event.kind
        {
            checkpoint_id = Some(id.clone());
            checkpoint_files = Some(files.clone());
            break;
        }
    }

    let mut artifacts: Option<Value> = None;
    for event in events {
        if let EventKind::ToolEnded {
            artifacts: tool_artifacts,
            ..
        } = &event.kind
        {
            artifacts = tool_artifacts.clone();
        }
    }

    let mut affected_paths: Option<Vec<String>> = None;
    if let Some(Value::Object(map)) = artifacts {
        if let Some(Value::Array(items)) = map.get("changed_files") {
            let mut paths = Vec::new();
            for item in items {
                if let Some(path) = item.as_str() {
                    paths.push(normalize_rel_path_string(path));
                }
            }
            affected_paths = Some(paths);
        } else if let Some(Value::String(path)) = map.get("path") {
            affected_paths = Some(vec![normalize_rel_path_string(path)]);
        }
    }

    if affected_paths.is_none() {
        affected_paths = checkpoint_files.map(|files| {
            files
                .into_iter()
                .map(|path| normalize_rel_path_string(&path))
                .collect()
        });
    }

    if let Some(paths) = affected_paths.as_mut() {
        paths.sort();
        paths.dedup();
    }

    Some(ToolSideEffects {
        tool_id,
        tool_name,
        affected_paths,
        checkpoint_id,
    })
}

fn normalize_rel_path_string(path: &str) -> String {
    path.replace('\\', "/")
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

fn openresponses_items_from_context_bundle(bundle: &ContextBundleV1) -> Vec<ItemParam> {
    bundle
        .items()
        .iter()
        .map(|item| match item {
            ContextBundleItemV1::Message { role, content, .. } => {
                ItemParam::message_text(role.clone(), content.clone())
            }
        })
        .collect()
}

fn compile_context_bundle_for_run(
    continuities: &ContinuityStore,
    event_log: &EventLog,
    snapshot_dir: &Path,
    run: &ContinuityRunLink,
    run_session_id: &str,
) -> Result<(String, Vec<ItemParam>, u64, Option<String>), String> {
    let input = continuities
        .load_context_compile_input_recent_messages_v1(&run.continuity_id, &run.message_id)?;

    let bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
        continuity_id: &run.continuity_id,
        continuity_events: &input.continuity_events,
        event_log,
        snapshot_dir,
        from_seq: input.from_seq,
        from_message_id: input.from_message_id.clone(),
        run_session_id,
        actor_id: &run.actor_id,
        origin: &run.origin,
    })?;
    let artifact_id = write_bundle_v1(continuities.workspace_root(), &bundle)?;
    let items = openresponses_items_from_context_bundle(&bundle);
    Ok((artifact_id, items, input.from_seq, input.from_message_id))
}

struct OpenResponsesRunContext<'a> {
    http: &'a reqwest::Client,
    config: &'a OpenResponsesConfig,
    tool_runner: &'a ToolRunner,
    workspace_lock: &'a WorkspaceLock,
    continuities: &'a ContinuityStore,
    continuity_run: Option<&'a ContinuityRunLink>,
    session_id: &'a str,
    initial_items: Option<Vec<ItemParam>>,
    prompt: &'a str,
    seq: &'a mut u64,
    sink: EventSink<'a>,
}

async fn run_openresponses_agent_loop(ctx: OpenResponsesRunContext<'_>) -> String {
    let OpenResponsesRunContext {
        http,
        config,
        tool_runner,
        workspace_lock,
        continuities,
        continuity_run,
        session_id,
        initial_items,
        prompt,
        seq,
        sink,
    } = ctx;
    let mut previous_response_id: Option<String> = None;
    let mut followup_tool_outputs: Option<Vec<ItemParam>> = None;
    let mut tool_call_count: u64 = 0;
    let stateless_history = config.stateless_history;
    let mut initial_request_items = initial_items;
    let mut history_items = if stateless_history {
        match initial_request_items.as_ref() {
            Some(items) => items.clone(),
            None => vec![ItemParam::user_message_text(prompt)],
        }
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
        } else if let Some(items) = initial_request_items.take() {
            build_streaming_request_items(config, items)
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
            let invocation = ToolInvocation {
                name: call.name.clone(),
                args: args_value,
                timeout_ms: None,
            };
            let output_value = if requires_workspace_lock(&invocation.name) {
                let _guard = workspace_lock.acquire().await;
                let tool_events = tool_runner.run(session_id, seq, invocation).await;
                let side_effects = summarize_continuity_tool_side_effects(&tool_events);
                let output_value = tool_events_to_function_call_output(&call.name, &tool_events);
                sink.emit_all(tool_events).await;
                if let (Some(link), Some(side_effects)) = (continuity_run, side_effects) {
                    let _ = continuities.append_tool_side_effects(link, session_id, side_effects);
                }
                output_value
            } else {
                let tool_events = tool_runner.run(session_id, seq, invocation).await;
                let output_value = tool_events_to_function_call_output(&call.name, &tool_events);
                sink.emit_all(tool_events).await;
                output_value
            };
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
    use rip_provider_openresponses::ToolChoiceParam;
    use tempfile::tempdir;

    fn make_event(kind: EventKind) -> Event {
        Event {
            id: "e1".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind,
        }
    }

    fn parsed_event(data: Value) -> ParsedEvent {
        let event_name = data
            .get("type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        ParsedEvent {
            kind: ParsedEventKind::Event,
            event: event_name,
            raw: data.to_string(),
            data: Some(data),
            errors: Vec::new(),
            response_errors: Vec::new(),
        }
    }

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

    #[test]
    fn parse_action_invalid_json_defaults_to_prompt() {
        match parse_action("{not json}") {
            InputAction::Prompt => {}
            _ => panic!("expected prompt"),
        }
    }

    #[test]
    fn tool_events_to_function_call_output_ok() {
        let events = vec![
            make_event(EventKind::ToolStdout {
                tool_id: "t1".to_string(),
                chunk: "out".to_string(),
            }),
            make_event(EventKind::ToolStderr {
                tool_id: "t1".to_string(),
                chunk: "err".to_string(),
            }),
            make_event(EventKind::ToolEnded {
                tool_id: "t1".to_string(),
                exit_code: 0,
                duration_ms: 1,
                artifacts: Some(serde_json::json!({"id": "a1"})),
            }),
        ];
        let value = tool_events_to_function_call_output("ls", &events);
        assert_eq!(value.get("ok").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(value.get("stdout").and_then(|v| v.as_str()), Some("out"));
        assert_eq!(value.get("stderr").and_then(|v| v.as_str()), Some("err"));
        assert_eq!(
            value
                .get("artifacts")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str()),
            Some("a1")
        );
    }

    #[test]
    fn tool_events_to_function_call_output_error() {
        let events = vec![
            make_event(EventKind::ToolStdout {
                tool_id: "t1".to_string(),
                chunk: "out".to_string(),
            }),
            make_event(EventKind::ToolFailed {
                tool_id: "t1".to_string(),
                error: "boom".to_string(),
            }),
        ];
        let value = tool_events_to_function_call_output("ls", &events);
        assert_eq!(value.get("ok").and_then(|v| v.as_bool()), Some(false));
        assert_eq!(value.get("error").and_then(|v| v.as_str()), Some("boom"));
    }

    #[test]
    fn function_call_item_includes_id_when_requested() {
        let call = FunctionCallItem {
            output_index: 0,
            call_id: "call_1".to_string(),
            item_id: Some("item_1".to_string()),
            name: "ls".to_string(),
            arguments: "{}".to_string(),
        };
        let item = function_call_item_from_call(&call, true);
        let value = item.value();
        assert_eq!(value.get("id").and_then(|v| v.as_str()), Some("item_1"));
    }

    #[test]
    fn function_call_item_generates_id_when_missing() {
        let call = FunctionCallItem {
            output_index: 0,
            call_id: "call_2".to_string(),
            item_id: None,
            name: "ls".to_string(),
            arguments: "{}".to_string(),
        };
        let item = function_call_item_from_call(&call, true);
        let value = item.value();
        assert_eq!(value.get("id").and_then(|v| v.as_str()), Some("fc_call_2"));
    }

    #[test]
    fn function_call_item_omits_id_when_disabled() {
        let call = FunctionCallItem {
            output_index: 0,
            call_id: "call_3".to_string(),
            item_id: Some("item_3".to_string()),
            name: "ls".to_string(),
            arguments: "{}".to_string(),
        };
        let item = function_call_item_from_call(&call, false);
        let value = item.value();
        assert!(value.get("id").is_none());
    }

    #[test]
    fn function_call_output_item_sets_id_when_enabled() {
        let item = function_call_output_item("call_4", "{\"ok\":true}".to_string(), true);
        let value = item.value();
        assert_eq!(
            value.get("id").and_then(|v| v.as_str()),
            Some("output_call_4")
        );
    }

    #[test]
    fn function_call_output_item_omits_id_when_disabled() {
        let item = function_call_output_item("call_5", "{\"ok\":true}".to_string(), false);
        let value = item.value();
        assert!(value.get("id").is_none());
    }

    #[test]
    fn tool_call_collector_tracks_function_calls() {
        let mut collector = ToolCallCollector::default();
        let added = parsed_event(serde_json::json!({
            "type": "response.output_item.added",
            "output_index": 1,
            "item": {
                "type": "function_call",
                "call_id": "call_1",
                "name": "ls",
                "arguments": "{\"path\":\".\"}"
            }
        }));
        collector.observe(&added);

        let delta = parsed_event(serde_json::json!({
            "type": "response.function_call_arguments.delta",
            "item_id": "call_1",
            "output_index": 1,
            "delta": "{\"path\":\".\"}"
        }));
        collector.observe(&delta);

        let done = parsed_event(serde_json::json!({
            "type": "response.output_item.done",
            "output_index": 1,
            "item": {
                "type": "function_call",
                "call_id": "call_1",
                "name": "ls"
            }
        }));
        collector.observe(&done);

        let calls = collector.drain_function_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].call_id, "call_1");
        assert_eq!(calls[0].name, "ls");
    }

    #[test]
    fn tool_call_collector_sorts_by_output_index() {
        let mut collector = ToolCallCollector::default();
        collector.completed_function_calls.push(FunctionCallItem {
            output_index: 2,
            call_id: "call_2".to_string(),
            item_id: None,
            name: "ls".to_string(),
            arguments: "{}".to_string(),
        });
        collector.completed_function_calls.push(FunctionCallItem {
            output_index: 1,
            call_id: "call_1".to_string(),
            item_id: None,
            name: "ls".to_string(),
            arguments: "{}".to_string(),
        });
        let calls = collector.drain_function_calls();
        assert_eq!(calls[0].output_index, 1);
        assert_eq!(calls[1].output_index, 2);
    }

    #[tokio::test]
    async fn openresponses_pipe_emits_done_event() {
        let dir = tempdir().expect("tmp");
        let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (sender, _) = broadcast::channel(8);
        let mut seq = 0;
        let sink = EventSink {
            sender: &sender,
            buffer: &buffer,
            event_log: &log,
        };
        let mut pipe =
            OpenResponsesSsePipe::new("s1", &mut seq, sink, None, ValidationOptions::strict());
        let saw_done = pipe.push_sse_str("data: [DONE]\n\n").await;
        assert!(saw_done);
        assert_eq!(seq, 1);
        let events = buffer.lock().await;
        assert_eq!(events.len(), 1);
        match &events[0].kind {
            EventKind::ProviderEvent { status, .. } => {
                assert_eq!(*status, rip_kernel::ProviderEventStatus::Done);
            }
            _ => panic!("expected provider_event"),
        }
    }

    #[tokio::test]
    async fn openresponses_pipe_emits_output_text_delta() {
        let dir = tempdir().expect("tmp");
        let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (sender, _) = broadcast::channel(8);
        let mut seq = 0;
        let sink = EventSink {
            sender: &sender,
            buffer: &buffer,
            event_log: &log,
        };
        let mut pipe =
            OpenResponsesSsePipe::new("s1", &mut seq, sink, None, ValidationOptions::strict());
        let saw_done = pipe
            .push_sse_str("data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n")
            .await;
        assert!(!saw_done);
        assert_eq!(seq, 2);
        let events = buffer.lock().await;
        assert_eq!(events.len(), 2);
        match &events[1].kind {
            EventKind::OutputTextDelta { delta } => assert_eq!(delta, "hi"),
            _ => panic!("expected output_text_delta"),
        }
    }

    #[tokio::test]
    async fn stream_openresponses_request_rejects_invalid_payload() {
        let dir = tempdir().expect("tmp");
        let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (sender, _) = broadcast::channel(8);
        let sink = EventSink {
            sender: &sender,
            buffer: &buffer,
            event_log: &log,
        };
        let config = OpenResponsesConfig {
            endpoint: "http://example.test/v1/responses".to_string(),
            api_key: None,
            model: None,
            tool_choice: ToolChoiceParam::auto(),
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = CreateResponsePayload::new(serde_json::json!({"input": {}}));
        let mut seq = 0;
        let mut collector = ToolCallCollector::default();
        let err = stream_openresponses_request(
            &reqwest::Client::new(),
            &config,
            "s1",
            payload,
            &mut seq,
            sink,
            &mut collector,
        )
        .await
        .unwrap_err();
        assert_eq!(err, "invalid_request");
        let events = buffer.lock().await;
        assert_eq!(events.len(), 1);
        match &events[0].kind {
            EventKind::ProviderEvent { status, errors, .. } => {
                assert_eq!(*status, rip_kernel::ProviderEventStatus::InvalidJson);
                assert!(!errors.is_empty());
            }
            _ => panic!("expected provider_event"),
        }
    }

    #[test]
    fn tool_call_collector_uses_arguments_done() {
        let mut collector = ToolCallCollector::default();
        let added = parsed_event(serde_json::json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": {
                "type": "function_call",
                "id": "item_1",
                "call_id": "call_1",
                "name": "read"
            }
        }));
        collector.observe(&added);

        let args_done = parsed_event(serde_json::json!({
            "type": "response.function_call_arguments.done",
            "item_id": "item_1",
            "output_index": 0,
            "arguments": "{\"path\":\"a.txt\"}"
        }));
        collector.observe(&args_done);

        let done = parsed_event(serde_json::json!({
            "type": "response.output_item.done",
            "output_index": 0,
            "item": {
                "type": "function_call",
                "id": "item_1",
                "call_id": "call_1",
                "name": "read"
            }
        }));
        collector.observe(&done);

        let calls = collector.drain_function_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].call_id, "call_1");
        assert_eq!(calls[0].arguments, "{\"path\":\"a.txt\"}");
    }

    #[test]
    fn tool_call_collector_records_response_id_even_without_event_type() {
        let mut collector = ToolCallCollector::default();
        let event = ParsedEvent {
            kind: ParsedEventKind::Event,
            event: None,
            raw: "{\"response\":{\"id\":\"resp_1\"}}".to_string(),
            data: Some(serde_json::json!({"response": {"id": "resp_1"}})),
            errors: Vec::new(),
            response_errors: Vec::new(),
        };
        collector.observe(&event);
        assert_eq!(collector.response_id.as_deref(), Some("resp_1"));
    }

    #[test]
    fn tool_call_collector_ignores_non_function_items() {
        let mut collector = ToolCallCollector::default();
        let added = parsed_event(serde_json::json!({
            "type": "response.output_item.added",
            "output_index": 0,
            "item": {
                "type": "output_text",
                "text": "hi"
            }
        }));
        collector.observe(&added);
        assert!(collector.drain_function_calls().is_empty());
    }

    #[tokio::test]
    async fn openresponses_pipe_handles_invalid_utf8_bytes() {
        let dir = tempdir().expect("tmp");
        let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (sender, _) = broadcast::channel(8);
        let mut seq = 0;
        let sink = EventSink {
            sender: &sender,
            buffer: &buffer,
            event_log: &log,
        };
        let mut pipe =
            OpenResponsesSsePipe::new("s1", &mut seq, sink, None, ValidationOptions::strict());
        let mut utf8_buf = Vec::new();
        let saw_done = pipe.push_bytes(&mut utf8_buf, &[0xFF]).await;
        assert!(!saw_done);
        assert!(utf8_buf.is_empty());
        let _ = pipe.push_bytes(&mut utf8_buf, b"data: [DONE]\n\n").await;
    }

    #[tokio::test]
    async fn openresponses_pipe_handles_partial_utf8_sequence() {
        let dir = tempdir().expect("tmp");
        let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (sender, _) = broadcast::channel(8);
        let mut seq = 0;
        let sink = EventSink {
            sender: &sender,
            buffer: &buffer,
            event_log: &log,
        };
        let mut pipe =
            OpenResponsesSsePipe::new("s1", &mut seq, sink, None, ValidationOptions::strict());
        let mut utf8_buf = Vec::new();
        let mut chunk = b"data: ".to_vec();
        chunk.push(0xF0);
        let saw_done = pipe.push_bytes(&mut utf8_buf, &chunk).await;
        assert!(!saw_done);
        assert_eq!(utf8_buf, vec![0xF0]);
    }

    #[tokio::test]
    async fn openresponses_pipe_finish_flushes_done() {
        let dir = tempdir().expect("tmp");
        let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (sender, _) = broadcast::channel(8);
        let mut seq = 0;
        let sink = EventSink {
            sender: &sender,
            buffer: &buffer,
            event_log: &log,
        };
        let mut pipe =
            OpenResponsesSsePipe::new("s1", &mut seq, sink, None, ValidationOptions::strict());
        let saw_done = pipe.push_sse_str("data: [DONE]\n").await;
        assert!(!saw_done);
        let saw_done = pipe.finish().await;
        assert!(!saw_done);
    }

    #[tokio::test]
    async fn stream_openresponses_request_reports_transport_error() {
        let dir = tempdir().expect("tmp");
        let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (sender, _) = broadcast::channel(8);
        let sink = EventSink {
            sender: &sender,
            buffer: &buffer,
            event_log: &log,
        };
        let config = OpenResponsesConfig {
            endpoint: "http://127.0.0.1:0/v1/responses".to_string(),
            api_key: None,
            model: Some("fixture-model".to_string()),
            tool_choice: ToolChoiceParam::auto(),
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "hi");
        assert!(payload.errors().is_empty());
        let mut seq = 0;
        let mut collector = ToolCallCollector::default();
        let err = stream_openresponses_request(
            &reqwest::Client::new(),
            &config,
            "s1",
            payload,
            &mut seq,
            sink,
            &mut collector,
        )
        .await
        .unwrap_err();
        assert_eq!(err, "provider_error");
        let events = buffer.lock().await;
        assert_eq!(events.len(), 1);
        match &events[0].kind {
            EventKind::ProviderEvent { status, .. } => {
                assert_eq!(*status, rip_kernel::ProviderEventStatus::InvalidJson);
            }
            _ => panic!("expected provider_event"),
        }
    }

    #[tokio::test]
    async fn stream_openresponses_request_reports_http_error() {
        use axum::http::StatusCode;
        use axum::routing::post;
        use axum::{response::IntoResponse, Router as AxumRouter};
        use tokio::net::TcpListener;

        let provider_app = AxumRouter::new().route(
            "/v1/responses",
            post(|| async move { (StatusCode::BAD_REQUEST, "boom").into_response() }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            axum::serve(listener, provider_app).await.expect("serve");
        });

        let dir = tempdir().expect("tmp");
        let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (sender, _) = broadcast::channel(8);
        let sink = EventSink {
            sender: &sender,
            buffer: &buffer,
            event_log: &log,
        };
        let config = OpenResponsesConfig {
            endpoint: format!("http://{addr}/v1/responses"),
            api_key: None,
            model: Some("fixture-model".to_string()),
            tool_choice: ToolChoiceParam::auto(),
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let payload = build_streaming_request(&config, "hi");
        assert!(payload.errors().is_empty());
        let mut seq = 0;
        let mut collector = ToolCallCollector::default();
        let err = stream_openresponses_request(
            &reqwest::Client::new(),
            &config,
            "s1",
            payload,
            &mut seq,
            sink,
            &mut collector,
        )
        .await
        .unwrap_err();
        assert_eq!(err, "provider_error");
        let events = buffer.lock().await;
        assert_eq!(events.len(), 1);
        match &events[0].kind {
            EventKind::ProviderEvent { status, raw, .. } => {
                assert_eq!(*status, rip_kernel::ProviderEventStatus::InvalidJson);
                assert!(raw.as_deref().unwrap_or("").contains("provider http error"));
            }
            _ => panic!("expected provider_event"),
        }
    }

    #[tokio::test]
    async fn run_openresponses_agent_loop_stateless_completes() {
        use axum::http::header::CONTENT_TYPE;
        use axum::routing::post;
        use axum::Router as AxumRouter;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::net::TcpListener;

        let tool_sse = "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"noop\",\"arguments\":\"{}\"}}\n\n\
data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"noop\",\"arguments\":\"{}\"}}\n\n\
data: [DONE]\n\n";
        let output_sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n\
data: [DONE]\n\n";

        let counter = Arc::new(AtomicUsize::new(0));
        let tool_sse = Arc::new(tool_sse.to_string());
        let output_sse = Arc::new(output_sse.to_string());
        let provider_app = AxumRouter::new().route(
            "/v1/responses",
            post({
                let counter = counter.clone();
                let tool_sse = tool_sse.clone();
                let output_sse = output_sse.clone();
                move || {
                    let counter = counter.clone();
                    let tool_sse = tool_sse.clone();
                    let output_sse = output_sse.clone();
                    async move {
                        let idx = counter.fetch_add(1, Ordering::SeqCst);
                        let body = if idx == 0 { tool_sse } else { output_sse };
                        ([(CONTENT_TYPE, "text/event-stream")], body.to_string())
                    }
                }
            }),
        );

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            axum::serve(listener, provider_app).await.expect("serve");
        });

        let dir = tempdir().expect("tmp");
        let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (sender, _) = broadcast::channel(8);
        let sink = EventSink {
            sender: &sender,
            buffer: &buffer,
            event_log: &log,
        };

        let registry = Arc::new(rip_tools::ToolRegistry::default());
        registry.register(
            "noop",
            Arc::new(|_invocation| Box::pin(async { rip_tools::ToolOutput::success(vec![]) })),
        );
        let tool_runner = ToolRunner::new(registry, 1);
        let workspace_lock = crate::workspace_lock::WorkspaceLock::new();
        let continuity_workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&continuity_workspace).expect("workspace");
        let continuity_log =
            Arc::new(EventLog::new(dir.path().join("continuity_events.jsonl")).expect("log"));
        let continuity_store = ContinuityStore::new(
            dir.path().join("continuity_data"),
            continuity_workspace,
            continuity_log,
        )
        .expect("continuities");

        let config = OpenResponsesConfig {
            endpoint: format!("http://{addr}/v1/responses"),
            api_key: None,
            model: Some("fixture-model".to_string()),
            tool_choice: ToolChoiceParam::auto(),
            followup_user_message: None,
            stateless_history: true,
            parallel_tool_calls: false,
        };
        let mut seq = 0;
        let http = reqwest::Client::new();
        let reason = run_openresponses_agent_loop(OpenResponsesRunContext {
            http: &http,
            config: &config,
            tool_runner: &tool_runner,
            workspace_lock: &workspace_lock,
            continuities: &continuity_store,
            continuity_run: None,
            session_id: "s1",
            initial_items: None,
            prompt: "hi",
            seq: &mut seq,
            sink,
        })
        .await;
        assert_eq!(reason, "completed");
        let events = buffer.lock().await;
        assert!(events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ToolStarted { .. })));
    }

    #[tokio::test]
    async fn run_openresponses_agent_loop_requires_previous_response_id() {
        use axum::http::header::CONTENT_TYPE;
        use axum::routing::post;
        use axum::Router as AxumRouter;
        use tokio::net::TcpListener;

        let tool_sse = "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"noop\",\"arguments\":\"{}\"}}\n\n\
data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"noop\",\"arguments\":\"{}\"}}\n\n\
data: [DONE]\n\n";
        let provider_app = AxumRouter::new().route(
            "/v1/responses",
            post({
                let tool_sse = tool_sse.to_string();
                move || {
                    let tool_sse = tool_sse.clone();
                    async move { ([(CONTENT_TYPE, "text/event-stream")], tool_sse) }
                }
            }),
        );

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            axum::serve(listener, provider_app).await.expect("serve");
        });

        let dir = tempdir().expect("tmp");
        let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (sender, _) = broadcast::channel(8);
        let sink = EventSink {
            sender: &sender,
            buffer: &buffer,
            event_log: &log,
        };

        let registry = Arc::new(rip_tools::ToolRegistry::default());
        registry.register(
            "noop",
            Arc::new(|_invocation| Box::pin(async { rip_tools::ToolOutput::success(vec![]) })),
        );
        let tool_runner = ToolRunner::new(registry, 1);
        let workspace_lock = crate::workspace_lock::WorkspaceLock::new();
        let continuity_workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&continuity_workspace).expect("workspace");
        let continuity_log =
            Arc::new(EventLog::new(dir.path().join("continuity_events.jsonl")).expect("log"));
        let continuity_store = ContinuityStore::new(
            dir.path().join("continuity_data"),
            continuity_workspace,
            continuity_log,
        )
        .expect("continuities");

        let config = OpenResponsesConfig {
            endpoint: format!("http://{addr}/v1/responses"),
            api_key: None,
            model: Some("fixture-model".to_string()),
            tool_choice: ToolChoiceParam::auto(),
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let mut seq = 0;
        let http = reqwest::Client::new();
        let reason = run_openresponses_agent_loop(OpenResponsesRunContext {
            http: &http,
            config: &config,
            tool_runner: &tool_runner,
            workspace_lock: &workspace_lock,
            continuities: &continuity_store,
            continuity_run: None,
            session_id: "s1",
            initial_items: None,
            prompt: "hi",
            seq: &mut seq,
            sink,
        })
        .await;
        assert_eq!(reason, "provider_error");
    }

    #[tokio::test]
    async fn run_openresponses_agent_loop_stateful_completes() {
        use axum::http::header::CONTENT_TYPE;
        use axum::routing::post;
        use axum::Router as AxumRouter;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::net::TcpListener;

        let tool_sse = "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n\
data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"noop\",\"arguments\":\"{}\"}}\n\n\
data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"noop\",\"arguments\":\"{}\"}}\n\n\
data: [DONE]\n\n";
        let output_sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n\
data: [DONE]\n\n";

        let counter = Arc::new(AtomicUsize::new(0));
        let tool_sse = Arc::new(tool_sse.to_string());
        let output_sse = Arc::new(output_sse.to_string());
        let provider_app = AxumRouter::new().route(
            "/v1/responses",
            post({
                let counter = counter.clone();
                let tool_sse = tool_sse.clone();
                let output_sse = output_sse.clone();
                move || {
                    let counter = counter.clone();
                    let tool_sse = tool_sse.clone();
                    let output_sse = output_sse.clone();
                    async move {
                        let idx = counter.fetch_add(1, Ordering::SeqCst);
                        let body = if idx == 0 { tool_sse } else { output_sse };
                        ([(CONTENT_TYPE, "text/event-stream")], body.to_string())
                    }
                }
            }),
        );

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            axum::serve(listener, provider_app).await.expect("serve");
        });

        let dir = tempdir().expect("tmp");
        let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let (sender, _) = broadcast::channel(8);
        let sink = EventSink {
            sender: &sender,
            buffer: &buffer,
            event_log: &log,
        };

        let registry = Arc::new(rip_tools::ToolRegistry::default());
        registry.register(
            "noop",
            Arc::new(|_invocation| Box::pin(async { rip_tools::ToolOutput::success(vec![]) })),
        );
        let tool_runner = ToolRunner::new(registry, 1);
        let workspace_lock = crate::workspace_lock::WorkspaceLock::new();
        let continuity_workspace = dir.path().join("workspace");
        std::fs::create_dir_all(&continuity_workspace).expect("workspace");
        let continuity_log =
            Arc::new(EventLog::new(dir.path().join("continuity_events.jsonl")).expect("log"));
        let continuity_store = ContinuityStore::new(
            dir.path().join("continuity_data"),
            continuity_workspace,
            continuity_log,
        )
        .expect("continuities");

        let config = OpenResponsesConfig {
            endpoint: format!("http://{addr}/v1/responses"),
            api_key: None,
            model: Some("fixture-model".to_string()),
            tool_choice: ToolChoiceParam::auto(),
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        };
        let mut seq = 0;
        let http = reqwest::Client::new();
        let reason = run_openresponses_agent_loop(OpenResponsesRunContext {
            http: &http,
            config: &config,
            tool_runner: &tool_runner,
            workspace_lock: &workspace_lock,
            continuities: &continuity_store,
            continuity_run: None,
            session_id: "s1",
            initial_items: None,
            prompt: "hi",
            seq: &mut seq,
            sink,
        })
        .await;
        assert_eq!(reason, "completed");
        let events = buffer.lock().await;
        assert!(events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ToolStarted { .. })));
    }

    #[tokio::test]
    async fn run_session_with_tool_invocation_emits_events() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");
        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let continuities = Arc::new(
            ContinuityStore::new(data_dir, workspace_dir, event_log.clone()).expect("continuities"),
        );
        let snapshot_dir = Arc::new(dir.path().join("snapshots"));
        let runtime = Arc::new(Runtime::new());

        let registry = Arc::new(rip_tools::ToolRegistry::default());
        registry.register(
            "noop",
            Arc::new(|_invocation| Box::pin(async { rip_tools::ToolOutput::success(vec![]) })),
        );
        let tool_runner = Arc::new(ToolRunner::new(registry, 1));
        let workspace_lock = Arc::new(crate::workspace_lock::WorkspaceLock::new());

        let (sender, _) = broadcast::channel(8);
        let events = Arc::new(Mutex::new(Vec::new()));
        let ctx = SessionContext {
            runtime,
            tool_runner,
            workspace_lock,
            http_client: reqwest::Client::new(),
            openresponses: None,
            sender,
            events: events.clone(),
            event_log,
            snapshot_dir,
            continuities,
            continuity_run: None,
            server_session_id: "s1".to_string(),
            input: "{\"tool\":\"noop\",\"args\":{}}".to_string(),
        };

        run_session(ctx).await;
        let guard = events.lock().await;
        assert!(guard
            .iter()
            .any(|event| matches!(event.kind, EventKind::ToolStarted { .. })));
        assert!(guard
            .iter()
            .any(|event| matches!(event.kind, EventKind::ToolEnded { .. })));
    }
}
