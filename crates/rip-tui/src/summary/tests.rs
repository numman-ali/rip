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
            EventKind::ContinuityContextSelectionDecided {
                run_session_id: "s1".to_string(),
                message_id: "m1".to_string(),
                compiler_id: "rip.context_compiler.v1".to_string(),
                compiler_strategy: "recent_messages_v1".to_string(),
                limits: serde_json::json!({ "recent_messages_v1_limit": 16 }),
                compaction_checkpoint: None,
                compaction_checkpoints: Vec::new(),
                resets: Vec::new(),
                reason: Some(serde_json::json!({
                    "selected": "recent_messages_v1",
                    "cause": "test",
                })),
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
            "continuity_context_selection_decided",
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
fn event_summary_uses_provider_payload_type_when_event_name_is_missing() {
    let event = make_event(EventKind::ProviderEvent {
        provider: "openresponses".to_string(),
        status: ProviderEventStatus::Event,
        event_name: None,
        data: Some(serde_json::json!({
            "type": "response.reasoning.delta",
            "delta": "step"
        })),
        raw: None,
        errors: vec![],
        response_errors: vec![],
    });
    assert_eq!(event_summary(&event), "response.reasoning.delta");
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
fn event_summary_covers_empty_suffix_and_fallback_branches() {
    let created = event_summary(&make_event(EventKind::ContinuityCreated {
        workspace: "/workspace".to_string(),
        title: Some(String::new()),
    }));
    assert!(created.contains("/workspace"));

    let selection = event_summary(&make_event(EventKind::ContinuityContextSelectionDecided {
        run_session_id: "run-123".to_string(),
        message_id: "m1".to_string(),
        compiler_id: "rip.context_compiler.v1".to_string(),
        compiler_strategy: "recent_messages_v1".to_string(),
        limits: serde_json::json!({}),
        compaction_checkpoint: None,
        compaction_checkpoints: Vec::new(),
        resets: Vec::new(),
        reason: None,
        actor_id: "user".to_string(),
        origin: "cli".to_string(),
    }));
    assert!(selection.contains("ckpt_to_seq=none"));

    let cursor_prev_empty =
        event_summary(&make_event(EventKind::ContinuityProviderCursorUpdated {
            provider: "openresponses".to_string(),
            endpoint: None,
            model: None,
            cursor: Some(serde_json::json!({"previous_response_id": ""})),
            action: "set".to_string(),
            reason: None,
            run_session_id: None,
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
        }));
    assert!(cursor_prev_empty.contains("cursor=set"));

    let headers_without_id = event_summary(&make_event(EventKind::OpenResponsesResponseHeaders {
        request_index: 7,
        status: 204,
        request_id: Some(String::new()),
        content_type: None,
    }));
    assert_eq!(headers_without_id, "req=7 status=204");
}

#[test]
fn truncate_returns_input_when_short() {
    assert_eq!(truncate("short", 10), "short".to_string());
}

#[test]
fn event_type_maps_remaining_variants() {
    let cases = [
        (
            EventKind::ContinuityCompactionCheckpointCreated {
                checkpoint_id: "ckpt-1".to_string(),
                cut_rule_id: "stride_messages_v1".to_string(),
                summary_kind: "cumulative_v1".to_string(),
                summary_artifact_id: "artifact-1".to_string(),
                from_seq: 1,
                from_message_id: Some("m1".to_string()),
                to_seq: 2,
                to_message_id: Some("m2".to_string()),
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
            "continuity_compaction_checkpoint_created",
        ),
        (
            EventKind::ContinuityCompactionAutoScheduleDecided {
                decision_id: "d1".to_string(),
                policy_id: "policy".to_string(),
                decision: "run".to_string(),
                execute: true,
                stride_messages: 8,
                max_new_checkpoints: 2,
                block_on_inflight: false,
                message_count: 32,
                cut_rule_id: "stride_messages_v1".to_string(),
                planned: Vec::new(),
                job_id: None,
                job_kind: None,
                reason: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
            "continuity_compaction_auto_schedule_decided",
        ),
        (
            EventKind::ContinuityHandoffCreated {
                from_thread_id: "thread-1".to_string(),
                from_seq: 9,
                from_message_id: Some("m9".to_string()),
                summary_artifact_id: None,
                summary_markdown: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
            "continuity_handoff_created",
        ),
        (
            EventKind::OpenResponsesRequest {
                endpoint: "https://api.openai.com/v1/responses".to_string(),
                model: Some("gpt-5".to_string()),
                request_index: 0,
                kind: "response.create".to_string(),
                body_artifact_id: "a".repeat(64),
                body_bytes: 10,
                total_bytes: 12,
                truncated: true,
            },
            "openresponses_request",
        ),
        (
            EventKind::OpenResponsesRequestStarted {
                endpoint: "https://api.openai.com/v1/responses".to_string(),
                model: Some("gpt-5".to_string()),
                request_index: 0,
                kind: "response.create".to_string(),
            },
            "openresponses_request_started",
        ),
        (
            EventKind::OpenResponsesResponseHeaders {
                request_index: 0,
                status: 200,
                request_id: Some("req_123".to_string()),
                content_type: Some("text/event-stream".to_string()),
            },
            "openresponses_response_headers",
        ),
        (
            EventKind::OpenResponsesResponseFirstByte { request_index: 0 },
            "openresponses_response_first_byte",
        ),
        (
            EventKind::ToolTaskSpawned {
                task_id: "task-1".to_string(),
                tool_name: "shell".to_string(),
                args: serde_json::json!({}),
                cwd: None,
                title: None,
                execution_mode: rip_kernel::ToolTaskExecutionMode::Pipes,
                origin_session_id: None,
                artifacts: None,
            },
            "tool_task_spawned",
        ),
        (
            EventKind::ToolTaskStatus {
                task_id: "task-1".to_string(),
                status: rip_kernel::ToolTaskStatus::Running,
                exit_code: None,
                started_at_ms: None,
                ended_at_ms: None,
                artifacts: None,
                error: None,
            },
            "tool_task_status",
        ),
        (
            EventKind::ToolTaskCancelRequested {
                task_id: "task-1".to_string(),
                reason: "stop".to_string(),
            },
            "tool_task_cancel_requested",
        ),
        (
            EventKind::ToolTaskCancelled {
                task_id: "task-1".to_string(),
                reason: "stop".to_string(),
                wall_time_ms: Some(10),
            },
            "tool_task_cancelled",
        ),
        (
            EventKind::ToolTaskOutputDelta {
                task_id: "task-1".to_string(),
                stream: rip_kernel::ToolTaskStream::Stdout,
                chunk: "out".to_string(),
                artifacts: None,
            },
            "tool_task_output_delta",
        ),
        (
            EventKind::ToolTaskStdinWritten {
                task_id: "task-1".to_string(),
                chunk_b64: "aGk=".to_string(),
            },
            "tool_task_stdin_written",
        ),
        (
            EventKind::ToolTaskResized {
                task_id: "task-1".to_string(),
                rows: 24,
                cols: 80,
            },
            "tool_task_resized",
        ),
        (
            EventKind::ToolTaskSignalled {
                task_id: "task-1".to_string(),
                signal: "TERM".to_string(),
            },
            "tool_task_signalled",
        ),
    ];

    for (kind, expected) in cases {
        assert_eq!(event_type(&make_event(kind)), expected);
    }
}

#[test]
fn event_summary_formats_remaining_variants() {
    let created = event_summary(&make_event(EventKind::ContinuityCreated {
        workspace: "/workspace".to_string(),
        title: Some("My Thread".to_string()),
    }));
    assert!(created.contains("My Thread"));

    let message = event_summary(&make_event(EventKind::ContinuityMessageAppended {
        actor_id: "user".to_string(),
        origin: "cli".to_string(),
        content: "Ship the UI".to_string(),
    }));
    assert!(message.contains("Ship the UI"));

    let run_spawned = event_summary(&make_event(EventKind::ContinuityRunSpawned {
        run_session_id: "run-123".to_string(),
        message_id: "m1".to_string(),
        actor_id: None,
        origin: None,
    }));
    assert!(run_spawned.contains("run=run-123"));

    let selection = event_summary(&make_event(EventKind::ContinuityContextSelectionDecided {
        run_session_id: "run-123".to_string(),
        message_id: "m1".to_string(),
        compiler_id: "rip.context_compiler.v1".to_string(),
        compiler_strategy: "recent_messages_v1".to_string(),
        limits: serde_json::json!({}),
        compaction_checkpoint: Some(rip_kernel::ContextSelectionCompactionCheckpointV1 {
            checkpoint_id: "ckpt-1".to_string(),
            summary_kind: "cumulative_v1".to_string(),
            summary_artifact_id: "artifact-1".to_string(),
            to_seq: 42,
        }),
        compaction_checkpoints: Vec::new(),
        resets: vec![rip_kernel::ContextSelectionResetV1 {
            input: "retry".to_string(),
            action: "drop_cursor".to_string(),
            reason: "test".to_string(),
            ref_: None,
        }],
        reason: None,
        actor_id: "user".to_string(),
        origin: "cli".to_string(),
    }));
    assert!(selection.contains("ckpt_to_seq=42"));
    assert!(selection.contains("resets=1"));

    let compiled = event_summary(&make_event(EventKind::ContinuityContextCompiled {
        run_session_id: "run-123".to_string(),
        bundle_artifact_id: "artifact-bundle".to_string(),
        compiler_id: "rip.context_compiler.v1".to_string(),
        compiler_strategy: "recent_messages_v1".to_string(),
        from_seq: 1,
        from_message_id: None,
        actor_id: "user".to_string(),
        origin: "cli".to_string(),
    }));
    assert!(compiled.contains("bundle=artifact-bundle"));

    let cursor_prev = event_summary(&make_event(EventKind::ContinuityProviderCursorUpdated {
        provider: "openresponses".to_string(),
        endpoint: None,
        model: None,
        cursor: Some(serde_json::json!({"previous_response_id": "resp_1234567890"})),
        action: "rotate".to_string(),
        reason: None,
        run_session_id: None,
        actor_id: "user".to_string(),
        origin: "cli".to_string(),
    }));
    assert!(cursor_prev.contains("prev=resp_1234567890"));

    let cursor_set = event_summary(&make_event(EventKind::ContinuityProviderCursorUpdated {
        provider: "openresponses".to_string(),
        endpoint: None,
        model: None,
        cursor: Some(serde_json::json!({"cursor": "present"})),
        action: "set".to_string(),
        reason: None,
        run_session_id: None,
        actor_id: "user".to_string(),
        origin: "cli".to_string(),
    }));
    assert!(cursor_set.contains("cursor=set"));

    let cursor_none = event_summary(&make_event(EventKind::ContinuityProviderCursorUpdated {
        provider: "openresponses".to_string(),
        endpoint: None,
        model: None,
        cursor: None,
        action: "clear".to_string(),
        reason: None,
        run_session_id: None,
        actor_id: "user".to_string(),
        origin: "cli".to_string(),
    }));
    assert!(cursor_none.contains("cursor=none"));

    let checkpoint = event_summary(&make_event(
        EventKind::ContinuityCompactionCheckpointCreated {
            checkpoint_id: "ckpt-1".to_string(),
            cut_rule_id: "stride_messages_v1".to_string(),
            summary_kind: "cumulative_v1".to_string(),
            summary_artifact_id: "artifact-1".to_string(),
            from_seq: 1,
            from_message_id: None,
            to_seq: 9,
            to_message_id: None,
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
        },
    ));
    assert!(checkpoint.contains("to_seq=9"));

    let auto_with_job = event_summary(&make_event(
        EventKind::ContinuityCompactionAutoScheduleDecided {
            decision_id: "d1".to_string(),
            policy_id: "policy".to_string(),
            decision: "run".to_string(),
            execute: true,
            stride_messages: 8,
            max_new_checkpoints: 1,
            block_on_inflight: false,
            message_count: 16,
            cut_rule_id: "stride_messages_v1".to_string(),
            planned: Vec::new(),
            job_id: Some("job-1".to_string()),
            job_kind: Some("compaction".to_string()),
            reason: None,
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
        },
    ));
    assert!(auto_with_job.contains("job=job-1"));

    let auto_without_job = event_summary(&make_event(
        EventKind::ContinuityCompactionAutoScheduleDecided {
            decision_id: "d1".to_string(),
            policy_id: "policy".to_string(),
            decision: "skip".to_string(),
            execute: false,
            stride_messages: 8,
            max_new_checkpoints: 1,
            block_on_inflight: false,
            message_count: 16,
            cut_rule_id: "stride_messages_v1".to_string(),
            planned: Vec::new(),
            job_id: None,
            job_kind: None,
            reason: None,
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
        },
    ));
    assert!(auto_without_job.contains("decision=skip"));

    for summary in [
        event_summary(&make_event(EventKind::ContinuityJobSpawned {
            job_id: "job-1".to_string(),
            job_kind: "compaction".to_string(),
            details: None,
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
        })),
        event_summary(&make_event(EventKind::ContinuityJobEnded {
            job_id: "job-1".to_string(),
            job_kind: "compaction".to_string(),
            status: "completed".to_string(),
            result: None,
            error: None,
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
        })),
        event_summary(&make_event(EventKind::ContinuityRunEnded {
            run_session_id: "run-1".to_string(),
            message_id: "m1".to_string(),
            reason: "done".to_string(),
            actor_id: None,
            origin: None,
        })),
        event_summary(&make_event(EventKind::ContinuityToolSideEffects {
            run_session_id: "run-1".to_string(),
            tool_id: "tool-1".to_string(),
            tool_name: "write".to_string(),
            affected_paths: Some(vec!["a.txt".to_string()]),
            checkpoint_id: None,
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
        })),
        event_summary(&make_event(EventKind::ContinuityToolSideEffects {
            run_session_id: "run-1".to_string(),
            tool_id: "tool-1".to_string(),
            tool_name: "write".to_string(),
            affected_paths: None,
            checkpoint_id: None,
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
        })),
        event_summary(&make_event(EventKind::ContinuityBranched {
            parent_thread_id: "thread-1".to_string(),
            parent_seq: 7,
            parent_message_id: None,
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
        })),
        event_summary(&make_event(EventKind::ContinuityHandoffCreated {
            from_thread_id: "thread-1".to_string(),
            from_seq: 9,
            from_message_id: None,
            summary_artifact_id: None,
            summary_markdown: None,
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
        })),
    ] {
        assert!(!summary.is_empty());
    }

    assert_eq!(
        event_summary(&make_event(EventKind::ToolFailed {
            tool_id: "tool-1".to_string(),
            error: "boom".to_string(),
        })),
        "\"boom\""
    );

    assert_eq!(
        event_summary(&make_event(EventKind::OpenResponsesRequest {
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            model: Some("gpt-5".to_string()),
            request_index: 3,
            kind: "response.create".to_string(),
            body_artifact_id: "a".repeat(64),
            body_bytes: 11,
            total_bytes: 20,
            truncated: true,
        })),
        "req=3 model=gpt-5 bytes=11/20 (truncated)"
    );
    assert_eq!(
        event_summary(&make_event(EventKind::OpenResponsesRequestStarted {
            endpoint: "https://api.openai.com/v1/responses".to_string(),
            model: None,
            request_index: 2,
            kind: "response.create".to_string(),
        })),
        "req=2 model=<unset> (started)"
    );
    assert_eq!(
        event_summary(&make_event(EventKind::OpenResponsesResponseHeaders {
            request_index: 1,
            status: 202,
            request_id: Some("req_1234567890".to_string()),
            content_type: None,
        })),
        "req=1 status=202 id=req_1234567890"
    );
    assert_eq!(
        event_summary(&make_event(EventKind::OpenResponsesResponseFirstByte {
            request_index: 4
        })),
        "req=4 (first_byte)"
    );
    assert_eq!(
        event_summary(&make_event(EventKind::ToolTaskSpawned {
            task_id: "task-1".to_string(),
            tool_name: "shell".to_string(),
            args: serde_json::json!({}),
            cwd: None,
            title: None,
            execution_mode: rip_kernel::ToolTaskExecutionMode::Pipes,
            origin_session_id: None,
            artifacts: None,
        })),
        "shell"
    );
    assert_eq!(
        event_summary(&make_event(EventKind::ToolTaskStatus {
            task_id: "task-1".to_string(),
            status: rip_kernel::ToolTaskStatus::Cancelled,
            exit_code: None,
            started_at_ms: None,
            ended_at_ms: None,
            artifacts: None,
            error: None,
        })),
        "cancelled"
    );
    assert_eq!(
        event_summary(&make_event(EventKind::ToolTaskCancelRequested {
            task_id: "task-1".to_string(),
            reason: "user cancel".to_string(),
        })),
        "\"user cancel\""
    );
    assert_eq!(
        event_summary(&make_event(EventKind::ToolTaskCancelled {
            task_id: "task-1".to_string(),
            reason: "done".to_string(),
            wall_time_ms: None,
        })),
        "\"done\""
    );
    assert_eq!(
        event_summary(&make_event(EventKind::ToolTaskOutputDelta {
            task_id: "task-1".to_string(),
            stream: rip_kernel::ToolTaskStream::Stdout,
            chunk: "output".to_string(),
            artifacts: None,
        })),
        "\"output\""
    );
    assert_eq!(
        event_summary(&make_event(EventKind::ToolTaskStdinWritten {
            task_id: "task-1".to_string(),
            chunk_b64: "aGk=".to_string(),
        })),
        "\"aGk=\""
    );
    assert_eq!(
        event_summary(&make_event(EventKind::ToolTaskResized {
            task_id: "task-1".to_string(),
            rows: 24,
            cols: 80,
        })),
        "24x80"
    );
    assert_eq!(
        event_summary(&make_event(EventKind::ToolTaskSignalled {
            task_id: "task-1".to_string(),
            signal: "TERM".to_string(),
        })),
        "TERM"
    );
}
