//! Frame-driven state ingestion.
//!
//! Everything RIP does flows through the continuity event log; the
//! TUI derives its state from that stream and nothing else. `update`
//! is the single entry point the driver calls for each frame: it
//! updates the run-level timing snapshot, opens the recovery overlay
//! on the first provider error, and dispatches to `ingest_derived_state`
//! for the ambient run-status maps (tools, tasks, jobs, context). The
//! canvas model is a separate consumer (`CanvasModel::ingest`) called
//! at the tail of `update`, so the structured transcript and the
//! ambient summaries share a single source of truth per frame.

use std::collections::BTreeSet;

use rip_kernel::{Event, EventKind, ProviderEventStatus, ToolTaskExecutionMode, ToolTaskStatus};
use serde_json::Value;

use super::{
    ContextStatus, ContextSummary, JobStatus, JobSummary, Overlay, TaskSummary, ToolStatus,
    ToolSummary, TuiState,
};

impl TuiState {
    pub fn update(&mut self, event: Event) {
        if self.session_id.is_none() {
            self.session_id = Some(event.session_id.clone());
        }

        self.last_event_ms = Some(event.timestamp_ms);
        if is_error_event(&event.kind) {
            self.last_error_seq = Some(event.seq);
            self.awaiting_response = false;
            self.pending_prompt = None;
            self.clear_status_message();
            // C.10: the first provider error for a run auto-opens the
            // recovery overlay so the operator has a one-keystroke
            // path to retry / rotate / switch / X-ray. Only push when
            // nothing else already owns the overlay stack — if the
            // user is mid-palette or mid-detail view we keep their
            // context; the error chip persists on the activity strip
            // and they can reach the overlay via the palette.
            if matches!(self.overlay_stack.top(), Overlay::None) {
                self.overlay_stack
                    .set(Overlay::ErrorRecovery { seq: event.seq });
            }
        }

        match &event.kind {
            EventKind::SessionStarted { input: _ } => {
                if self.start_ms.is_none() {
                    self.start_ms = Some(event.timestamp_ms);
                }
                self.awaiting_response = true;
                // `canvas.ingest` below handles both sides (pending prompt
                // skipped vs. implied UserTurn materialized from the frame).
                if self.pending_prompt.is_some() {
                    self.pending_prompt = None;
                }
            }
            EventKind::ToolTaskSpawned { .. } => {
                if self.start_ms.is_none() {
                    self.start_ms = Some(event.timestamp_ms);
                }
                if self.awaiting_response {
                    self.set_status_message("working...");
                }
            }
            EventKind::OpenResponsesRequestStarted {
                endpoint, model, ..
            } => {
                if self.openresponses_request_started_ms.is_none() {
                    self.openresponses_request_started_ms = Some(event.timestamp_ms);
                }
                self.openresponses_endpoint = Some(endpoint.clone());
                self.openresponses_model = model.clone();
                if self.awaiting_response {
                    self.set_status_message("waiting for model...");
                }
            }
            EventKind::OpenResponsesResponseHeaders { .. } => {
                if self.openresponses_response_headers_ms.is_none() {
                    self.openresponses_response_headers_ms = Some(event.timestamp_ms);
                }
            }
            EventKind::OpenResponsesResponseFirstByte { .. } => {
                if self.openresponses_response_first_byte_ms.is_none() {
                    self.openresponses_response_first_byte_ms = Some(event.timestamp_ms);
                }
            }
            EventKind::OutputTextDelta { delta: _ } => {
                if self.first_output_ms.is_none() {
                    self.first_output_ms = Some(event.timestamp_ms);
                }
                self.awaiting_response = false;
                self.clear_status_message();
                // Canvas ingest owns the delta → StreamCollector → AgentTurn
                // plumbing (B.5); derived-state layer only tracks timings.
            }
            EventKind::SessionEnded { .. } => {
                if self.end_ms.is_none() {
                    self.end_ms = Some(event.timestamp_ms);
                }
                self.awaiting_response = false;
                self.pending_prompt = None;
                self.clear_status_message();
            }
            EventKind::ToolTaskStatus { status, .. } => {
                if self.end_ms.is_none()
                    && matches!(
                        status,
                        rip_kernel::ToolTaskStatus::Exited
                            | rip_kernel::ToolTaskStatus::Cancelled
                            | rip_kernel::ToolTaskStatus::Failed
                    )
                {
                    self.end_ms = Some(event.timestamp_ms);
                }
                if matches!(
                    status,
                    rip_kernel::ToolTaskStatus::Exited
                        | rip_kernel::ToolTaskStatus::Cancelled
                        | rip_kernel::ToolTaskStatus::Failed
                ) {
                    self.awaiting_response = false;
                    self.clear_status_message();
                }
            }
            EventKind::ProviderEvent { provider, .. } => {
                if provider == "openresponses"
                    && self.openresponses_first_provider_event_ms.is_none()
                {
                    self.openresponses_first_provider_event_ms = Some(event.timestamp_ms);
                }
                if self.awaiting_response {
                    self.set_status_message("working...");
                }
            }
            _ => {}
        }

        self.ingest_derived_state(&event);
        self.canvas.ingest(&event);

        let seq = event.seq;
        self.frames.push(event);
        if self.auto_follow || self.selected_seq.is_none() {
            self.selected_seq = Some(seq);
        }
    }

