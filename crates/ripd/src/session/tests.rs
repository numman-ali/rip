use super::*;
use crate::provider_openresponses::{
    OpenResponsesApproximateLocation, OpenResponsesInclude, OpenResponsesReasoningConfig,
    OpenResponsesWebSearchConfig, ReasoningEffort, ReasoningSummary, SearchContextSize,
    DEFAULT_OPENROUTER_MODEL,
};
use crate::CompactionCheckpointCumulativeV1Request;
use rip_kernel::ProviderEventStatus;
use rip_provider_openresponses::ToolChoiceParam;
use tempfile::tempdir;

fn continuity_store_for_session(
    dir: &tempfile::TempDir,
) -> (Arc<EventLog>, ContinuityStore, PathBuf) {
    let data_dir = dir.path().join("data");
    let workspace_root = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_root).expect("workspace");
    let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
    let store =
        ContinuityStore::new(data_dir.clone(), workspace_root, event_log.clone()).expect("store");
    (event_log, store, data_dir)
}

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
fn compile_context_bundle_for_run_uses_recent_messages_when_no_checkpoint_exists() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = continuity_store_for_session(&dir);
    let snapshot_dir = dir.path().join("snapshots");
    std::fs::create_dir_all(&snapshot_dir).expect("snapshots");

    let continuity_id = store.ensure_default().expect("ensure");
    let _m1 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "hello".to_string(),
        )
        .expect("append");
    let m2 = store
        .append_message(
            &continuity_id,
            "assistant".to_string(),
            "cli".to_string(),
            "world".to_string(),
        )
        .expect("append");

    let outcome = context_compile::compile_context_bundle_for_run(
        &store,
        &event_log,
        &snapshot_dir,
        &ContinuityRunLink {
            continuity_id: continuity_id.clone(),
            message_id: m2,
            actor_id: "alice".to_string(),
            origin: "cli".to_string(),
        },
        "run-1",
    )
    .expect("compile");

    assert_eq!(outcome.decision.compiler_strategy, "recent_messages_v1");
    assert!(outcome.decision.compaction_checkpoint.is_none());
    assert_eq!(
        outcome
            .decision
            .reason
            .as_ref()
            .and_then(|reason| reason.get("cause"))
            .and_then(|value| value.as_str()),
        Some("no_compaction_checkpoint")
    );
    assert_eq!(outcome.compiled.items.len(), 2);
    let values = outcome
        .compiled
        .items
        .iter()
        .map(|item| item.value().clone())
        .collect::<Vec<_>>();
    assert!(values
        .iter()
        .all(|value| { value.get("type").and_then(|field| field.as_str()) == Some("message") }));
    assert!(values.iter().any(|value| {
        value
            .get("content")
            .and_then(|field| field.as_str())
            .unwrap_or("")
            .contains("hello")
    }));
    assert!(outcome
        .compiled
        .items
        .iter()
        .all(|item| item.errors().is_empty()));
}

