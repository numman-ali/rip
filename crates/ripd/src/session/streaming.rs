use super::*;

#[derive(Clone, Copy)]
pub(super) struct EventSink<'a> {
    sender: &'a broadcast::Sender<Event>,
    buffer: &'a Arc<Mutex<Vec<Event>>>,
    event_log: &'a EventLog,
}

impl<'a> EventSink<'a> {
    pub(super) fn new(
        sender: &'a broadcast::Sender<Event>,
        buffer: &'a Arc<Mutex<Vec<Event>>>,
        event_log: &'a EventLog,
    ) -> Self {
        Self {
            sender,
            buffer,
            event_log,
        }
    }

    pub(super) async fn emit(self, event: Event) {
        super::emit_event(event, self.sender, self.buffer, self.event_log).await;
    }

    pub(super) async fn emit_all(self, events: Vec<Event>) {
        super::emit_events(events, self.sender, self.buffer, self.event_log).await;
    }
}

pub(super) struct OpenResponsesSsePipe<'a> {
    session_id: String,
    decoder: SseDecoder,
    mapper: EventFrameMapper,
    seq_offset: u64,
    seq: &'a mut u64,
    sink: EventSink<'a>,
    collector: Option<&'a mut ToolCallCollector>,
}

impl<'a> OpenResponsesSsePipe<'a> {
    pub(super) fn new(
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

    pub(super) async fn emit_transport_error(&mut self, error: String) {
        self.sink
            .emit(Event {
                id: Uuid::new_v4().to_string(),
                session_id: self.session_id.clone(),
                timestamp_ms: super::now_ms(),
                seq: *self.seq,
                kind: rip_kernel::EventKind::ProviderEvent {
                    provider: "openresponses".to_string(),
                    status: rip_kernel::ProviderEventStatus::Event,
                    event_name: None,
                    data: None,
                    raw: None,
                    errors: vec![error],
                    response_errors: Vec::new(),
                },
            })
            .await;
        *self.seq += 1;
    }

    pub(super) async fn push_sse_str(&mut self, chunk: &str) -> bool {
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

    pub(super) async fn push_bytes(&mut self, utf8_buf: &mut Vec<u8>, bytes: &[u8]) -> bool {
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

    pub(super) async fn finish(&mut self) -> bool {
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
pub(super) struct FunctionCallItem {
    pub(super) output_index: u64,
    pub(super) call_id: String,
    pub(super) item_id: Option<String>,
    pub(super) name: String,
    pub(super) arguments: String,
}

#[derive(Debug, Clone, Default)]
struct FunctionCallBuffer {
    output_index: u64,
    call_id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Debug, Default)]
pub(super) struct ToolCallCollector {
    pub(super) response_id: Option<String>,
    function_call_by_item_id: HashMap<String, FunctionCallBuffer>,
    item_id_by_call_id: HashMap<String, String>,
    pub(super) completed_function_calls: Vec<FunctionCallItem>,
}

impl ToolCallCollector {
    pub(super) fn observe(&mut self, parsed: &ParsedEvent) {
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

    pub(super) fn drain_function_calls(&mut self) -> Vec<FunctionCallItem> {
        let mut calls = std::mem::take(&mut self.completed_function_calls);
        calls.sort_by_key(|call| call.output_index);
        calls
    }
}

pub(super) fn tool_events_to_function_call_output(tool_name: &str, events: &[Event]) -> Value {
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

pub(super) fn summarize_continuity_tool_side_effects(events: &[Event]) -> Option<ToolSideEffects> {
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

pub(super) fn function_call_item_from_call(call: &FunctionCallItem, include_id: bool) -> ItemParam {
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

pub(super) fn function_call_output_item(
    call_id: &str,
    output_json: String,
    include_id: bool,
) -> ItemParam {
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
