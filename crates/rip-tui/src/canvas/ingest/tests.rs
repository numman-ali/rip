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
        CanvasMessage::SystemNotice {
            level, seq, text, ..
        } => {
            assert_eq!(*level, NoticeLevel::Danger);
            assert_eq!(*seq, 5);
            assert_eq!(text, "Provider error: bad");
        }
        _ => unreachable!(),
    }
}

#[test]
fn provider_reasoning_events_append_to_agent_turn_without_error_notice() {
    let mut canvas = CanvasModel::new();
    canvas.ingest(&event(
        0,
        100,
        EventKind::SessionStarted {
            input: "think".to_string(),
        },
    ));
    canvas.ingest(&event(
        1,
        110,
        EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Event,
            event_name: Some("response.reasoning.delta".to_string()),
            data: Some(json!({ "delta": "step one" })),
            raw: None,
            errors: Vec::new(),
            response_errors: Vec::new(),
        },
    ));
    canvas.ingest(&event(
        2,
        120,
        EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Event,
            event_name: Some("response.reasoning_summary_text.done".to_string()),
            data: Some(json!({ "text": "concise summary" })),
            raw: None,
            errors: Vec::new(),
            response_errors: Vec::new(),
        },
    ));

    let agent = canvas
        .messages
        .iter()
        .find_map(|message| match message {
            CanvasMessage::AgentTurn {
                reasoning_text,
                reasoning_summary,
                ..
            } => Some((reasoning_text, reasoning_summary)),
            _ => None,
        })
        .expect("agent turn");
    assert_eq!(agent.0, "step one");
    assert_eq!(agent.1, "concise summary");
    assert!(!canvas
        .messages
        .iter()
        .any(|message| matches!(message, CanvasMessage::SystemNotice { .. })));
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
