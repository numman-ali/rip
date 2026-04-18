//! Frame → `CanvasMessage` ingestion rules (Phase B.1).
//!
//! Takes frames from the continuity/run stream and translates them into the
//! structured canvas model. The renderer (B.2+) reads this model instead of
//! the string-based `output_text`.
//!
//! Invariants the revamp plan fixes in stone:
//!
//! - Nothing here reads external state; dispatch happens only on the frame's
//!   own fields. The kernel is responsible for emitting correct
//!   `job_kind` / `role` / `actor_id` / `origin`; the TUI renders them.
//! - `JobNotice` rides existing `ContinuityJobSpawned` / `ContinuityJobEnded`
//!   frames, not imagined `ContinuityRetrieval*` / `ContinuityReviewer*`
//!   kinds. A new `job_kind` value flows through with zero ingestion
//!   changes (glyph lookup is a render concern).
//! - `ExtensionPanel` is declared but never produced here — it lands when
//!   the P2 `extension.ui` capability ships.

use rip_kernel::{Event, EventKind, ProviderEventStatus, ToolTaskStatus};
use serde_json::Value;

use super::model::*;
use super::stream_collector::StreamCollector;
use super::CanvasModel;

pub(super) fn apply(canvas: &mut CanvasModel, event: &Event) {
    match &event.kind {
        EventKind::SessionStarted { input } => on_session_started(canvas, event, input),
        EventKind::OutputTextDelta { delta } => append_agent_delta(canvas, delta),
        EventKind::SessionEnded { .. } => finalize_agent_turn(canvas, event.timestamp_ms),
        EventKind::ToolStarted {
            tool_id,
            name,
            args,
            ..
        } => push_tool_card(canvas, event, tool_id, name, args),
        EventKind::ToolStdout { tool_id, chunk } => {
            append_tool_body(canvas, tool_id, ToolStream::Stdout, chunk);
        }
        EventKind::ToolStderr { tool_id, chunk } => {
            append_tool_body(canvas, tool_id, ToolStream::Stderr, chunk);
        }
        EventKind::ToolEnded {
            tool_id,
            exit_code,
            duration_ms,
            artifacts,
        } => finalize_tool_card_success(
            canvas,
            tool_id,
            *exit_code,
            *duration_ms,
            artifacts.as_ref(),
        ),
        EventKind::ToolFailed { tool_id, error } => {
            finalize_tool_card_failure(canvas, tool_id, error);
        }
        EventKind::ToolTaskSpawned {
            task_id,
            tool_name,
            title,
            execution_mode,
            artifacts,
            ..
        } => push_task_card(
            canvas,
            task_id,
            tool_name,
            title.clone(),
            *execution_mode,
            artifacts.as_ref(),
        ),
        EventKind::ToolTaskStatus {
            task_id,
            status,
            exit_code,
            started_at_ms,
            artifacts,
            error,
            ..
        } => update_task_card_status(
            canvas,
            task_id,
            *status,
            *exit_code,
            *started_at_ms,
            error.as_deref(),
            artifacts.as_ref(),
        ),
        EventKind::ToolTaskOutputDelta {
            task_id,
            stream,
            chunk,
            ..
        } => {
            let kind = match stream {
                rip_kernel::ToolTaskStream::Stdout => ToolStream::Stdout,
                rip_kernel::ToolTaskStream::Stderr => ToolStream::Stderr,
                rip_kernel::ToolTaskStream::Pty => ToolStream::Stdout,
            };
            append_task_body(canvas, task_id, kind, chunk);
        }
        EventKind::ProviderEvent {
            status,
            errors,
            response_errors,
            ..
        } => {
            if is_provider_error(status, errors, response_errors) {
                push_system_notice(
                    canvas,
                    event,
                    NoticeLevel::Danger,
                    "Provider error".to_string(),
                    "provider_event",
                );
            }
        }
        EventKind::CheckpointFailed { error, .. } => push_system_notice(
            canvas,
            event,
            NoticeLevel::Danger,
            format!("Checkpoint failed: {error}"),
            "checkpoint_failed",
        ),
        EventKind::ContinuityJobSpawned {
            job_id,
            job_kind,
            details,
            actor_id,
            origin,
        } => push_job_notice(
            canvas,
            event,
            job_id,
            job_kind,
            details.clone(),
            actor_id,
            origin,
        ),
        EventKind::ContinuityJobEnded {
            job_id,
            status,
            error,
            result,
            ..
        } => update_job_notice(
            canvas,
            job_id,
            status,
            result.as_ref(),
            error.as_deref(),
            event.timestamp_ms,
        ),
        EventKind::ContinuityContextSelectionDecided {
            run_session_id,
            compiler_strategy,
            ..
        } => upsert_context_notice(
            canvas,
            run_session_id,
            compiler_strategy,
            ContextLifecycle::Selecting,
            None,
        ),
        EventKind::ContinuityContextCompiled {
            run_session_id,
            compiler_strategy,
            bundle_artifact_id,
            ..
        } => upsert_context_notice(
            canvas,
            run_session_id,
            compiler_strategy,
            ContextLifecycle::Compiled,
            Some(bundle_artifact_id.clone()),
        ),
        EventKind::ContinuityCompactionCheckpointCreated {
            checkpoint_id,
            from_seq,
            to_seq,
            summary_artifact_id,
            ..
        } => push_compaction_checkpoint(
            canvas,
            checkpoint_id,
            *from_seq,
            *to_seq,
            summary_artifact_id,
        ),
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum ToolStream {
    Stdout,
    Stderr,
}

fn on_session_started(canvas: &mut CanvasModel, event: &Event, input: &str) {
    // If `begin_pending_turn` already pushed a `UserTurn`, the last message is
    // it; skip duplicating. Otherwise the run started without surface-side
    // submission (replay, backend-originated run) and we materialize the
    // implied user turn from the frame's `input`.
    let already_pending = matches!(canvas.messages.last(), Some(CanvasMessage::UserTurn { .. }));
    if !already_pending {
        let trimmed = input.trim();
        if !trimmed.is_empty() {
            let id = canvas.mint_id();
            canvas.messages.push(CanvasMessage::UserTurn {
                message_id: id,
                actor_id: "user".to_string(),
                origin: "frame".to_string(),
                blocks: vec![Block::Paragraph(CachedText::plain(trimmed))],
                submitted_at_ms: event.timestamp_ms,
            });
        }
    }

    let id = canvas.mint_id();
    canvas.messages.push(CanvasMessage::AgentTurn {
        message_id: id,
        run_session_id: event.session_id.clone(),
        agent_id: None,
        role: AgentRole::Primary,
        actor_id: "agent".to_string(),
        model: None,
        blocks: Vec::new(),
        streaming_tail: String::new(),
        streaming: true,
        started_at_ms: event.timestamp_ms,
        ended_at_ms: None,
    });
}

/// Feed a streaming delta through the collector and promote any
/// completed paragraphs (B.5). Tail text lives on the message itself
/// — we rebuild the collector from it each call since a single
/// `CanvasMessage` can't own an enum-variant-scoped collector without
/// significant API churn, and the rebuild is O(tail) in the worst case.
fn append_agent_delta(canvas: &mut CanvasModel, delta: &str) {
    if delta.is_empty() {
        return;
    }
    for message in canvas.messages.iter_mut().rev() {
        let CanvasMessage::AgentTurn {
            blocks,
            streaming,
            streaming_tail,
            ..
        } = message
        else {
            continue;
        };
        if !*streaming {
            continue;
        }
        let mut collector = StreamCollector::from_tail(std::mem::take(streaming_tail));
        let step = collector.push(delta);
        blocks.extend(step.new_stable);
        *streaming_tail = collector.into_tail();
        return;
    }
}

fn finalize_agent_turn(canvas: &mut CanvasModel, now_ms: u64) {
    for message in canvas.messages.iter_mut().rev() {
        let CanvasMessage::AgentTurn {
            streaming,
            streaming_tail,
            ended_at_ms,
            blocks,
            ..
        } = message
        else {
            continue;
        };
        if !*streaming {
            continue;
        }
        let mut collector = StreamCollector::from_tail(std::mem::take(streaming_tail));
        blocks.extend(collector.finalize());
        *streaming_tail = String::new();
        *streaming = false;
        *ended_at_ms = Some(now_ms);
        return;
    }
}

fn push_tool_card(
    canvas: &mut CanvasModel,
    event: &Event,
    tool_id: &str,
    name: &str,
    args: &Value,
) {
    let id = canvas.mint_id();
    let args_text = match serde_json::to_string_pretty(args) {
        Ok(pretty) => pretty,
        Err(_) => args.to_string(),
    };
    canvas.messages.push(CanvasMessage::ToolCard {
        message_id: id,
        tool_id: tool_id.to_string(),
        tool_name: name.to_string(),
        args_block: Block::ToolArgsJson(CachedText::plain(&args_text)),
        status: ToolCardStatus::Running,
        body: Vec::new(),
        expanded: false,
        artifact_ids: Vec::new(),
        started_seq: event.seq,
        started_at_ms: event.timestamp_ms,
    });
}

fn append_tool_body(canvas: &mut CanvasModel, tool_id: &str, stream: ToolStream, chunk: &str) {
    if chunk.is_empty() {
        return;
    }
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::ToolCard {
            tool_id: id, body, ..
        } = message
        {
            if id == tool_id {
                let block = match stream {
                    ToolStream::Stdout => Block::ToolStdout(CachedText::plain(chunk)),
                    ToolStream::Stderr => Block::ToolStderr(CachedText::plain(chunk)),
                };
                body.push(block);
                return;
            }
        }
    }
}

fn finalize_tool_card_success(
    canvas: &mut CanvasModel,
    tool_id: &str,
    exit_code: i32,
    duration_ms: u64,
    artifacts: Option<&Value>,
) {
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::ToolCard {
            tool_id: id,
            status,
            artifact_ids,
            ..
        } = message
        {
            if id == tool_id {
                *status = ToolCardStatus::Succeeded {
                    duration_ms,
                    exit_code,
                };
                if let Some(value) = artifacts {
                    merge_artifact_ids(artifact_ids, value);
                }
                return;
            }
        }
    }
}

