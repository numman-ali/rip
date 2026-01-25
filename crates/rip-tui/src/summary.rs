use rip_kernel::{Event, EventKind, ProviderEventStatus};

pub fn event_type(event: &Event) -> &'static str {
    match &event.kind {
        EventKind::SessionStarted { .. } => "session_started",
        EventKind::OutputTextDelta { .. } => "output_text_delta",
        EventKind::SessionEnded { .. } => "session_ended",
        EventKind::ContinuityCreated { .. } => "continuity_created",
        EventKind::ContinuityMessageAppended { .. } => "continuity_message_appended",
        EventKind::ContinuityRunSpawned { .. } => "continuity_run_spawned",
        EventKind::ContinuityContextCompiled { .. } => "continuity_context_compiled",
        EventKind::ContinuityProviderCursorUpdated { .. } => "continuity_provider_cursor_updated",
        EventKind::ContinuityCompactionCheckpointCreated { .. } => {
            "continuity_compaction_checkpoint_created"
        }
        EventKind::ContinuityCompactionAutoScheduleDecided { .. } => {
            "continuity_compaction_auto_schedule_decided"
        }
        EventKind::ContinuityJobSpawned { .. } => "continuity_job_spawned",
        EventKind::ContinuityJobEnded { .. } => "continuity_job_ended",
        EventKind::ContinuityRunEnded { .. } => "continuity_run_ended",
        EventKind::ContinuityToolSideEffects { .. } => "continuity_tool_side_effects",
        EventKind::ContinuityBranched { .. } => "continuity_branched",
        EventKind::ContinuityHandoffCreated { .. } => "continuity_handoff_created",
        EventKind::ToolStarted { .. } => "tool_started",
        EventKind::ToolStdout { .. } => "tool_stdout",
        EventKind::ToolStderr { .. } => "tool_stderr",
        EventKind::ToolEnded { .. } => "tool_ended",
        EventKind::ToolFailed { .. } => "tool_failed",
        EventKind::ProviderEvent { .. } => "provider_event",
        EventKind::CheckpointCreated { .. } => "checkpoint_created",
        EventKind::CheckpointRewound { .. } => "checkpoint_rewound",
        EventKind::CheckpointFailed { .. } => "checkpoint_failed",
        EventKind::ToolTaskSpawned { .. } => "tool_task_spawned",
        EventKind::ToolTaskStatus { .. } => "tool_task_status",
        EventKind::ToolTaskCancelRequested { .. } => "tool_task_cancel_requested",
        EventKind::ToolTaskCancelled { .. } => "tool_task_cancelled",
        EventKind::ToolTaskOutputDelta { .. } => "tool_task_output_delta",
        EventKind::ToolTaskStdinWritten { .. } => "tool_task_stdin_written",
        EventKind::ToolTaskResized { .. } => "tool_task_resized",
        EventKind::ToolTaskSignalled { .. } => "tool_task_signalled",
    }
}

