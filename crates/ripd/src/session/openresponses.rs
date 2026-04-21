use super::streaming::{
    function_call_item_from_call, function_call_output_item, tool_events_to_function_call_output,
    EventSink, OpenResponsesSsePipe, ToolCallCollector,
};
use super::*;
use crate::openresponses_compat::resolve_openresponses_compat_profile;

pub(super) struct OpenResponsesRunContext<'a> {
    pub(super) http: &'a reqwest::Client,
    pub(super) config: &'a OpenResponsesConfig,
    pub(super) tool_runner: &'a ToolRunner,
    pub(super) workspace_lock: &'a WorkspaceLock,
    pub(super) continuities: &'a ContinuityStore,
    pub(super) continuity_run: Option<&'a ContinuityRunLink>,
    pub(super) session_id: &'a str,
    pub(super) initial_items: Option<Vec<ItemParam>>,
    pub(super) prompt: &'a str,
    pub(super) seq: &'a mut u64,
    pub(super) sink: EventSink<'a>,
}

pub(super) struct OpenResponsesLoopOutcome {
    pub(super) reason: String,
    pub(super) last_response_id: Option<String>,
}

#[derive(Debug, Clone)]
enum ToolChoiceEnforcement {
    AllFunctions,
    NoTools,
    OnlyFunctions(HashSet<String>),
}

impl ToolChoiceEnforcement {
    fn from_tool_choice(tool_choice: &rip_provider_openresponses::ToolChoiceParam) -> Self {
        Self::from_value(tool_choice.value())
    }

    fn from_value(value: &Value) -> Self {
        match value {
            Value::String(value) => match value.as_str() {
                "none" => Self::NoTools,
                _ => Self::AllFunctions,
            },
            Value::Object(obj) => match obj.get("type").and_then(|value| value.as_str()) {
                Some("function") => {
                    let mut allowed = HashSet::new();
                    if let Some(name) = obj.get("name").and_then(|value| value.as_str()) {
                        if !name.is_empty() {
                            allowed.insert(name.to_string());
                        }
                    }
                    Self::OnlyFunctions(allowed)
                }
                Some("allowed_tools") => {
                    if obj.get("mode").and_then(|value| value.as_str()) == Some("none") {
                        return Self::NoTools;
                    }
                    let mut allowed = HashSet::new();
                    if let Some(tools) = obj.get("tools").and_then(|value| value.as_array()) {
                        for tool in tools {
                            let Some(tool) = tool.as_object() else {
                                continue;
                            };
                            if tool.get("type").and_then(|value| value.as_str()) != Some("function")
                            {
                                continue;
                            }
                            let Some(name) = tool.get("name").and_then(|value| value.as_str())
                            else {
                                continue;
                            };
                            if name.is_empty() {
                                continue;
                            }
                            allowed.insert(name.to_string());
                        }
                    }
                    Self::OnlyFunctions(allowed)
                }
                _ => Self::AllFunctions,
            },
            _ => Self::AllFunctions,
        }
    }

    fn allows_function(&self, name: &str) -> bool {
        match self {
            ToolChoiceEnforcement::AllFunctions => true,
            ToolChoiceEnforcement::NoTools => false,
            ToolChoiceEnforcement::OnlyFunctions(allowed) => allowed.contains(name),
        }
    }
}

fn rejected_tool_invocation_events(
    session_id: &str,
    seq: &mut u64,
    invocation: &ToolInvocation,
    call_id: &str,
    error: &str,
) -> Vec<Event> {
    let tool_id = format!("tool_denied_{call_id}");
    let started = Event {
        id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        timestamp_ms: super::now_ms(),
        seq: *seq,
        kind: EventKind::ToolStarted {
            tool_id: tool_id.clone(),
            name: invocation.name.clone(),
            args: invocation.args.clone(),
            timeout_ms: invocation.timeout_ms,
        },
    };
    *seq += 1;

    let failed = Event {
        id: Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        timestamp_ms: super::now_ms(),
        seq: *seq,
        kind: EventKind::ToolFailed {
            tool_id,
            error: error.to_string(),
        },
    };
    *seq += 1;

    vec![started, failed]
}