    pub fn selected_event(&self) -> Option<&Event> {
        let seq = self.selected_seq?;
        self.frames.get_by_seq(seq)
    }

    pub fn ttft_ms(&self) -> Option<u64> {
        Some(self.first_output_ms?.saturating_sub(self.start_ms?))
    }

    pub fn e2e_ms(&self) -> Option<u64> {
        Some(self.end_ms?.saturating_sub(self.start_ms?))
    }

    pub fn openresponses_headers_ms(&self) -> Option<u64> {
        Some(
            self.openresponses_response_headers_ms?
                .saturating_sub(self.openresponses_request_started_ms?),
        )
    }

    pub fn openresponses_first_byte_ms(&self) -> Option<u64> {
        Some(
            self.openresponses_response_first_byte_ms?
                .saturating_sub(self.openresponses_request_started_ms?),
        )
    }

    pub fn openresponses_first_provider_event_ms(&self) -> Option<u64> {
        Some(
            self.openresponses_first_provider_event_ms?
                .saturating_sub(self.openresponses_request_started_ms?),
        )
    }

    fn ingest_derived_state(&mut self, event: &Event) {
        match &event.kind {
            EventKind::ToolStarted {
                tool_id,
                name,
                args,
                ..
            } => {
                let entry = ToolSummary {
                    tool_id: tool_id.clone(),
                    name: name.clone(),
                    args: args.clone(),
                    started_seq: event.seq,
                    started_at_ms: event.timestamp_ms,
                    status: ToolStatus::Running,
                    stdout_preview: String::new(),
                    stderr_preview: String::new(),
                    artifact_ids: BTreeSet::new(),
                };
                self.tools.insert(tool_id.clone(), entry);
            }
            EventKind::ToolStdout { tool_id, chunk } => {
                if let Some(tool) = self.tools.get_mut(tool_id) {
                    push_preview(&mut tool.stdout_preview, chunk, self.max_preview_bytes);
                }
            }
            EventKind::ToolStderr { tool_id, chunk } => {
                if let Some(tool) = self.tools.get_mut(tool_id) {
                    push_preview(&mut tool.stderr_preview, chunk, self.max_preview_bytes);
                }
            }
            EventKind::ToolEnded {
                tool_id,
                exit_code,
                duration_ms,
                artifacts,
            } => {
                if let Some(tool) = self.tools.get_mut(tool_id) {
                    tool.status = ToolStatus::Ended {
                        exit_code: *exit_code,
                        duration_ms: *duration_ms,
                    };
                    if let Some(value) = artifacts {
                        for artifact_id in extract_artifact_ids(value) {
                            tool.artifact_ids.insert(artifact_id.clone());
                            self.artifacts.insert(artifact_id);
                        }
                    }
                }
            }
            EventKind::ToolFailed { tool_id, error } => {
                if let Some(tool) = self.tools.get_mut(tool_id) {
                    tool.status = ToolStatus::Failed {
                        error: error.clone(),
                    };
                }
            }
            EventKind::ToolTaskSpawned {
                task_id,
                tool_name,
                args,
                cwd,
                title,
                execution_mode,
                artifacts,
                ..
            } => {
                let mut artifact_ids = BTreeSet::new();
                if let Some(value) = artifacts {
                    for artifact_id in extract_artifact_ids(value) {
                        artifact_ids.insert(artifact_id.clone());
                        self.artifacts.insert(artifact_id);
                    }
                }
                let entry = TaskSummary {
                    task_id: task_id.clone(),
                    tool_name: tool_name.clone(),
                    args: args.clone(),
                    cwd: cwd.clone(),
                    title: title.clone(),
                    execution_mode: *execution_mode,
                    status: ToolTaskStatus::Queued,
                    exit_code: None,
                    started_at_ms: None,
                    ended_at_ms: None,
                    error: None,
                    stdout_preview: String::new(),
                    stderr_preview: String::new(),
                    pty_preview: String::new(),
                    artifact_ids,
                };
                self.tasks.insert(task_id.clone(), entry);
            }
            EventKind::ToolTaskStatus {
                task_id,
                status,
                exit_code,
                started_at_ms,
                ended_at_ms,
                artifacts,
                error,
            } => {
                let entry = self
                    .tasks
                    .entry(task_id.clone())
                    .or_insert_with(|| TaskSummary {
                        task_id: task_id.clone(),
                        tool_name: "unknown".to_string(),
                        args: Value::Null,
                        cwd: None,
                        title: None,
                        execution_mode: ToolTaskExecutionMode::Pipes,
                        status: *status,
                        exit_code: *exit_code,
                        started_at_ms: *started_at_ms,
                        ended_at_ms: *ended_at_ms,
                        error: error.clone(),
                        stdout_preview: String::new(),
                        stderr_preview: String::new(),
                        pty_preview: String::new(),
                        artifact_ids: BTreeSet::new(),
                    });
                entry.status = *status;
                entry.exit_code = *exit_code;
                entry.started_at_ms = *started_at_ms;
                entry.ended_at_ms = *ended_at_ms;
                entry.error = error.clone();
                if let Some(value) = artifacts {
                    for artifact_id in extract_artifact_ids(value) {
                        entry.artifact_ids.insert(artifact_id.clone());
                        self.artifacts.insert(artifact_id);
                    }
                }
            }
            EventKind::ToolTaskOutputDelta {
                task_id,
                stream,
                chunk,
                artifacts,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id) {
                    match stream {
                        rip_kernel::ToolTaskStream::Stdout => {
                            push_preview(&mut task.stdout_preview, chunk, self.max_preview_bytes);
                        }
                        rip_kernel::ToolTaskStream::Stderr => {
                            push_preview(&mut task.stderr_preview, chunk, self.max_preview_bytes);
                        }
                        rip_kernel::ToolTaskStream::Pty => {
                            push_preview(&mut task.pty_preview, chunk, self.max_preview_bytes);
                        }
                    }
                    if let Some(value) = artifacts {
                        for artifact_id in extract_artifact_ids(value) {
                            task.artifact_ids.insert(artifact_id.clone());
                            self.artifacts.insert(artifact_id);
                        }
                    }
                }
            }
            EventKind::ContinuityJobSpawned {
                job_id, job_kind, ..
            } => {
                self.jobs.insert(
                    job_id.clone(),
                    JobSummary {
                        job_id: job_id.clone(),
                        job_kind: job_kind.clone(),
                        status: JobStatus::Running,
                    },
                );
            }
            EventKind::ContinuityJobEnded {
                job_id,
                job_kind,
                status,
                error,
                ..
            } => {
                self.jobs.insert(
                    job_id.clone(),
                    JobSummary {
                        job_id: job_id.clone(),
                        job_kind: job_kind.clone(),
                        status: JobStatus::Ended {
                            status: status.clone(),
                            error: error.clone(),
                        },
                    },
                );
            }
            EventKind::ContinuityContextSelectionDecided {
                run_session_id,
                compiler_strategy,
                ..
            } => {
                self.context = Some(ContextSummary {
                    run_session_id: run_session_id.clone(),
                    compiler_strategy: compiler_strategy.clone(),
                    status: ContextStatus::Selecting,
                    bundle_artifact_id: None,
                });
            }
            EventKind::ContinuityContextCompiled {
                run_session_id,
                bundle_artifact_id,
                compiler_strategy,
                ..
            } => {
                self.artifacts.insert(bundle_artifact_id.clone());
                self.context = Some(ContextSummary {
                    run_session_id: run_session_id.clone(),
                    compiler_strategy: compiler_strategy.clone(),
                    status: ContextStatus::Compiled,
                    bundle_artifact_id: Some(bundle_artifact_id.clone()),
                });
            }
            EventKind::ContinuityCompactionCheckpointCreated {
                summary_artifact_id,
                ..
            } => {
                self.artifacts.insert(summary_artifact_id.clone());
            }
            EventKind::OpenResponsesRequest {
                body_artifact_id, ..
            } => {
                self.artifacts.insert(body_artifact_id.clone());
            }
            _ => {}
        }
    }
}

