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
    let sink = EventSink::new(&sender, &buffer, &log);
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
    let sink = EventSink::new(&sender, &buffer, &log);
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
    let sink = EventSink::new(&sender, &buffer, &log);
    let config = OpenResponsesConfig {
        endpoint: "http://example.test/v1/responses".to_string(),
        api_key: None,
        model: None,
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        followup_user_message: None,
        stateless_history: false,
        parallel_tool_calls: false,
    };
    let payload = CreateResponsePayload::new(serde_json::json!({"input": {}}));
    let mut seq = 0;
    let mut collector = ToolCallCollector::default();
    let http = reqwest::Client::new();
    let err = stream_openresponses_request(OpenResponsesStreamRequest {
        http: &http,
        config: &config,
        workspace_root: dir.path(),
        session_id: "s1",
        payload,
        request_index: 0,
        request_kind: "test",
        seq: &mut seq,
        sink,
        collector: &mut collector,
    })
    .await
    .unwrap_err();
    assert_eq!(err, "invalid_request");
    let events = buffer.lock().await;
    assert_eq!(events.len(), 1);
    match &events[0].kind {
        EventKind::ProviderEvent { status, errors, .. } => {
            assert_eq!(*status, rip_kernel::ProviderEventStatus::Event);
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
    let sink = EventSink::new(&sender, &buffer, &log);
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
    let sink = EventSink::new(&sender, &buffer, &log);
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
    let sink = EventSink::new(&sender, &buffer, &log);
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
    let sink = EventSink::new(&sender, &buffer, &log);
    let config = OpenResponsesConfig {
        endpoint: "http://127.0.0.1:0/v1/responses".to_string(),
        api_key: None,
        model: Some("fixture-model".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        followup_user_message: None,
        stateless_history: false,
        parallel_tool_calls: false,
    };
    let payload = build_streaming_request(&config, "hi");
    assert!(payload.errors().is_empty());
    let mut seq = 0;
    let mut collector = ToolCallCollector::default();
    let http = reqwest::Client::new();
    let err = stream_openresponses_request(OpenResponsesStreamRequest {
        http: &http,
        config: &config,
        workspace_root: dir.path(),
        session_id: "s1",
        payload,
        request_index: 0,
        request_kind: "test",
        seq: &mut seq,
        sink,
        collector: &mut collector,
    })
    .await
    .unwrap_err();
    assert_eq!(err, "provider_error");
    let events = buffer.lock().await;
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[0].kind,
        EventKind::OpenResponsesRequestStarted { .. }
    ));
    match &events[1].kind {
        EventKind::ProviderEvent { status, .. } => {
            assert_eq!(*status, rip_kernel::ProviderEventStatus::Event);
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
    let sink = EventSink::new(&sender, &buffer, &log);
    let config = OpenResponsesConfig {
        endpoint: format!("http://{addr}/v1/responses"),
        api_key: None,
        model: Some("fixture-model".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        followup_user_message: None,
        stateless_history: false,
        parallel_tool_calls: false,
    };
    let payload = build_streaming_request(&config, "hi");
    assert!(payload.errors().is_empty());
    let mut seq = 0;
    let mut collector = ToolCallCollector::default();
    let http = reqwest::Client::new();
    let err = stream_openresponses_request(OpenResponsesStreamRequest {
        http: &http,
        config: &config,
        workspace_root: dir.path(),
        session_id: "s1",
        payload,
        request_index: 0,
        request_kind: "test",
        seq: &mut seq,
        sink,
        collector: &mut collector,
    })
    .await
    .unwrap_err();
    assert_eq!(err, "provider_error");
    let events = buffer.lock().await;
    assert_eq!(events.len(), 3);
    assert!(matches!(
        events[0].kind,
        EventKind::OpenResponsesRequestStarted { .. }
    ));
    match &events[1].kind {
        EventKind::OpenResponsesResponseHeaders { status, .. } => {
            assert_eq!(*status, 400);
        }
        _ => panic!("expected openresponses_response_headers"),
    }
    match &events[2].kind {
        EventKind::ProviderEvent { status, errors, .. } => {
            assert_eq!(*status, rip_kernel::ProviderEventStatus::Event);
            assert!(errors
                .iter()
                .any(|error| error.contains("provider http error")));
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
    let sink = EventSink::new(&sender, &buffer, &log);

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
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        followup_user_message: None,
        stateless_history: true,
        parallel_tool_calls: false,
    };
    let mut seq = 0;
    let http = reqwest::Client::new();
    let outcome = run_openresponses_agent_loop(OpenResponsesRunContext {
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
    assert_eq!(outcome.reason, "completed");
    assert!(outcome.last_response_id.is_none());
    let events = buffer.lock().await;
    assert!(events
        .iter()
        .any(|event| matches!(event.kind, EventKind::ToolStarted { .. })));
}

#[tokio::test]
async fn run_openresponses_agent_loop_rejects_tools_when_tool_choice_none() {
    use axum::extract::{Json, State};
    use axum::http::header::CONTENT_TYPE;
    use axum::routing::post;
    use axum::Router as AxumRouter;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::TcpListener;

    #[derive(Clone)]
    struct ProviderState {
        counter: Arc<AtomicUsize>,
        requests: Arc<Mutex<Vec<Value>>>,
        tool_sse: Arc<String>,
        output_sse: Arc<String>,
    }

    async fn handler(
        State(state): State<ProviderState>,
        Json(body): Json<Value>,
    ) -> impl axum::response::IntoResponse {
        state.requests.lock().await.push(body);
        let idx = state.counter.fetch_add(1, Ordering::SeqCst);
        let sse = if idx == 0 {
            state.tool_sse.as_str()
        } else {
            state.output_sse.as_str()
        };
        ([(CONTENT_TYPE, "text/event-stream")], sse.to_string())
    }

    let tool_sse = "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"noop\",\"arguments\":\"{}\"}}\n\n\
data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_1\",\"name\":\"noop\",\"arguments\":\"{}\"}}\n\n\
data: [DONE]\n\n";
    let output_sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n\
data: [DONE]\n\n";

    let state = ProviderState {
        counter: Arc::new(AtomicUsize::new(0)),
        requests: Arc::new(Mutex::new(Vec::new())),
        tool_sse: Arc::new(tool_sse.to_string()),
        output_sse: Arc::new(output_sse.to_string()),
    };
    let provider_app = AxumRouter::new()
        .route("/v1/responses", post(handler))
        .with_state(state.clone());

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, provider_app).await.expect("serve");
    });

    let dir = tempdir().expect("tmp");
    let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let (sender, _) = broadcast::channel(8);
    let sink = EventSink::new(&sender, &buffer, &log);

    let executed = Arc::new(AtomicUsize::new(0));
    let registry = Arc::new(rip_tools::ToolRegistry::default());
    registry.register(
        "noop",
        Arc::new({
            let executed = executed.clone();
            move |_invocation| {
                let executed = executed.clone();
                Box::pin(async move {
                    executed.fetch_add(1, Ordering::SeqCst);
                    rip_tools::ToolOutput::success(vec![])
                })
            }
        }),
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
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::none(),
        followup_user_message: None,
        stateless_history: true,
        parallel_tool_calls: false,
    };
    let mut seq = 0;
    let http = reqwest::Client::new();
    let outcome = run_openresponses_agent_loop(OpenResponsesRunContext {
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

    assert_eq!(outcome.reason, "completed");
    assert_eq!(executed.load(Ordering::SeqCst), 0);

    let requests = state.requests.lock().await;
    assert_eq!(requests.len(), 2);
    let input = requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .expect("input items");
    let output_item = input
        .iter()
        .find(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                && item.get("call_id").and_then(|value| value.as_str()) == Some("call_1")
        })
        .expect("function_call_output item");
    let output = output_item
        .get("output")
        .and_then(|value| value.as_str())
        .expect("output string");
    let output_value: Value = serde_json::from_str(output).expect("output json");
    assert_eq!(
        output_value.get("ok").and_then(|value| value.as_bool()),
        Some(false)
    );
    assert!(output_value
        .get("error")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .contains("rejected"));
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
    let sink = EventSink::new(&sender, &buffer, &log);

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
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        followup_user_message: None,
        stateless_history: false,
        parallel_tool_calls: false,
    };
    let mut seq = 0;
    let http = reqwest::Client::new();
    let outcome = run_openresponses_agent_loop(OpenResponsesRunContext {
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
    assert_eq!(outcome.reason, "provider_error");
    assert!(outcome.last_response_id.is_none());
}

#[tokio::test]
async fn run_openresponses_agent_loop_enforces_allowed_tools() {
    use axum::extract::{Json, State};
    use axum::http::header::CONTENT_TYPE;
    use axum::routing::post;
    use axum::Router as AxumRouter;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::TcpListener;

    #[derive(Clone)]
    struct ProviderState {
        counter: Arc<AtomicUsize>,
        requests: Arc<Mutex<Vec<Value>>>,
        tool_sse: Arc<String>,
        output_sse: Arc<String>,
    }

    async fn handler(
        State(state): State<ProviderState>,
        Json(body): Json<Value>,
    ) -> impl axum::response::IntoResponse {
        state.requests.lock().await.push(body);
        let idx = state.counter.fetch_add(1, Ordering::SeqCst);
        let sse = if idx == 0 {
            state.tool_sse.as_str()
        } else {
            state.output_sse.as_str()
        };
        ([(CONTENT_TYPE, "text/event-stream")], sse.to_string())
    }

    let tool_sse = "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_a\",\"name\":\"noop_allowed\",\"arguments\":\"{}\"}}\n\n\
data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_a\",\"name\":\"noop_allowed\",\"arguments\":\"{}\"}}\n\n\
data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_b\",\"name\":\"noop_disallowed\",\"arguments\":\"{}\"}}\n\n\
data: {\"type\":\"response.output_item.done\",\"output_index\":1,\"item\":{\"type\":\"function_call\",\"call_id\":\"call_b\",\"name\":\"noop_disallowed\",\"arguments\":\"{}\"}}\n\n\
data: [DONE]\n\n";
    let output_sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n\
data: [DONE]\n\n";

    let state = ProviderState {
        counter: Arc::new(AtomicUsize::new(0)),
        requests: Arc::new(Mutex::new(Vec::new())),
        tool_sse: Arc::new(tool_sse.to_string()),
        output_sse: Arc::new(output_sse.to_string()),
    };
    let provider_app = AxumRouter::new()
        .route("/v1/responses", post(handler))
        .with_state(state.clone());

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, provider_app).await.expect("serve");
    });

    let dir = tempdir().expect("tmp");
    let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let (sender, _) = broadcast::channel(8);
    let sink = EventSink::new(&sender, &buffer, &log);

    let allowed_executed = Arc::new(AtomicUsize::new(0));
    let disallowed_executed = Arc::new(AtomicUsize::new(0));
    let registry = Arc::new(rip_tools::ToolRegistry::default());
    registry.register(
        "noop_allowed",
        Arc::new({
            let allowed_executed = allowed_executed.clone();
            move |_invocation| {
                let allowed_executed = allowed_executed.clone();
                Box::pin(async move {
                    allowed_executed.fetch_add(1, Ordering::SeqCst);
                    rip_tools::ToolOutput::success(vec![])
                })
            }
        }),
    );
    registry.register(
        "noop_disallowed",
        Arc::new({
            let disallowed_executed = disallowed_executed.clone();
            move |_invocation| {
                let disallowed_executed = disallowed_executed.clone();
                Box::pin(async move {
                    disallowed_executed.fetch_add(1, Ordering::SeqCst);
                    rip_tools::ToolOutput::success(vec![])
                })
            }
        }),
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

    let tool_choice = ToolChoiceParam::new(serde_json::json!({
        "type": "allowed_tools",
        "tools": [{ "type": "function", "name": "noop_allowed" }],
        "mode": "auto",
    }));
    assert!(tool_choice.errors().is_empty());
    let config = OpenResponsesConfig {
        endpoint: format!("http://{addr}/v1/responses"),
        api_key: None,
        model: Some("fixture-model".to_string()),
        headers: Vec::new(),
        tool_choice,
        followup_user_message: None,
        stateless_history: true,
        parallel_tool_calls: false,
    };
    let mut seq = 0;
    let http = reqwest::Client::new();
    let outcome = run_openresponses_agent_loop(OpenResponsesRunContext {
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

    assert_eq!(outcome.reason, "completed");
    assert_eq!(allowed_executed.load(Ordering::SeqCst), 1);
    assert_eq!(disallowed_executed.load(Ordering::SeqCst), 0);

    let requests = state.requests.lock().await;
    assert_eq!(requests.len(), 2);
    let input = requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .expect("input items");
    let output_item = input
        .iter()
        .find(|item| {
            item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
                && item.get("call_id").and_then(|value| value.as_str()) == Some("call_b")
        })
        .expect("function_call_output call_b item");
    let output = output_item
        .get("output")
        .and_then(|value| value.as_str())
        .expect("output string");
    let output_value: Value = serde_json::from_str(output).expect("output json");
    assert_eq!(
        output_value.get("ok").and_then(|value| value.as_bool()),
        Some(false)
    );
    assert!(output_value
        .get("error")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .contains("rejected"));
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
    let sink = EventSink::new(&sender, &buffer, &log);

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
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        followup_user_message: None,
        stateless_history: false,
        parallel_tool_calls: false,
    };
    let mut seq = 0;
    let http = reqwest::Client::new();
    let outcome = run_openresponses_agent_loop(OpenResponsesRunContext {
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
    assert_eq!(outcome.reason, "completed");
    assert_eq!(outcome.last_response_id.as_deref(), Some("resp_1"));
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