pub fn event_summary(event: &Event) -> String {
    match &event.kind {
        EventKind::SessionStarted { input } => format!("{:?}", truncate(input, 64)),
        EventKind::OutputTextDelta { delta } => format!("{:?}", truncate(delta, 64)),
        EventKind::SessionEnded { reason } => format!("{:?}", truncate(reason, 64)),
        EventKind::ContinuityCreated { workspace, title } => {
            if let Some(title) = title.as_deref().filter(|t| !t.is_empty()) {
                format!("{:?}", truncate(title, 64))
            } else {
                format!("{:?}", truncate(workspace, 64))
            }
        }
        EventKind::ContinuityMessageAppended { content, .. } => {
            format!("{:?}", truncate(content, 64))
        }
        EventKind::ContinuityRunSpawned { run_session_id, .. } => {
            format!("run={}", truncate(run_session_id, 16))
        }
        EventKind::ContinuityContextCompiled {
            run_session_id,
            bundle_artifact_id,
            compiler_strategy,
            ..
        } => format!(
            "run={} bundle={} ({})",
            truncate(run_session_id, 16),
            truncate(bundle_artifact_id, 16),
            truncate(compiler_strategy, 32)
        ),
        EventKind::ContinuityProviderCursorUpdated {
            provider,
            action,
            cursor,
            ..
        } => {
            let prev = cursor
                .as_ref()
                .and_then(|value| value.get("previous_response_id"))
                .and_then(|value| value.as_str())
                .unwrap_or("");
            let prev_short = truncate(prev, 16);
            if cursor.is_some() && !prev_short.is_empty() {
                format!(
                    "provider={} action={} prev={}",
                    truncate(provider, 16),
                    truncate(action, 16),
                    prev_short
                )
            } else if cursor.is_some() {
                format!(
                    "provider={} action={} cursor=set",
                    truncate(provider, 16),
                    truncate(action, 16)
                )
            } else {
                format!(
                    "provider={} action={} cursor=none",
                    truncate(provider, 16),
                    truncate(action, 16)
                )
            }
        }
        EventKind::ContinuityCompactionCheckpointCreated {
            checkpoint_id,
            summary_artifact_id,
            to_seq,
            cut_rule_id,
            ..
        } => format!(
            "ckpt={} to_seq={} summary={} ({})",
            truncate(checkpoint_id, 16),
            to_seq,
            truncate(summary_artifact_id, 16),
            truncate(cut_rule_id, 32)
        ),
        EventKind::ContinuityCompactionAutoScheduleDecided {
            policy_id,
            decision,
            job_id,
            ..
        } => match job_id.as_deref() {
            Some(job_id) => format!(
                "policy={} decision={} job={}",
                truncate(policy_id, 32),
                truncate(decision, 32),
                truncate(job_id, 16)
            ),
            None => format!(
                "policy={} decision={}",
                truncate(policy_id, 32),
                truncate(decision, 32)
            ),
        },
        EventKind::ContinuityJobSpawned {
            job_id, job_kind, ..
        } => format!("job={} id={}", truncate(job_kind, 32), truncate(job_id, 16)),
        EventKind::ContinuityJobEnded {
            job_id,
            job_kind,
            status,
            ..
        } => format!(
            "job={} id={} ({})",
            truncate(job_kind, 32),
            truncate(job_id, 16),
            truncate(status, 32)
        ),
        EventKind::ContinuityRunEnded {
            run_session_id,
            reason,
            ..
        } => format!(
            "run={} ({})",
            truncate(run_session_id, 16),
            truncate(reason, 32)
        ),
        EventKind::ContinuityToolSideEffects {
            run_session_id,
            tool_name,
            affected_paths,
            ..
        } => {
            let paths = match affected_paths {
                Some(paths) => paths.len().to_string(),
                None => "?".to_string(),
            };
            format!(
                "run={} tool={} (paths={})",
                truncate(run_session_id, 16),
                truncate(tool_name, 32),
                paths
            )
        }
        EventKind::ContinuityBranched {
            parent_thread_id,
            parent_seq,
            ..
        } => format!("from={} @{}", truncate(parent_thread_id, 16), parent_seq),
        EventKind::ContinuityHandoffCreated {
            from_thread_id,
            from_seq,
            ..
        } => format!("from={} @{}", truncate(from_thread_id, 16), from_seq),
        EventKind::ToolStarted { name, .. } => name.to_string(),
        EventKind::ToolStdout { chunk, .. } | EventKind::ToolStderr { chunk, .. } => {
            format!("{:?}", truncate(chunk, 64))
        }
        EventKind::ToolEnded { exit_code, .. } => format!("exit={exit_code}"),
        EventKind::ToolFailed { error, .. } => format!("{:?}", truncate(error, 64)),
        EventKind::ProviderEvent {
            status,
            event_name,
            errors,
            response_errors,
            ..
        } => match status {
            ProviderEventStatus::Event => event_name.as_deref().unwrap_or("event").to_string(),
            ProviderEventStatus::Done => "done".to_string(),
            ProviderEventStatus::InvalidJson => {
                if !errors.is_empty() || !response_errors.is_empty() {
                    format!(
                        "invalid_json ({})",
                        errors.len().saturating_add(response_errors.len())
                    )
                } else {
                    "invalid_json".to_string()
                }
            }
        },
        EventKind::CheckpointCreated { label, .. } | EventKind::CheckpointRewound { label, .. } => {
            format!("{:?}", truncate(label, 64))
        }
        EventKind::CheckpointFailed { error, .. } => format!("{:?}", truncate(error, 64)),
        EventKind::ToolTaskSpawned { tool_name, .. } => tool_name.to_string(),
        EventKind::ToolTaskStatus { status, .. } => format!("{status:?}").to_lowercase(),
        EventKind::ToolTaskCancelRequested { reason, .. }
        | EventKind::ToolTaskCancelled { reason, .. } => format!("{:?}", truncate(reason, 64)),
        EventKind::ToolTaskOutputDelta { chunk, .. } => format!("{:?}", truncate(chunk, 64)),
        EventKind::ToolTaskStdinWritten { chunk_b64, .. } => {
            format!("{:?}", truncate(chunk_b64, 64))
        }
        EventKind::ToolTaskResized { rows, cols, .. } => format!("{rows}x{cols}"),
        EventKind::ToolTaskSignalled { signal, .. } => signal.to_string(),
    }
}