pub(super) async fn run_openresponses_agent_loop(
    ctx: OpenResponsesRunContext<'_>,
) -> OpenResponsesLoopOutcome {
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
    let mut request_index: u64 = 0;
    let compat = resolve_openresponses_compat_profile(
        config.provider_id.as_deref(),
        &config.endpoint,
        config.model.as_deref(),
    );
    let conversation = compat.conversation(config.stateless_history);
    let stateless_history = matches!(
        conversation.effective,
        crate::openresponses_compat::ConversationStrategy::StatelessHistory
    );
    let tool_choice_enforcement = ToolChoiceEnforcement::from_tool_choice(&config.tool_choice);
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
            return OpenResponsesLoopOutcome {
                reason: "max_tool_calls_exceeded".to_string(),
                last_response_id: previous_response_id,
            };
        }

        let (payload, request_kind) = if let Some(tool_outputs) = followup_tool_outputs.take() {
            if stateless_history {
                (
                    build_streaming_followup_request(config, None, history_items.clone()),
                    "followup_stateless_history",
                )
            } else {
                let Some(prev) = previous_response_id.as_deref() else {
                    return OpenResponsesLoopOutcome {
                        reason: "provider_error".to_string(),
                        last_response_id: previous_response_id,
                    };
                };
                (
                    build_streaming_followup_request(config, Some(prev), tool_outputs),
                    "followup",
                )
            }
        } else if let Some(items) = initial_request_items.take() {
            (
                build_streaming_request_items(config, items),
                "initial_items",
            )
        } else if stateless_history {
            (
                build_streaming_request_items(config, history_items.clone()),
                "stateless_history",
            )
        } else {
            (build_streaming_request(config, prompt), "prompt")
        };

        let mut collector = ToolCallCollector::default();
        let stream_result = stream_openresponses_request(OpenResponsesStreamRequest {
            http,
            config,
            workspace_root: continuities.workspace_root(),
            session_id,
            payload,
            request_index,
            request_kind,
            seq,
            sink,
            collector: &mut collector,
        })
        .await;
        request_index = request_index.saturating_add(1);
        if let Err(reason) = stream_result {
            return OpenResponsesLoopOutcome {
                reason,
                last_response_id: previous_response_id,
            };
        }

        if let Some(id) = collector.response_id.clone() {
            previous_response_id = Some(id);
        }

        let tool_calls = collector.drain_function_calls();
        if tool_calls.is_empty() {
            return OpenResponsesLoopOutcome {
                reason: "completed".to_string(),
                last_response_id: previous_response_id,
            };
        }
        if previous_response_id.is_none() && !stateless_history {
            return OpenResponsesLoopOutcome {
                reason: "provider_error".to_string(),
                last_response_id: previous_response_id,
            };
        }

        let mut tool_outputs = Vec::new();
        if stateless_history {
            for call in &tool_calls {
                history_items.push(function_call_item_from_call(call, true));
            }
        }
        for call in tool_calls {
            if tool_call_count >= DEFAULT_MAX_TOOL_CALLS {
                return OpenResponsesLoopOutcome {
                    reason: "max_tool_calls_exceeded".to_string(),
                    last_response_id: previous_response_id,
                };
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
            let output_value = if !tool_choice_enforcement.allows_function(&invocation.name) {
                let error = format!(
                    "tool call rejected by tool_choice (call_id={}, name={})",
                    call.call_id, call.name
                );
                let tool_events = rejected_tool_invocation_events(
                    session_id,
                    seq,
                    &invocation,
                    &call.call_id,
                    &error,
                );
                let output_value = tool_events_to_function_call_output(&call.name, &tool_events);
                sink.emit_all(tool_events).await;
                output_value
            } else if requires_workspace_lock(&invocation.name) {
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

pub(super) struct OpenResponsesStreamRequest<'a> {
    pub(super) http: &'a reqwest::Client,
    pub(super) config: &'a OpenResponsesConfig,
    pub(super) workspace_root: &'a Path,
    pub(super) session_id: &'a str,
    pub(super) payload: CreateResponsePayload,
    pub(super) request_index: u64,
    pub(super) request_kind: &'a str,
    pub(super) seq: &'a mut u64,
    pub(super) sink: EventSink<'a>,
    pub(super) collector: &'a mut ToolCallCollector,
}

pub(super) async fn stream_openresponses_request<'a>(
    req: OpenResponsesStreamRequest<'a>,
) -> Result<(), String> {
    let validation = validation_options_for_stream(req.config);

    if !req.payload.errors().is_empty() {
        req.sink
            .emit(Event {
                id: Uuid::new_v4().to_string(),
                session_id: req.session_id.to_string(),
                timestamp_ms: super::now_ms(),
                seq: *req.seq,
                kind: rip_kernel::EventKind::ProviderEvent {
                    provider: "openresponses".to_string(),
                    status: rip_kernel::ProviderEventStatus::Event,
                    event_name: None,
                    data: None,
                    raw: Some(req.payload.body().to_string()),
                    errors: req.payload.errors().to_vec(),
                    response_errors: Vec::new(),
                },
            })
            .await;
        *req.seq += 1;
        return Err("invalid_request".to_string());
    }

    let request_dump_cfg = crate::openresponses_observability::request_dump_config_from_env();
    if let Some(event) = crate::openresponses_observability::maybe_dump_openresponses_request(
        request_dump_cfg,
        crate::openresponses_observability::OpenResponsesRequestDumpInput {
            workspace_root: req.workspace_root,
            session_id: req.session_id,
            timestamp_ms: super::now_ms(),
            seq: *req.seq,
            endpoint: &req.config.endpoint,
            request_index: req.request_index,
            kind: req.request_kind,
            body: req.payload.body(),
        },
    )? {
        req.sink.emit(event).await;
        *req.seq += 1;
    }

    let model = req
        .payload
        .body()
        .get("model")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    req.sink
        .emit(Event {
            id: Uuid::new_v4().to_string(),
            session_id: req.session_id.to_string(),
            timestamp_ms: super::now_ms(),
            seq: *req.seq,
            kind: EventKind::OpenResponsesRequestStarted {
                endpoint: req.config.endpoint.clone(),
                model,
                request_index: req.request_index,
                kind: req.request_kind.to_string(),
            },
        })
        .await;
    *req.seq += 1;

    let mut request = req.http.post(&req.config.endpoint).json(req.payload.body());
    if let Some(key) = req.config.api_key.as_deref() {
        request = request.bearer_auth(key);
    }
    for (name, value) in &req.config.headers {
        request = request.header(name, value);
    }

    let response = match request.send().await {
        Ok(response) => response,
        Err(err) => {
            let mut pipe =
                OpenResponsesSsePipe::new(req.session_id, req.seq, req.sink, None, validation);
            pipe.emit_transport_error(err.to_string()).await;
            return Err("provider_error".to_string());
        }
    };

    let status = response.status();
    let request_id = response
        .headers()
        .get("x-request-id")
        .or_else(|| response.headers().get("x-openai-request-id"))
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    req.sink
        .emit(Event {
            id: Uuid::new_v4().to_string(),
            session_id: req.session_id.to_string(),
            timestamp_ms: super::now_ms(),
            seq: *req.seq,
            kind: EventKind::OpenResponsesResponseHeaders {
                request_index: req.request_index,
                status: status.as_u16(),
                request_id,
                content_type,
            },
        })
        .await;
    *req.seq += 1;

    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let mut pipe =
            OpenResponsesSsePipe::new(req.session_id, req.seq, req.sink, None, validation);
        pipe.emit_transport_error(format!("provider http error: {status}: {body}"))
            .await;
        return Err("provider_error".to_string());
    }

    let mut utf8_buf = Vec::new();
    let mut stream = response.bytes_stream();
    let Some(first) = stream.next().await else {
        let mut pipe =
            OpenResponsesSsePipe::new(req.session_id, req.seq, req.sink, None, validation);
        pipe.emit_transport_error("provider stream ended before first byte".to_string())
            .await;
        return Err("provider_error".to_string());
    };
    let first_chunk = match first {
        Ok(chunk) => chunk,
        Err(err) => {
            let mut pipe =
                OpenResponsesSsePipe::new(req.session_id, req.seq, req.sink, None, validation);
            pipe.emit_transport_error(err.to_string()).await;
            return Err("provider_error".to_string());
        }
    };

    req.sink
        .emit(Event {
            id: Uuid::new_v4().to_string(),
            session_id: req.session_id.to_string(),
            timestamp_ms: super::now_ms(),
            seq: *req.seq,
            kind: EventKind::OpenResponsesResponseFirstByte {
                request_index: req.request_index,
            },
        })
        .await;
    *req.seq += 1;

    let mut pipe = OpenResponsesSsePipe::new(
        req.session_id,
        req.seq,
        req.sink,
        Some(req.collector),
        validation,
    );
    let mut saw_done = pipe.push_bytes(&mut utf8_buf, &first_chunk).await;
    while !saw_done {
        let Some(next) = stream.next().await else {
            break;
        };
        let chunk = match next {
            Ok(chunk) => chunk,
            Err(err) => {
                pipe.emit_transport_error(err.to_string()).await;
                return Err("provider_error".to_string());
            }
        };
        saw_done = pipe.push_bytes(&mut utf8_buf, &chunk).await;
    }

    if !saw_done {
        let _ = pipe.finish().await;
    }

    Ok(())
}

pub(super) fn validation_options_for_stream(config: &OpenResponsesConfig) -> ValidationOptions {
    resolve_openresponses_compat_profile(
        config.provider_id.as_deref(),
        &config.endpoint,
        config.model.as_deref(),
    )
    .validation_options(config.stateless_history)
}
