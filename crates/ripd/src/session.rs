use std::collections::{HashMap, HashSet};
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

use crate::compaction_summary::{
    read_compaction_summary_v1, COMPACTION_SUMMARY_KIND_CUMULATIVE_V1,
};
use crate::context_bundle::{write_bundle_v1, ContextBundleItemV1, ContextBundleV1};
use crate::context_compiler::{
    compile_hierarchical_summaries_recent_messages_v1, compile_recent_messages_v1,
    compile_summaries_recent_messages_v1, CompileHierarchicalSummariesRecentMessagesV1Request,
    CompileRecentMessagesV1Request, CompileSummariesRecentMessagesV1Request,
    HierarchicalSummaryRefV1, CONTEXT_COMPILER_ID_V1,
    CONTEXT_COMPILER_STRATEGY_HIERARCHICAL_SUMMARIES_RECENT_MESSAGES_V1,
    CONTEXT_COMPILER_STRATEGY_RECENT_MESSAGES_V1,
    CONTEXT_COMPILER_STRATEGY_SUMMARIES_RECENT_MESSAGES_V1, HIERARCHICAL_SUMMARIES_V1_MAX_REFS,
    RECENT_MESSAGES_V1_LIMIT,
};
use crate::continuities::{
    CompactionCheckpointForCompile, ContextCompiledPayload, ContextSelectionDecidedPayload,
    ContinuityRunLink, ContinuityStore, ToolSideEffects,
};
use crate::provider_openresponses::{
    build_streaming_followup_request, build_streaming_request, build_streaming_request_items,
    OpenResponsesConfig, DEFAULT_MAX_TOOL_CALLS,
};
use crate::workspace_lock::{requires_workspace_lock, WorkspaceLock};

mod context_compile;
mod openresponses;
mod streaming;
#[cfg(test)]
mod tests;

use self::context_compile::{compile_context_bundle_for_run, ContextCompileOutcomeForRun};
use self::openresponses::{
    run_openresponses_agent_loop, OpenResponsesLoopOutcome, OpenResponsesRunContext,
};
#[cfg(test)]
use self::openresponses::{stream_openresponses_request, OpenResponsesStreamRequest};
#[cfg(test)]
use self::streaming::{
    function_call_item_from_call, function_call_output_item, tool_events_to_function_call_output,
    FunctionCallItem, OpenResponsesSsePipe, ToolCallCollector,
};
use self::streaming::{summarize_continuity_tool_side_effects, EventSink};

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
                let sink = EventSink::new(&sender, &events, event_log.as_ref());
                let mut initial_items: Option<Vec<ItemParam>> = None;
                if let Some(link) = continuity_run.as_ref() {
                    match compile_context_bundle_for_run(
                        continuities.as_ref(),
                        event_log.as_ref(),
                        snapshot_dir.as_ref().as_path(),
                        link,
                        &runtime_session_id,
                    ) {
                        Ok(outcome) => {
                            let ContextCompileOutcomeForRun { decision, compiled } = outcome;
                            let compiler_strategy = decision.compiler_strategy.clone();
                            let _ = continuities.append_context_selection_decided(
                                &link.continuity_id,
                                ContextSelectionDecidedPayload {
                                    run_session_id: runtime_session_id.clone(),
                                    message_id: link.message_id.clone(),
                                    compiler_id: decision.compiler_id,
                                    compiler_strategy,
                                    limits: decision.limits,
                                    compaction_checkpoint: decision.compaction_checkpoint,
                                    compaction_checkpoints: decision.compaction_checkpoints,
                                    resets: decision.resets,
                                    reason: decision.reason,
                                    actor_id: link.actor_id.clone(),
                                    origin: link.origin.clone(),
                                },
                            );
                            let _ = continuities.append_context_compiled(
                                &link.continuity_id,
                                ContextCompiledPayload {
                                    run_session_id: runtime_session_id.clone(),
                                    bundle_artifact_id: compiled.bundle_artifact_id,
                                    compiler_id: CONTEXT_COMPILER_ID_V1.to_string(),
                                    compiler_strategy: decision.compiler_strategy,
                                    from_seq: compiled.from_seq,
                                    from_message_id: compiled.from_message_id,
                                    actor_id: link.actor_id.clone(),
                                    origin: link.origin.clone(),
                                },
                            );
                            initial_items = Some(compiled.items);
                        }
                        Err(_) => {
                            emit_event(
                                Event {
                                    id: Uuid::new_v4().to_string(),
                                    session_id: runtime_session_id.clone(),
                                    timestamp_ms: now_ms(),
                                    seq,
                                    kind: EventKind::SessionEnded {
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
                    let outcome = run_openresponses_agent_loop(OpenResponsesRunContext {
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
                    let OpenResponsesLoopOutcome {
                        reason,
                        last_response_id,
                    } = outcome;

                    if reason == "completed" {
                        if let (Some(link), Some(cursor)) =
                            (continuity_run.as_ref(), last_response_id.as_deref())
                        {
                            let _ = continuities.append_provider_cursor_updated(
                                &link.continuity_id,
                                crate::continuities::ProviderCursorUpdatedPayload {
                                    provider: "openresponses".to_string(),
                                    endpoint: Some(config.endpoint.clone()),
                                    model: config.model.clone(),
                                    cursor: Some(serde_json::json!({
                                        "previous_response_id": cursor
                                    })),
                                    action: "set".to_string(),
                                    reason: Some("run_completed".to_string()),
                                    run_session_id: Some(runtime_session_id.clone()),
                                    actor_id: link.actor_id.clone(),
                                    origin: link.origin.clone(),
                                },
                            );
                        }
                    }
                    emit_event(
                        Event {
                            id: Uuid::new_v4().to_string(),
                            session_id: runtime_session_id.clone(),
                            timestamp_ms: now_ms(),
                            seq,
                            kind: EventKind::SessionEnded { reason },
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