#[test]
fn compile_context_bundle_for_run_uses_summary_and_hierarchy_strategies() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = continuity_store_for_session(&dir);
    let snapshot_dir = dir.path().join("snapshots");
    std::fs::create_dir_all(&snapshot_dir).expect("snapshots");

    let continuity_id = store.ensure_default().expect("ensure");
    let m1 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m1".to_string(),
        )
        .expect("append");
    let m2 = store
        .append_message(
            &continuity_id,
            "assistant".to_string(),
            "cli".to_string(),
            "m2".to_string(),
        )
        .expect("append");
    let _m3 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m3".to_string(),
        )
        .expect("append");
    let m4 = store
        .append_message(
            &continuity_id,
            "assistant".to_string(),
            "cli".to_string(),
            "m4".to_string(),
        )
        .expect("append");

    let (_checkpoint_1, summary_1, to_seq_1, _to_message_1, _cut_rule_1) = store
        .compaction_checkpoint_cumulative_v1(
            &continuity_id,
            CompactionCheckpointCumulativeV1Request {
                summary_markdown: Some("summary one".to_string()),
                summary_artifact_id: None,
                to_message_id: Some(m1.clone()),
                to_seq: None,
                stride_messages: None,
                actor_id: "alice".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("checkpoint one");

    let summary_outcome = context_compile::compile_context_bundle_for_run(
        &store,
        &event_log,
        &snapshot_dir,
        &ContinuityRunLink {
            continuity_id: continuity_id.clone(),
            message_id: m4.clone(),
            actor_id: "alice".to_string(),
            origin: "cli".to_string(),
        },
        "run-summary",
    )
    .expect("compile summaries");

    assert_eq!(
        summary_outcome.decision.compiler_strategy,
        "summaries_recent_messages_v1"
    );
    assert_eq!(
        summary_outcome
            .decision
            .compaction_checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.to_seq),
        Some(to_seq_1)
    );
    let summary_values = summary_outcome
        .compiled
        .items
        .iter()
        .map(|item| item.value().clone())
        .collect::<Vec<_>>();
    assert!(summary_values.iter().any(|value| {
        value.get("role").and_then(|field| field.as_str()) == Some("system")
            && value
                .get("content")
                .and_then(|field| field.as_str())
                .unwrap_or("")
                .contains("Compaction summary (earlier context)")
    }));
    assert!(summary_values.iter().any(|value| {
        value
            .get("content")
            .and_then(|field| field.as_str())
            .unwrap_or("")
            .contains("summary one")
    }));

    let (_checkpoint_2, summary_2, to_seq_2, _to_message_2, _cut_rule_2) = store
        .compaction_checkpoint_cumulative_v1(
            &continuity_id,
            CompactionCheckpointCumulativeV1Request {
                summary_markdown: Some("summary two".to_string()),
                summary_artifact_id: None,
                to_message_id: Some(m2),
                to_seq: None,
                stride_messages: None,
                actor_id: "alice".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("checkpoint two");

    let hierarchical_outcome = context_compile::compile_context_bundle_for_run(
        &store,
        &event_log,
        &snapshot_dir,
        &ContinuityRunLink {
            continuity_id,
            message_id: m4,
            actor_id: "alice".to_string(),
            origin: "cli".to_string(),
        },
        "run-hierarchical",
    )
    .expect("compile hierarchy");

    assert_eq!(
        hierarchical_outcome.decision.compiler_strategy,
        "hierarchical_summaries_recent_messages_v1"
    );
    assert_eq!(
        hierarchical_outcome.decision.compaction_checkpoints.len(),
        2
    );
    assert_eq!(
        hierarchical_outcome
            .decision
            .reason
            .as_ref()
            .and_then(|reason| reason.get("cause"))
            .and_then(|value| value.as_str()),
        Some("compaction_checkpoint_hierarchy")
    );
    assert_eq!(
        hierarchical_outcome
            .decision
            .reason
            .as_ref()
            .and_then(|reason| reason.get("levels"))
            .and_then(|value| value.as_u64()),
        Some(2)
    );
    let hierarchical_values = hierarchical_outcome
        .compiled
        .items
        .iter()
        .map(|item| item.value().clone())
        .collect::<Vec<_>>();
    assert!(
        hierarchical_values
            .iter()
            .filter(|value| {
                value.get("role").and_then(|field| field.as_str()) == Some("system")
            })
            .count()
            >= 2
    );
    assert!(hierarchical_values.iter().any(|value| {
        value
            .get("content")
            .and_then(|field| field.as_str())
            .unwrap_or("")
            .contains(&summary_1)
            || value
                .get("content")
                .and_then(|field| field.as_str())
                .unwrap_or("")
                .contains("summary one")
    }));
    assert!(hierarchical_values.iter().any(|value| {
        value
            .get("content")
            .and_then(|field| field.as_str())
            .unwrap_or("")
            .contains(&summary_2)
            || value
                .get("content")
                .and_then(|field| field.as_str())
                .unwrap_or("")
                .contains("summary two")
    }));
    assert_eq!(
        hierarchical_outcome
            .decision
            .compaction_checkpoints
            .iter()
            .map(|checkpoint| checkpoint.to_seq)
            .collect::<Vec<_>>(),
        vec![to_seq_1, to_seq_2]
    );
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
async fn openresponses_pipe_openrouter_compat_does_not_emit_schema_errors_for_reasoning_text() {
    let dir = tempdir().expect("tmp");
    let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let (sender, _) = broadcast::channel(8);
    let mut seq = 0;
    let sink = EventSink::new(&sender, &buffer, &log);
    let mut pipe = OpenResponsesSsePipe::new(
        "s1",
        &mut seq,
        sink,
        None,
        ValidationOptions::compat_openrouter(),
    );
    let payload = "event: response.created\n\
                  data: {\"type\":\"response.created\",\"sequence_number\":1,\"response\":{\"background\":false,\"completed_at\":null,\"created_at\":1776635696,\"error\":null,\"frequency_penalty\":0,\"id\":\"resp_1\",\"incomplete_details\":null,\"instructions\":null,\"max_output_tokens\":null,\"max_tool_calls\":32,\"metadata\":{},\"model\":\"nvidia/nemotron-3-nano-30b-a3b:free\",\"object\":\"response\",\"output\":[],\"parallel_tool_calls\":false,\"presence_penalty\":0,\"previous_response_id\":null,\"prompt_cache_key\":null,\"reasoning\":null,\"safety_identifier\":null,\"service_tier\":\"auto\",\"status\":\"in_progress\",\"store\":false,\"temperature\":1,\"text\":{\"format\":{\"type\":\"text\"}},\"tool_choice\":\"auto\",\"tools\":[],\"top_logprobs\":0,\"top_p\":1,\"truncation\":\"disabled\",\"usage\":null}}\n\n\
                  event: response.reasoning_text.delta\n\
                  data: {\"type\":\"response.reasoning_text.delta\",\"sequence_number\":2,\"item_id\":\"rs_tmp_1\",\"output_index\":0,\"content_index\":0,\"delta\":\"We\"}\n\n\
                  event: response.output_text.delta\n\
                  data: {\"type\":\"response.output_text.delta\",\"sequence_number\":3,\"item_id\":\"msg_1\",\"output_index\":1,\"content_index\":0,\"delta\":\"Hello! 👋\",\"logprobs\":[]}\n\n";
    let saw_done = pipe.push_sse_str(payload).await;
    assert!(!saw_done);

    let events = buffer.lock().await;
    let provider_events: Vec<_> = events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::ProviderEvent {
                errors,
                response_errors,
                ..
            } => Some((errors, response_errors)),
            _ => None,
        })
        .collect();
    assert_eq!(provider_events.len(), 3);
    for (errors, response_errors) in provider_events {
        assert!(errors.is_empty(), "unexpected provider errors: {errors:?}");
        assert!(
            response_errors.is_empty(),
            "unexpected response errors: {response_errors:?}"
        );
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
        provider_id: None,
        endpoint: "http://example.test/v1/responses".to_string(),
        api_key: None,
        model: None,
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        include: Vec::new(),
        reasoning: None,
        web_search: None,
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
fn validation_options_for_stream_uses_openrouter_compat_profile() {
    let config = OpenResponsesConfig {
        provider_id: None,
        endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
        api_key: None,
        model: Some("nvidia/nemotron-3-nano-30b-a3b:free".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        include: Vec::new(),
        reasoning: None,
        web_search: None,
        followup_user_message: None,
        stateless_history: false,
        parallel_tool_calls: false,
    };

    assert_eq!(
        super::openresponses::validation_options_for_stream(&config),
        ValidationOptions::compat_openrouter()
    );
}

#[test]
fn validation_options_for_stream_prefers_provider_id_over_endpoint_heuristic() {
    let config = OpenResponsesConfig {
        provider_id: Some("openrouter".to_string()),
        endpoint: "http://127.0.0.1:4010/v1/responses".to_string(),
        api_key: None,
        model: Some("nvidia/nemotron-3-nano-30b-a3b:free".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        include: Vec::new(),
        reasoning: None,
        web_search: None,
        followup_user_message: None,
        stateless_history: false,
        parallel_tool_calls: false,
    };

    assert_eq!(
        super::openresponses::validation_options_for_stream(&config),
        ValidationOptions::compat_openrouter()
    );
}

#[test]
fn validation_options_for_stream_adds_missing_item_ids_for_stateless_history() {
    let config = OpenResponsesConfig {
        provider_id: None,
        endpoint: "https://api.openai.com/v1/responses".to_string(),
        api_key: None,
        model: Some("gpt-5".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        include: Vec::new(),
        reasoning: None,
        web_search: None,
        followup_user_message: None,
        stateless_history: true,
        parallel_tool_calls: false,
    };

    assert_eq!(
        super::openresponses::validation_options_for_stream(&config),
        ValidationOptions::compat_missing_item_ids().with_response_web_search_tools()
    );
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
        provider_id: None,
        endpoint: "http://127.0.0.1:0/v1/responses".to_string(),
        api_key: None,
        model: Some("fixture-model".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        include: Vec::new(),
        reasoning: None,
        web_search: None,
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
        provider_id: None,
        endpoint: format!("http://{addr}/v1/responses"),
        api_key: None,
        model: Some("fixture-model".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        include: Vec::new(),
        reasoning: None,
        web_search: None,
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
async fn stream_openresponses_request_sends_auth_headers_and_request_controls() {
    use axum::extract::{Json, State};
    use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
    use axum::routing::post;
    use axum::{response::IntoResponse, Router as AxumRouter};
    use serde_json::Value;
    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::net::TcpListener;

    #[derive(Clone, Default)]
    struct ProviderCapture {
        auth: Arc<StdMutex<Option<String>>>,
        content_type: Arc<StdMutex<Option<String>>>,
        referer: Arc<StdMutex<Option<String>>>,
        extra: Arc<StdMutex<Option<String>>>,
        body: Arc<StdMutex<Option<Value>>>,
    }

    let capture = ProviderCapture::default();
    let provider_app = AxumRouter::new()
        .route(
            "/v1/responses",
            post(
                |State(capture): State<ProviderCapture>,
                 headers: axum::http::HeaderMap,
                 Json(body): Json<Value>| async move {
                    *capture.auth.lock().expect("auth") = headers
                        .get(AUTHORIZATION)
                        .and_then(|value| value.to_str().ok())
                        .map(|value| value.to_string());
                    *capture.content_type.lock().expect("content_type") = headers
                        .get(CONTENT_TYPE)
                        .and_then(|value| value.to_str().ok())
                        .map(|value| value.to_string());
                    *capture.referer.lock().expect("referer") = headers
                        .get("http-referer")
                        .and_then(|value| value.to_str().ok())
                        .map(|value| value.to_string());
                    *capture.extra.lock().expect("extra") = headers
                        .get("x-rip-test")
                        .and_then(|value| value.to_str().ok())
                        .map(|value| value.to_string());
                    *capture.body.lock().expect("body") = Some(body);

                    let sse =
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n\
data: [DONE]\n\n";
                    ([(CONTENT_TYPE, "text/event-stream")], sse).into_response()
                },
            ),
        )
        .with_state(capture.clone());

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
        provider_id: Some("openrouter".to_string()),
        endpoint: format!("http://{addr}/v1/responses"),
        api_key: Some("sk-test-openrouter".to_string()),
        model: None,
        headers: vec![
            ("HTTP-Referer".to_string(), "https://rip.test".to_string()),
            ("X-RIP-Test".to_string(), "alpha".to_string()),
        ],
        tool_choice: ToolChoiceParam::required(),
        include: Vec::new(),
        reasoning: Some(OpenResponsesReasoningConfig {
            effort: Some(ReasoningEffort::High),
            summary: Some(ReasoningSummary::Detailed),
        }),
        web_search: None,
        followup_user_message: None,
        stateless_history: false,
        parallel_tool_calls: true,
    };
    let payload = build_streaming_request(&config, "hi");
    assert!(payload.errors().is_empty());

    let mut seq = 0;
    let mut collector = ToolCallCollector::default();
    let http = reqwest::Client::new();
    let result = stream_openresponses_request(OpenResponsesStreamRequest {
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
    .await;
    assert_eq!(result, Ok(()));

    assert_eq!(
        capture.auth.lock().expect("auth").clone().as_deref(),
        Some("Bearer sk-test-openrouter")
    );
    let content_type = capture
        .content_type
        .lock()
        .expect("content_type")
        .clone()
        .unwrap_or_default();
    assert!(
        content_type.starts_with("application/json"),
        "unexpected content-type: {content_type}"
    );
    assert_eq!(
        capture.referer.lock().expect("referer").clone().as_deref(),
        Some("https://rip.test")
    );
    assert_eq!(
        capture.extra.lock().expect("extra").clone().as_deref(),
        Some("alpha")
    );

    let body = capture
        .body
        .lock()
        .expect("body")
        .clone()
        .expect("captured body");
    assert_eq!(
        body.get("model").and_then(|value| value.as_str()),
        Some(DEFAULT_OPENROUTER_MODEL)
    );
    assert_eq!(
        body.get("stream").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        body.get("parallel_tool_calls")
            .and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        body.get("max_tool_calls").and_then(|value| value.as_u64()),
        Some(DEFAULT_MAX_TOOL_CALLS)
    );
    assert_eq!(
        body.get("tool_choice").and_then(|value| value.as_str()),
        Some("required")
    );
    assert_eq!(
        body.get("reasoning")
            .and_then(|value| value.get("effort"))
            .and_then(|value| value.as_str()),
        Some("high")
    );
    assert_eq!(
        body.get("reasoning")
            .and_then(|value| value.get("summary"))
            .and_then(|value| value.as_str()),
        Some("detailed")
    );
    assert!(body
        .get("tools")
        .and_then(|value| value.as_array())
        .is_some());
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
        provider_id: None,
        endpoint: format!("http://{addr}/v1/responses"),
        api_key: None,
        model: Some("fixture-model".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        include: Vec::new(),
        reasoning: None,
        web_search: None,
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
async fn run_openresponses_agent_loop_emits_compat_warning_and_coerces_openrouter_to_stateless() {
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
        provider_id: Some("openrouter".to_string()),
        endpoint: format!("http://{addr}/v1/responses"),
        api_key: None,
        model: Some("fixture-model".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        include: vec![
            OpenResponsesInclude::ReasoningEncryptedContent,
            OpenResponsesInclude::MessageOutputTextLogprobs,
        ],
        reasoning: None,
        web_search: None,
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
    assert!(outcome.last_response_id.is_none());

    let events = buffer.lock().await;
    assert!(events.iter().any(|event| match &event.kind {
        EventKind::ProviderEvent {
            status,
            event_name,
            data,
            errors,
            response_errors,
            ..
        } => {
            *status == ProviderEventStatus::Event
                && errors.is_empty()
                && response_errors.is_empty()
                && event_name.as_deref() == Some("rip.compat.warning")
                && data
                    .as_ref()
                    .and_then(|value| value.get("type"))
                    .and_then(|value| value.as_str())
                    == Some("rip.compat.warning")
        }
        _ => false,
    }));
    let warning_frames: Vec<_> = events
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::ProviderEvent {
                event_name, data, ..
            } if event_name.as_deref() == Some("rip.compat.warning") => data.clone(),
            _ => None,
        })
        .collect();
    drop(events);

    let requests = state.requests.lock().await;
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0].get("include"),
        Some(&serde_json::json!(["reasoning.encrypted_content"]))
    );
    assert!(requests[1].get("previous_response_id").is_none());
    let input = requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .expect("input items");
    assert_eq!(
        input
            .first()
            .and_then(|item| item.get("type"))
            .and_then(|value| value.as_str()),
        Some("message")
    );
    assert!(warning_frames.iter().any(|value| {
        value
            .get("message")
            .and_then(|value| value.as_str())
            .is_some_and(|text| text.contains("include=message.output_text.logprobs"))
    }));
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
        provider_id: None,
        endpoint: format!("http://{addr}/v1/responses"),
        api_key: None,
        model: Some("fixture-model".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::none(),
        include: Vec::new(),
        reasoning: None,
        web_search: None,
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
async fn run_openresponses_agent_loop_sends_openrouter_web_search_extension() {
    use axum::extract::{Json, State};
    use axum::http::header::CONTENT_TYPE;
    use axum::routing::post;
    use axum::Router as AxumRouter;
    use tokio::net::TcpListener;

    #[derive(Clone)]
    struct ProviderCapture {
        request: Arc<Mutex<Option<Value>>>,
    }

    async fn handler(
        State(capture): State<ProviderCapture>,
        Json(body): Json<Value>,
    ) -> impl axum::response::IntoResponse {
        *capture.request.lock().await = Some(body);
        let sse = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n\
data: [DONE]\n\n";
        ([(CONTENT_TYPE, "text/event-stream")], sse.to_string())
    }

    let capture = ProviderCapture {
        request: Arc::new(Mutex::new(None)),
    };
    let provider_app = AxumRouter::new()
        .route("/v1/responses", post(handler))
        .with_state(capture.clone());

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
        provider_id: Some("openrouter".to_string()),
        endpoint: format!("http://{addr}/v1/responses"),
        api_key: None,
        model: Some("google/gemma-4-26b-a4b-it".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        include: Vec::new(),
        reasoning: None,
        web_search: Some(OpenResponsesWebSearchConfig {
            enabled: true,
            search_context_size: Some(SearchContextSize::Medium),
            external_web_access: Some(true),
            user_location: Some(OpenResponsesApproximateLocation {
                country: Some("US".to_string()),
                region: None,
                city: None,
                timezone: None,
            }),
        }),
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
        prompt: "what happened today?",
        seq: &mut seq,
        sink,
    })
    .await;

    assert_eq!(outcome.reason, "completed");

    let warning_frames: Vec<_> = buffer
        .lock()
        .await
        .iter()
        .filter_map(|event| match &event.kind {
            EventKind::ProviderEvent {
                event_name, data, ..
            } if event_name.as_deref() == Some("rip.compat.warning") => data.clone(),
            _ => None,
        })
        .collect();
    assert!(warning_frames.iter().any(|value| {
        value
            .get("message")
            .and_then(|value| value.as_str())
            .is_some_and(|text| text.contains("external_web_access is not supported"))
    }));
    assert!(!warning_frames.iter().any(|value| {
        value
            .get("message")
            .and_then(|value| value.as_str())
            .is_some_and(|text| text.contains("user_location"))
    }));

    let request = capture.request.lock().await.clone().expect("request body");
    let tools = request
        .get("tools")
        .and_then(|value| value.as_array())
        .expect("tools array");
    assert!(tools
        .iter()
        .all(|tool| tool.get("type").and_then(|value| value.as_str()) != Some("web_search")));
    let web_search = tools
        .iter()
        .find(|tool| {
            tool.get("type").and_then(|value| value.as_str()) == Some("openrouter:web_search")
        })
        .expect("openrouter web_search tool");
    assert_eq!(
        web_search
            .get("parameters")
            .and_then(|value| value.get("search_context_size"))
            .and_then(|value| value.as_str()),
        Some("medium")
    );
    assert_eq!(
        web_search
            .get("parameters")
            .and_then(|value| value.get("user_location"))
            .and_then(|value| value.get("country"))
            .and_then(|value| value.as_str()),
        Some("US")
    );
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
        provider_id: None,
        endpoint: format!("http://{addr}/v1/responses"),
        api_key: None,
        model: Some("fixture-model".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        include: Vec::new(),
        reasoning: None,
        web_search: None,
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
        provider_id: None,
        endpoint: format!("http://{addr}/v1/responses"),
        api_key: None,
        model: Some("fixture-model".to_string()),
        headers: Vec::new(),
        tool_choice,
        include: Vec::new(),
        reasoning: None,
        web_search: None,
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
        provider_id: None,
        endpoint: format!("http://{addr}/v1/responses"),
        api_key: None,
        model: Some("fixture-model".to_string()),
        headers: Vec::new(),
        tool_choice: ToolChoiceParam::auto(),
        include: Vec::new(),
        reasoning: None,
        web_search: None,
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

#[tokio::test]
async fn run_session_with_prompt_and_no_provider_uses_runtime_loop() {
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
        input: "hello".to_string(),
    };

    run_session(ctx).await;
    let guard = events.lock().await;
    assert!(guard
        .iter()
        .any(|event| matches!(event.kind, EventKind::SessionStarted { .. })));
    assert!(guard
        .iter()
        .any(|event| matches!(event.kind, EventKind::OutputTextDelta { .. })));
    assert!(guard.iter().any(|event| matches!(
        &event.kind,
        EventKind::SessionEnded { reason } if reason == "completed"
    )));
}

#[tokio::test]
async fn run_session_ends_when_context_compile_fails_before_provider_loop() {
    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_dir).expect("workspace");
    let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
    let continuities = Arc::new(
        ContinuityStore::new(data_dir, workspace_dir, event_log.clone()).expect("continuities"),
    );
    let continuity_id = continuities.ensure_default().expect("ensure");
    let snapshot_dir = Arc::new(dir.path().join("snapshots"));
    let runtime = Arc::new(Runtime::new());

    let registry = Arc::new(rip_tools::ToolRegistry::default());
    let tool_runner = Arc::new(ToolRunner::new(registry, 1));
    let workspace_lock = Arc::new(crate::workspace_lock::WorkspaceLock::new());

    let (sender, _) = broadcast::channel(8);
    let events = Arc::new(Mutex::new(Vec::new()));
    let ctx = SessionContext {
        runtime,
        tool_runner,
        workspace_lock,
        http_client: reqwest::Client::new(),
        openresponses: Some(OpenResponsesConfig {
            provider_id: None,
            endpoint: "http://127.0.0.1:9/v1/responses".to_string(),
            api_key: None,
            model: Some("fixture-model".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            include: Vec::new(),
            reasoning: None,
            web_search: None,
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        }),
        sender,
        events: events.clone(),
        event_log,
        snapshot_dir,
        continuities,
        continuity_run: Some(ContinuityRunLink {
            continuity_id,
            message_id: "missing-message".to_string(),
            actor_id: "alice".to_string(),
            origin: "cli".to_string(),
        }),
        server_session_id: "s1".to_string(),
        input: "hello".to_string(),
    };

    run_session(ctx).await;
    let guard = events.lock().await;
    assert!(guard
        .iter()
        .any(|event| matches!(event.kind, EventKind::SessionStarted { .. })));
    assert!(guard.iter().any(|event| matches!(
        &event.kind,
        EventKind::SessionEnded { reason } if reason == "context_compile_failed"
    )));
    assert!(
        !guard
            .iter()
            .any(|event| matches!(event.kind, EventKind::ProviderEvent { .. })),
        "provider loop should not start when context compilation fails"
    );
}