fn finalize_tool_card_failure(canvas: &mut CanvasModel, tool_id: &str, error: &str) {
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::ToolCard {
            tool_id: id,
            status,
            ..
        } = message
        {
            if id == tool_id {
                *status = ToolCardStatus::Failed {
                    error: error.to_string(),
                };
                return;
            }
        }
    }
}

fn push_task_card(
    canvas: &mut CanvasModel,
    task_id: &str,
    tool_name: &str,
    title: Option<String>,
    execution_mode: rip_kernel::ToolTaskExecutionMode,
    artifacts: Option<&Value>,
) {
    let id = canvas.mint_id();
    let mut artifact_ids: Vec<String> = Vec::new();
    if let Some(value) = artifacts {
        merge_artifact_ids(&mut artifact_ids, value);
    }
    canvas.messages.push(CanvasMessage::TaskCard {
        message_id: id,
        task_id: task_id.to_string(),
        tool_name: tool_name.to_string(),
        title,
        execution_mode,
        status: TaskCardStatus::Queued,
        body: Vec::new(),
        expanded: false,
        artifact_ids,
        started_at_ms: None,
    });
}

fn update_task_card_status(
    canvas: &mut CanvasModel,
    task_id: &str,
    new_status: ToolTaskStatus,
    exit_code: Option<i32>,
    new_started_at_ms: Option<u64>,
    error: Option<&str>,
    artifacts: Option<&Value>,
) {
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::TaskCard {
            task_id: id,
            status,
            artifact_ids,
            started_at_ms,
            ..
        } = message
        {
            if id == task_id {
                *status = match new_status {
                    ToolTaskStatus::Queued => TaskCardStatus::Queued,
                    ToolTaskStatus::Running => TaskCardStatus::Running,
                    ToolTaskStatus::Exited => TaskCardStatus::Exited { exit_code },
                    ToolTaskStatus::Cancelled => TaskCardStatus::Cancelled,
                    ToolTaskStatus::Failed => TaskCardStatus::Failed {
                        error: error.map(ToString::to_string),
                    },
                };
                if started_at_ms.is_none() {
                    *started_at_ms = new_started_at_ms;
                }
                if let Some(value) = artifacts {
                    merge_artifact_ids(artifact_ids, value);
                }
                return;
            }
        }
    }
}