fn truncate(input: &str, max_len: usize) -> String {
    if input.chars().count() <= max_len {
        return input.to_string();
    }
    input.chars().take(max_len).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;
    use rip_kernel::{CheckpointAction, Event, EventKind, ProviderEventStatus};

    fn make_event(kind: EventKind) -> Event {
        Event {
            id: "e1".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind,
        }
    }

    #[test]
    fn event_type_maps_variants() {
        let cases = [
            (
                EventKind::SessionStarted {
                    input: "hi".to_string(),
                },
                "session_started",
            ),
            (
                EventKind::OutputTextDelta {
                    delta: "hi".to_string(),
                },
                "output_text_delta",
            ),
            (
                EventKind::SessionEnded {
                    reason: "done".to_string(),
                },
                "session_ended",
            ),
            (
                EventKind::ContinuityCreated {
                    workspace: "/workspace".to_string(),
                    title: None,
                },
                "continuity_created",
            ),
            (
                EventKind::ContinuityMessageAppended {
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                    content: "hi".to_string(),
                },
                "continuity_message_appended",
            ),
            (
                EventKind::ContinuityRunSpawned {
                    run_session_id: "s1".to_string(),
                    message_id: "m1".to_string(),
                    actor_id: None,
                    origin: None,
                },
                "continuity_run_spawned",
            ),
            (
                EventKind::ContinuityContextCompiled {
                    run_session_id: "s1".to_string(),
                    bundle_artifact_id: "a1".to_string(),
                    compiler_id: "rip.context_compiler.v1".to_string(),
                    compiler_strategy: "recent_messages_v1".to_string(),
                    from_seq: 3,
                    from_message_id: Some("m1".to_string()),
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
                "continuity_context_compiled",
            ),
            (
                EventKind::ContinuityProviderCursorUpdated {
                    provider: "openresponses".to_string(),
                    endpoint: None,
                    model: None,
                    cursor: None,
                    action: "rotated".to_string(),
                    reason: None,
                    run_session_id: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
                "continuity_provider_cursor_updated",
            ),
            (
                EventKind::ContinuityJobSpawned {
                    job_id: "j1".to_string(),
                    job_kind: "compaction_summarizer_v1".to_string(),
                    details: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
                "continuity_job_spawned",
            ),
            (
                EventKind::ContinuityJobEnded {
                    job_id: "j1".to_string(),
                    job_kind: "compaction_summarizer_v1".to_string(),
                    status: "completed".to_string(),
                    result: None,
                    error: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
                "continuity_job_ended",
            ),
            (
                EventKind::ContinuityRunEnded {
                    run_session_id: "s1".to_string(),
                    message_id: "m1".to_string(),
                    reason: "completed".to_string(),
                    actor_id: None,
                    origin: None,
                },
                "continuity_run_ended",
            ),
            (
                EventKind::ContinuityToolSideEffects {
                    run_session_id: "s1".to_string(),
                    tool_id: "tool_1".to_string(),
                    tool_name: "write".to_string(),
                    affected_paths: Some(vec!["a.txt".to_string()]),
                    checkpoint_id: Some("c1".to_string()),
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
                "continuity_tool_side_effects",
            ),
            (
                EventKind::ContinuityBranched {
                    parent_thread_id: "t1".to_string(),
                    parent_seq: 3,
                    parent_message_id: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
                "continuity_branched",
            ),
            (
                EventKind::ToolStarted {
                    tool_id: "t1".to_string(),
                    name: "ls".to_string(),
                    args: serde_json::json!({}),
                    timeout_ms: None,
                },
                "tool_started",
            ),
            (
                EventKind::ToolStdout {
                    tool_id: "t1".to_string(),
                    chunk: "out".to_string(),
                },
                "tool_stdout",
            ),
            (
                EventKind::ToolStderr {
                    tool_id: "t1".to_string(),
                    chunk: "err".to_string(),
                },
                "tool_stderr",
            ),
            (
                EventKind::ToolEnded {
                    tool_id: "t1".to_string(),
                    exit_code: 0,
                    duration_ms: 1,
                    artifacts: None,
                },
                "tool_ended",
            ),
            (
                EventKind::ToolFailed {
                    tool_id: "t1".to_string(),
                    error: "fail".to_string(),
                },
                "tool_failed",
            ),
            (
                EventKind::ProviderEvent {
                    provider: "openresponses".to_string(),
                    status: ProviderEventStatus::Event,
                    event_name: Some("response.output_text.delta".to_string()),
                    data: None,
                    raw: None,
                    errors: vec![],
                    response_errors: vec![],
                },
                "provider_event",
            ),
            (
                EventKind::CheckpointCreated {
                    checkpoint_id: "c1".to_string(),
                    label: "snap".to_string(),
                    created_at_ms: 1,
                    files: vec![],
                    auto: false,
                    tool_name: None,
                },
                "checkpoint_created",
            ),
            (
                EventKind::CheckpointRewound {
                    checkpoint_id: "c1".to_string(),
                    label: "snap".to_string(),
                    files: vec![],
                },
                "checkpoint_rewound",
            ),
            (
                EventKind::CheckpointFailed {
                    action: CheckpointAction::Create,
                    error: "nope".to_string(),
                },
                "checkpoint_failed",
            ),
        ];

        for (kind, expected) in cases {
            let event = make_event(kind);
            assert_eq!(event_type(&event), expected);
        }
    }

    #[test]
    fn event_summary_formats_provider_event_statuses() {
        let event = make_event(EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Event,
            event_name: None,
            data: None,
            raw: None,
            errors: vec![],
            response_errors: vec![],
        });
        assert_eq!(event_summary(&event), "event");

        let event = make_event(EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Done,
            event_name: Some("response.completed".to_string()),
            data: None,
            raw: None,
            errors: vec![],
            response_errors: vec![],
        });
        assert_eq!(event_summary(&event), "done");

        let event = make_event(EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::InvalidJson,
            event_name: None,
            data: None,
            raw: None,
            errors: vec!["bad json".to_string()],
            response_errors: vec!["schema".to_string()],
        });
        assert_eq!(event_summary(&event), "invalid_json (2)");
    }

    #[test]
    fn event_summary_truncates_long_values() {
        let long = "a".repeat(70);
        let event = make_event(EventKind::SessionStarted { input: long });
        let summary = event_summary(&event);
        assert!(summary.starts_with("\""));
        assert!(summary.ends_with("…\""));
        assert_eq!(summary.chars().count(), 67);
    }

    #[test]
    fn event_summary_handles_additional_variants() {
        let event = make_event(EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Event,
            event_name: Some("response.output_text.delta".to_string()),
            data: None,
            raw: None,
            errors: vec![],
            response_errors: vec![],
        });
        assert_eq!(event_summary(&event), "response.output_text.delta");

        let event = make_event(EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::InvalidJson,
            event_name: None,
            data: None,
            raw: None,
            errors: vec![],
            response_errors: vec![],
        });
        assert_eq!(event_summary(&event), "invalid_json");

        let event = make_event(EventKind::ToolStarted {
            tool_id: "t1".to_string(),
            name: "ls".to_string(),
            args: serde_json::json!({}),
            timeout_ms: None,
        });
        assert_eq!(event_summary(&event), "ls");

        let event = make_event(EventKind::ToolEnded {
            tool_id: "t1".to_string(),
            exit_code: 42,
            duration_ms: 10,
            artifacts: None,
        });
        assert_eq!(event_summary(&event), "exit=42");

        let event = make_event(EventKind::CheckpointCreated {
            checkpoint_id: "c1".to_string(),
            label: "snap".to_string(),
            created_at_ms: 1,
            files: vec![],
            auto: false,
            tool_name: None,
        });
        assert!(event_summary(&event).contains("snap"));
    }

    #[test]
    fn truncate_returns_input_when_short() {
        assert_eq!(truncate("short", 10), "short".to_string());
    }
}