pub(super) fn is_error_event(kind: &EventKind) -> bool {
    match kind {
        EventKind::ToolFailed { .. } => true,
        EventKind::CheckpointFailed { .. } => true,
        EventKind::ToolTaskStatus { status, .. } => matches!(status, ToolTaskStatus::Failed),
        EventKind::ProviderEvent {
            status,
            errors,
            response_errors,
            ..
        } => {
            *status == ProviderEventStatus::InvalidJson
                || (*status != ProviderEventStatus::Done
                    && (!errors.is_empty() || !response_errors.is_empty()))
        }
        _ => false,
    }
}

pub(super) fn push_preview(target: &mut String, chunk: &str, max_len: usize) {
    if chunk.is_empty() {
        return;
    }
    target.push_str(chunk);
    if target.len() <= max_len {
        return;
    }
    let keep = max_len / 2;
    let mut start = target.len().saturating_sub(keep);
    while start < target.len() && !target.is_char_boundary(start) {
        start += 1;
    }
    *target = target[start..].to_string();
}

pub(super) fn extract_artifact_ids(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    extract_artifact_ids_inner(value, &mut out);
    out
}

fn extract_artifact_ids_inner(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
        Value::String(s) => {
            if looks_like_artifact_id(s) {
                out.push(s.clone());
            }
        }
        Value::Array(items) => {
            for item in items {
                extract_artifact_ids_inner(item, out);
            }
        }
        Value::Object(map) => {
            for v in map.values() {
                extract_artifact_ids_inner(v, out);
            }
        }
    }
}

pub(super) fn looks_like_artifact_id(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}