fn append_task_body(canvas: &mut CanvasModel, task_id: &str, stream: ToolStream, chunk: &str) {
    if chunk.is_empty() {
        return;
    }
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::TaskCard {
            task_id: id, body, ..
        } = message
        {
            if id == task_id {
                let block = match stream {
                    ToolStream::Stdout => Block::ToolStdout(CachedText::plain(chunk)),
                    ToolStream::Stderr => Block::ToolStderr(CachedText::plain(chunk)),
                };
                body.push(block);
                return;
            }
        }
    }
}

fn push_system_notice(
    canvas: &mut CanvasModel,
    event: &Event,
    level: NoticeLevel,
    text: String,
    origin_event_kind: &str,
) {
    let id = canvas.mint_id();
    canvas.messages.push(CanvasMessage::SystemNotice {
        message_id: id,
        level,
        text,
        origin_event_kind: origin_event_kind.to_string(),
        seq: event.seq,
    });
}

#[allow(clippy::too_many_arguments)]
fn push_job_notice(
    canvas: &mut CanvasModel,
    event: &Event,
    job_id: &str,
    job_kind: &str,
    details: Option<Value>,
    actor_id: &str,
    origin: &str,
) {
    let id = canvas.mint_id();
    canvas.messages.push(CanvasMessage::JobNotice {
        message_id: id,
        job_id: job_id.to_string(),
        job_kind: job_kind.to_string(),
        details,
        status: JobLifecycle::Running,
        actor_id: actor_id.to_string(),
        origin: origin.to_string(),
        started_at_ms: Some(event.timestamp_ms),
        ended_at_ms: None,
    });
}

