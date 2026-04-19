use rip_kernel::{Event, EventKind, ProviderEventStatus};

pub fn event_type(event: &Event) -> &'static str {
    match &event.kind {
        EventKind::SessionStarted { .. } => "session_started",
        EventKind::OutputTextDelta { .. } => "output_text_delta",
        EventKind::SessionEnded { .. } => "session_ended",
        EventKind::ContinuityCreated { .. } => "continuity_created",
        EventKind::ContinuityMessageAppended { .. } => "continuity_message_appended",
        EventKind::ContinuityRunSpawned { .. } => "continuity_run_spawned",
        EventKind::ContinuityContextSelectionDecided { .. } => {
            "continuity_context_selection_decided"
        }
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
        EventKind::OpenResponsesRequest { .. } => "openresponses_request",
        EventKind::OpenResponsesRequestStarted { .. } => "openresponses_request_started",
        EventKind::OpenResponsesResponseHeaders { .. } => "openresponses_response_headers",
        EventKind::OpenResponsesResponseFirstByte { .. } => "openresponses_response_first_byte",
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
        EventKind::ContinuityContextSelectionDecided {
            run_session_id,
            compiler_strategy,
            compaction_checkpoint,
            resets,
            ..
        } => {
            let ckpt = compaction_checkpoint
                .as_ref()
                .map(|c| c.to_seq.to_string())
                .unwrap_or_else(|| "none".to_string());
            format!(
                "run={} ({}) ckpt_to_seq={} resets={}",
                truncate(run_session_id, 16),
                truncate(compiler_strategy, 32),
                ckpt,
                resets.len()
            )
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
        } => {
            let error_count = errors.len().saturating_add(response_errors.len());
            if error_count > 0 && *status != ProviderEventStatus::Done {
                match status {
                    ProviderEventStatus::InvalidJson => format!("invalid_json ({error_count})"),
                    _ => format!("error ({error_count})"),
                }
            } else {
                match status {
                    ProviderEventStatus::Event => {
                        event_name.as_deref().unwrap_or("event").to_string()
                    }
                    ProviderEventStatus::Done => "done".to_string(),
                    ProviderEventStatus::InvalidJson => "invalid_json".to_string(),
                }
            }
        }
        EventKind::OpenResponsesRequest {
            request_index,
            model,
            body_bytes,
            total_bytes,
            truncated,
            ..
        } => {
            let model = model.as_deref().unwrap_or("<unset>");
            if *truncated {
                format!(
                    "req={} model={} bytes={}/{} (truncated)",
                    request_index,
                    truncate(model, 40),
                    body_bytes,
                    total_bytes
                )
            } else {
                format!(
                    "req={} model={} bytes={}",
                    request_index,
                    truncate(model, 40),
                    body_bytes
                )
            }
        }
        EventKind::OpenResponsesRequestStarted {
            request_index,
            model,
            ..
        } => {
            let model = model.as_deref().unwrap_or("<unset>");
            format!(
                "req={} model={} (started)",
                request_index,
                truncate(model, 40)
            )
        }
        EventKind::OpenResponsesResponseHeaders {
            request_index,
            status,
            request_id,
            ..
        } => {
            let suffix = request_id
                .as_deref()
                .filter(|id| !id.is_empty())
                .map(|id| format!(" id={}", truncate(id, 16)))
                .unwrap_or_default();
            format!("req={} status={}{}", request_index, status, suffix)
        }
        EventKind::OpenResponsesResponseFirstByte { request_index } => {
            format!("req={} (first_byte)", request_index)
        }
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
mod tests;