fn update_job_notice(
    canvas: &mut CanvasModel,
    job_id: &str,
    status_str: &str,
    result: Option<&Value>,
    error: Option<&str>,
    now_ms: u64,
) {
    let lifecycle = match status_str {
        "completed" | "succeeded" | "success" => JobLifecycle::Succeeded {
            result: result.cloned(),
        },
        "cancelled" | "canceled" => JobLifecycle::Cancelled,
        _ => JobLifecycle::Failed {
            error: error.map(ToString::to_string),
        },
    };
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::JobNotice {
            job_id: id,
            status,
            ended_at_ms,
            ..
        } = message
        {
            if id == job_id {
                *status = lifecycle;
                *ended_at_ms = Some(now_ms);
                return;
            }
        }
    }
}

fn upsert_context_notice(
    canvas: &mut CanvasModel,
    run_session_id: &str,
    strategy: &str,
    lifecycle: ContextLifecycle,
    bundle_artifact_id: Option<String>,
) {
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::ContextNotice {
            run_session_id: id,
            strategy: current_strategy,
            status,
            bundle_artifact_id: current_bundle,
            ..
        } = message
        {
            if id == run_session_id {
                *current_strategy = strategy.to_string();
                *status = lifecycle;
                if bundle_artifact_id.is_some() {
                    *current_bundle = bundle_artifact_id;
                }
                return;
            }
        }
    }
    let id = canvas.mint_id();
    canvas.messages.push(CanvasMessage::ContextNotice {
        message_id: id,
        run_session_id: run_session_id.to_string(),
        strategy: strategy.to_string(),
        status: lifecycle,
        bundle_artifact_id,
        contributed_artifact_ids: Vec::new(),
    });
}

fn push_compaction_checkpoint(
    canvas: &mut CanvasModel,
    checkpoint_id: &str,
    from_seq: u64,
    to_seq: u64,
    summary_artifact_id: &str,
) {
    let id = canvas.mint_id();
    canvas.messages.push(CanvasMessage::CompactionCheckpoint {
        message_id: id,
        checkpoint_id: checkpoint_id.to_string(),
        from_seq,
        to_seq,
        summary_artifact_id: summary_artifact_id.to_string(),
    });
}

fn merge_artifact_ids(target: &mut Vec<String>, value: &Value) {
    let mut ids = Vec::new();
    collect_artifact_ids(value, &mut ids);
    for id in ids {
        if !target.iter().any(|existing| existing == &id) {
            target.push(id);
        }
    }
}

fn collect_artifact_ids(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
        Value::String(s) if looks_like_artifact_id(s) => out.push(s.clone()),
        Value::String(_) => {}
        Value::Array(items) => {
            for item in items {
                collect_artifact_ids(item, out);
            }
        }
        Value::Object(map) => {
            for v in map.values() {
                collect_artifact_ids(v, out);
            }
        }
    }
}

fn looks_like_artifact_id(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn is_provider_error(
    status: &ProviderEventStatus,
    errors: &[String],
    response_errors: &[String],
) -> bool {
    *status == ProviderEventStatus::InvalidJson
        || (*status != ProviderEventStatus::Done
            && (!errors.is_empty() || !response_errors.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::super::CanvasModel;
    use super::*;
    use rip_kernel::{Event, EventKind, ToolTaskExecutionMode};
    use serde_json::json;

    fn event(seq: u64, timestamp_ms: u64, kind: EventKind) -> Event {
        Event {
            id: format!("e{seq}"),
            session_id: "s1".to_string(),
            timestamp_ms,
            seq,
            kind,
        }
    }

    #[test]
    fn session_started_pushes_user_and_agent_turns_when_no_pending_exists() {
        let mut canvas = CanvasModel::new();
        canvas.ingest(&event(
            0,
            100,
            EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        ));
        assert_eq!(canvas.messages.len(), 2);
        assert!(matches!(canvas.messages[0], CanvasMessage::UserTurn { .. }));
        assert!(matches!(
            canvas.messages[1],
            CanvasMessage::AgentTurn {
                streaming: true,
                ..
            }
        ));
    }

    #[test]
    fn session_started_skips_pending_user_turn() {
        let mut canvas = CanvasModel::new();
        let id = canvas.mint_id();
        canvas.messages.push(CanvasMessage::UserTurn {
            message_id: id,
            actor_id: "user".into(),
            origin: "tui".into(),
            blocks: vec![Block::Paragraph(CachedText::plain("hi"))],
            submitted_at_ms: 90,
        });
        canvas.ingest(&event(
            0,
            100,
            EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        ));
        let user_count = canvas
            .messages
            .iter()
            .filter(|m| matches!(m, CanvasMessage::UserTurn { .. }))
            .count();
        assert_eq!(user_count, 1);
        assert!(matches!(
            canvas.messages.last(),
            Some(CanvasMessage::AgentTurn { .. })
        ));
    }

    #[test]
    fn output_delta_then_session_ended_closes_the_agent_turn() {
        let mut canvas = CanvasModel::new();
        canvas.ingest(&event(
            0,
            100,
            EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        ));
        canvas.ingest(&event(
            1,
            110,
            EventKind::OutputTextDelta {
                delta: "hello".to_string(),
            },
        ));
        canvas.ingest(&event(
            2,
            120,
            EventKind::SessionEnded {
                reason: "done".to_string(),
            },
        ));

        let agent = canvas
            .messages
            .iter()
            .find_map(|m| match m {
                CanvasMessage::AgentTurn {
                    blocks,
                    streaming,
                    ended_at_ms,
                    ..
                } => Some((blocks.len(), *streaming, *ended_at_ms)),
                _ => None,
            })
            .expect("agent turn present");
        assert_eq!(agent.0, 1);
        assert!(!agent.1);
        assert_eq!(agent.2, Some(120));
    }

    #[test]
    fn tool_lifecycle_emits_running_then_succeeded_with_artifacts_folded_in() {
        let mut canvas = CanvasModel::new();
        let artifact_id: String = std::iter::repeat_n('a', 64).collect();
        canvas.ingest(&event(
            0,
            100,
            EventKind::ToolStarted {
                tool_id: "t1".to_string(),
                name: "write".to_string(),
                args: json!({"path": "foo.md"}),
                timeout_ms: None,
            },
        ));
        canvas.ingest(&event(
            1,
            110,
            EventKind::ToolStdout {
                tool_id: "t1".to_string(),
                chunk: "wrote 1 byte".to_string(),
            },
        ));
        canvas.ingest(&event(
            2,
            120,
            EventKind::ToolEnded {
                tool_id: "t1".to_string(),
                exit_code: 0,
                duration_ms: 15,
                artifacts: Some(json!({"id": artifact_id})),
            },
        ));

        let card = canvas
            .messages
            .iter()
            .find(|m| matches!(m, CanvasMessage::ToolCard { .. }))
            .expect("tool card");
        match card {
            CanvasMessage::ToolCard {
                tool_name,
                status,
                body,
                artifact_ids,
                ..
            } => {
                assert_eq!(tool_name, "write");
                assert!(matches!(
                    status,
                    ToolCardStatus::Succeeded {
                        duration_ms: 15,
                        exit_code: 0
                    }
                ));
                assert_eq!(body.len(), 1);
                assert_eq!(artifact_ids.len(), 1);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn provider_error_pushes_system_notice_danger() {
        let mut canvas = CanvasModel::new();
        canvas.ingest(&event(
            5,
            200,
            EventKind::ProviderEvent {
                provider: "openresponses".to_string(),
                status: ProviderEventStatus::InvalidJson,
                event_name: None,
                data: None,
                raw: None,
                errors: vec!["bad".to_string()],
                response_errors: Vec::new(),
            },
        ));
        let notice = canvas
            .messages
            .iter()
            .find(|m| matches!(m, CanvasMessage::SystemNotice { .. }))
            .expect("notice");
        match notice {
            CanvasMessage::SystemNotice { level, seq, .. } => {
                assert_eq!(*level, NoticeLevel::Danger);
                assert_eq!(*seq, 5);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn continuity_jobs_spawn_as_running_then_resolve() {
        let mut canvas = CanvasModel::new();
        canvas.ingest(&event(
            0,
            100,
            EventKind::ContinuityJobSpawned {
                job_id: "j1".to_string(),
                job_kind: "compaction".to_string(),
                details: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ));
        canvas.ingest(&event(
            1,
            110,
            EventKind::ContinuityJobEnded {
                job_id: "j1".to_string(),
                job_kind: "compaction".to_string(),
                status: "completed".to_string(),
                result: Some(json!({"ok": true})),
                error: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ));
        let notice = canvas
            .messages
            .iter()
            .find(|m| matches!(m, CanvasMessage::JobNotice { .. }))
            .expect("job notice");
        match notice {
            CanvasMessage::JobNotice {
                status,
                ended_at_ms,
                ..
            } => {
                assert!(matches!(status, JobLifecycle::Succeeded { .. }));
                assert_eq!(*ended_at_ms, Some(110));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn context_notice_upserts_from_selecting_to_compiled() {
        let mut canvas = CanvasModel::new();
        canvas.ingest(&event(
            0,
            100,
            EventKind::ContinuityContextSelectionDecided {
                run_session_id: "r1".to_string(),
                message_id: "m1".to_string(),
                compiler_id: "rip.context_compiler.v1".to_string(),
                compiler_strategy: "recent_messages_v1".to_string(),
                limits: json!({}),
                compaction_checkpoint: None,
                compaction_checkpoints: Vec::new(),
                resets: Vec::new(),
                reason: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ));
        let bundle: String = std::iter::repeat_n('b', 64).collect();
        canvas.ingest(&event(
            1,
            110,
            EventKind::ContinuityContextCompiled {
                run_session_id: "r1".to_string(),
                bundle_artifact_id: bundle.clone(),
                compiler_id: "rip.context_compiler.v1".to_string(),
                compiler_strategy: "recent_messages_v1".to_string(),
                from_seq: 0,
                from_message_id: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ));

        let count = canvas
            .messages
            .iter()
            .filter(|m| matches!(m, CanvasMessage::ContextNotice { .. }))
            .count();
        assert_eq!(count, 1);
        match canvas
            .messages
            .iter()
            .find(|m| matches!(m, CanvasMessage::ContextNotice { .. }))
            .unwrap()
        {
            CanvasMessage::ContextNotice {
                status,
                bundle_artifact_id,
                ..
            } => {
                assert!(matches!(status, ContextLifecycle::Compiled));
                assert_eq!(bundle_artifact_id.as_deref(), Some(bundle.as_str()));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn task_card_task_spawn_sets_mode_and_status_progresses_to_exited() {
        let mut canvas = CanvasModel::new();
        canvas.ingest(&event(
            0,
            100,
            EventKind::ToolTaskSpawned {
                task_id: "task-1".to_string(),
                tool_name: "shell".to_string(),
                args: json!({"cmd": "pwd"}),
                cwd: None,
                title: Some("pwd".to_string()),
                execution_mode: ToolTaskExecutionMode::Pipes,
                origin_session_id: None,
                artifacts: None,
            },
        ));
        canvas.ingest(&event(
            1,
            110,
            EventKind::ToolTaskStatus {
                task_id: "task-1".to_string(),
                status: ToolTaskStatus::Exited,
                exit_code: Some(0),
                started_at_ms: Some(105),
                ended_at_ms: Some(110),
                artifacts: None,
                error: None,
            },
        ));
        let card = canvas
            .messages
            .iter()
            .find(|m| matches!(m, CanvasMessage::TaskCard { .. }))
            .expect("task card");
        match card {
            CanvasMessage::TaskCard {
                status,
                started_at_ms,
                ..
            } => {
                assert!(matches!(
                    status,
                    TaskCardStatus::Exited { exit_code: Some(0) }
                ));
                assert_eq!(*started_at_ms, Some(105));
            }
            _ => unreachable!(),
        }
    }
}
