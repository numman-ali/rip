use std::collections::{BTreeMap, BTreeSet};

use rip_kernel::{Event, EventKind, ProviderEventStatus, ToolTaskExecutionMode, ToolTaskStatus};
use serde_json::Value;

use crate::FrameStore;

const DEFAULT_MAX_FRAMES: usize = 10_000;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 1_000_000;
const DEFAULT_MAX_PREVIEW_BYTES: usize = 8_192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Overlay {
    None,
    Activity,
    ToolDetail { tool_id: String },
    TaskList,
    TaskDetail { task_id: String },
    ErrorDetail { seq: u64 },
    StallDetail,
}

#[derive(Debug, Clone)]
pub enum ToolStatus {
    Running,
    Ended { exit_code: i32, duration_ms: u64 },
    Failed { error: String },
}

#[derive(Debug, Clone)]
pub struct ToolSummary {
    pub tool_id: String,
    pub name: String,
    pub args: Value,
    pub started_seq: u64,
    pub started_at_ms: u64,
    pub status: ToolStatus,
    pub stdout_preview: String,
    pub stderr_preview: String,
    pub artifact_ids: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct TaskSummary {
    pub task_id: String,
    pub tool_name: String,
    pub args: Value,
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub execution_mode: ToolTaskExecutionMode,
    pub status: ToolTaskStatus,
    pub exit_code: Option<i32>,
    pub started_at_ms: Option<u64>,
    pub ended_at_ms: Option<u64>,
    pub error: Option<String>,
    pub stdout_preview: String,
    pub stderr_preview: String,
    pub pty_preview: String,
    pub artifact_ids: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub enum JobStatus {
    Running,
    Ended {
        status: String,
        error: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct JobSummary {
    pub job_id: String,
    pub job_kind: String,
    pub status: JobStatus,
}

#[derive(Debug, Clone)]
pub enum ContextStatus {
    Selecting,
    Compiled,
}

#[derive(Debug, Clone)]
pub struct ContextSummary {
    pub run_session_id: String,
    pub compiler_strategy: String,
    pub status: ContextStatus,
    pub bundle_artifact_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputViewMode {
    Rendered,
    Raw,
}

impl OutputViewMode {
    pub fn toggle(&mut self) {
        *self = match self {
            Self::Rendered => Self::Raw,
            Self::Raw => Self::Rendered,
        };
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rendered => "rendered",
            Self::Raw => "raw",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    DefaultDark,
    DefaultLight,
}

impl ThemeId {
    pub fn toggle(&mut self) {
        *self = match self {
            Self::DefaultDark => Self::DefaultLight,
            Self::DefaultLight => Self::DefaultDark,
        };
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::DefaultDark => "default-dark",
            Self::DefaultLight => "default-light",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TuiState {
    pub frames: FrameStore,
    pub selected_seq: Option<u64>,
    pub auto_follow: bool,
    pub output_view: OutputViewMode,
    pub theme: ThemeId,
    pub overlay: Overlay,
    pub activity_pinned: bool,
    pub now_ms: Option<u64>,
    pub session_id: Option<String>,
    pub start_ms: Option<u64>,
    pub first_output_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub openresponses_request_started_ms: Option<u64>,
    pub openresponses_response_headers_ms: Option<u64>,
    pub openresponses_response_first_byte_ms: Option<u64>,
    pub openresponses_first_provider_event_ms: Option<u64>,
    pub openresponses_endpoint: Option<String>,
    pub openresponses_model: Option<String>,
    pub output_text: String,
    pub output_truncated: bool,
    pub status_message: Option<String>,
    pub clipboard_buffer: Option<String>,
    pub tools: BTreeMap<String, ToolSummary>,
    pub tasks: BTreeMap<String, TaskSummary>,
    pub jobs: BTreeMap<String, JobSummary>,
    pub artifacts: BTreeSet<String>,
    pub context: Option<ContextSummary>,
    pub last_error_seq: Option<u64>,
    pub last_event_ms: Option<u64>,
    max_output_bytes: usize,
    max_preview_bytes: usize,
}

impl Default for TuiState {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_FRAMES, DEFAULT_MAX_OUTPUT_BYTES)
    }
}

impl TuiState {
    pub fn new(max_frames: usize, max_output_bytes: usize) -> Self {
        Self {
            frames: FrameStore::new(max_frames),
            selected_seq: None,
            auto_follow: true,
            output_view: OutputViewMode::Rendered,
            theme: ThemeId::DefaultDark,
            overlay: Overlay::None,
            activity_pinned: false,
            now_ms: None,
            session_id: None,
            start_ms: None,
            first_output_ms: None,
            end_ms: None,
            openresponses_request_started_ms: None,
            openresponses_response_headers_ms: None,
            openresponses_response_first_byte_ms: None,
            openresponses_first_provider_event_ms: None,
            openresponses_endpoint: None,
            openresponses_model: None,
            output_text: String::new(),
            output_truncated: false,
            status_message: None,
            clipboard_buffer: None,
            tools: BTreeMap::new(),
            tasks: BTreeMap::new(),
            jobs: BTreeMap::new(),
            artifacts: BTreeSet::new(),
            context: None,
            last_error_seq: None,
            last_event_ms: None,
            max_output_bytes: max_output_bytes.max(1),
            max_preview_bytes: DEFAULT_MAX_PREVIEW_BYTES,
        }
    }

    pub fn toggle_output_view(&mut self) {
        self.output_view.toggle();
    }

    pub fn toggle_theme(&mut self) {
        self.theme.toggle();
    }

    pub fn close_overlay(&mut self) {
        self.overlay = Overlay::None;
    }

    pub fn toggle_activity_overlay(&mut self) {
        self.overlay = match &self.overlay {
            Overlay::Activity => Overlay::None,
            _ => Overlay::Activity,
        };
    }

    pub fn toggle_tasks_overlay(&mut self) {
        self.overlay = match &self.overlay {
            Overlay::TaskList => Overlay::None,
            _ => Overlay::TaskList,
        };
    }

    pub fn open_selected_detail(&mut self) {
        // Prefer the most recent error, regardless of selection.
        if let Some(seq) = self.last_error_seq {
            self.overlay = match &self.overlay {
                Overlay::ErrorDetail { seq: current } if *current == seq => Overlay::None,
                _ => Overlay::ErrorDetail { seq },
            };
            return;
        }

        let Some(event) = self.selected_event() else {
            return;
        };

        let next = match &event.kind {
            EventKind::ToolStarted { tool_id, .. }
            | EventKind::ToolStdout { tool_id, .. }
            | EventKind::ToolStderr { tool_id, .. }
            | EventKind::ToolEnded { tool_id, .. }
            | EventKind::ToolFailed { tool_id, .. } => Overlay::ToolDetail {
                tool_id: tool_id.clone(),
            },
            EventKind::ToolTaskSpawned { task_id, .. }
            | EventKind::ToolTaskStatus { task_id, .. }
            | EventKind::ToolTaskOutputDelta { task_id, .. }
            | EventKind::ToolTaskCancelRequested { task_id, .. }
            | EventKind::ToolTaskCancelled { task_id, .. } => Overlay::TaskDetail {
                task_id: task_id.clone(),
            },
            _ => Overlay::None,
        };

        self.overlay = match (&self.overlay, next) {
            (Overlay::ToolDetail { tool_id: a }, Overlay::ToolDetail { tool_id: b }) if a == &b => {
                Overlay::None
            }
            (Overlay::TaskDetail { task_id: a }, Overlay::TaskDetail { task_id: b }) if a == &b => {
                Overlay::None
            }
            (_, next) => next,
        };
    }

    pub fn set_status_message(&mut self, message: impl Into<String>) {
        self.status_message = Some(message.into());
    }

    pub fn set_now_ms(&mut self, now_ms: u64) {
        self.now_ms = Some(now_ms);
    }

    pub fn is_stalled(&self, threshold_ms: u64) -> bool {
        let Some(now_ms) = self.now_ms else {
            return false;
        };
        let Some(last_ms) = self.last_event_ms else {
            return false;
        };
        now_ms.saturating_sub(last_ms) >= threshold_ms
    }

    pub fn has_error(&self) -> bool {
        self.last_error_seq.is_some()
    }

    pub fn running_tool_ids(&self) -> impl Iterator<Item = &str> {
        self.tools.iter().filter_map(|(id, tool)| {
            matches!(tool.status, ToolStatus::Running).then_some(id.as_str())
        })
    }

    pub fn running_task_ids(&self) -> impl Iterator<Item = &str> {
        self.tasks.iter().filter_map(|(id, task)| {
            matches!(
                task.status,
                ToolTaskStatus::Queued | ToolTaskStatus::Running
            )
            .then_some(id.as_str())
        })
    }

    pub fn running_job_ids(&self) -> impl Iterator<Item = &str> {
        self.jobs
            .iter()
            .filter_map(|(id, job)| matches!(job.status, JobStatus::Running).then_some(id.as_str()))
    }

    pub fn update(&mut self, event: Event) {
        if self.session_id.is_none() {
            self.session_id = Some(event.session_id.clone());
        }

        self.last_event_ms = Some(event.timestamp_ms);
        if is_error_event(&event.kind) {
            self.last_error_seq = Some(event.seq);
        }

        match &event.kind {
            EventKind::SessionStarted { input } => {
                if self.start_ms.is_none() {
                    self.start_ms = Some(event.timestamp_ms);
                }
                self.push_user_prompt(input);
            }
            EventKind::ToolTaskSpawned { .. } => {
                if self.start_ms.is_none() {
                    self.start_ms = Some(event.timestamp_ms);
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
            EventKind::OutputTextDelta { delta } => {
                if self.first_output_ms.is_none() {
                    self.first_output_ms = Some(event.timestamp_ms);
                }
                self.push_output(delta);
            }
            EventKind::SessionEnded { .. } => {
                if self.end_ms.is_none() {
                    self.end_ms = Some(event.timestamp_ms);
                }
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
            }
            EventKind::ProviderEvent { provider, .. } => {
                if provider == "openresponses"
                    && self.openresponses_first_provider_event_ms.is_none()
                {
                    self.openresponses_first_provider_event_ms = Some(event.timestamp_ms);
                }
            }
            _ => {}
        }

        self.ingest_derived_state(&event);

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

    fn push_output(&mut self, delta: &str) {
        if delta.is_empty() {
            return;
        }

        self.output_text.push_str(delta);
        if self.output_text.len() <= self.max_output_bytes {
            return;
        }

        self.output_truncated = true;
        let keep = self.max_output_bytes / 2;
        let mut start = self.output_text.len().saturating_sub(keep);
        while start < self.output_text.len() && !self.output_text.is_char_boundary(start) {
            start += 1;
        }
        self.output_text = self.output_text[start..].to_string();
    }

    fn push_user_prompt(&mut self, input: &str) {
        if input.trim().is_empty() {
            return;
        }
        // Canvas should stay conversational-first; we always show the prompt that started this run.
        self.push_output("You: ");
        self.push_output(input);
        self.push_output("\n\n");
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

fn is_error_event(kind: &EventKind) -> bool {
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
                || !errors.is_empty()
                || !response_errors.is_empty()
        }
        _ => false,
    }
}

fn push_preview(target: &mut String, chunk: &str, max_len: usize) {
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

fn extract_artifact_ids(value: &Value) -> Vec<String> {
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

fn looks_like_artifact_id(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rip_kernel::{
        CheckpointAction, Event, EventKind, ProviderEventStatus, ToolTaskExecutionMode,
        ToolTaskStatus, ToolTaskStream,
    };
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
    fn computes_ttft_and_e2e() {
        let mut state = TuiState::new(100, 1024);
        state.update(event(
            0,
            1000,
            EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        ));
        state.update(event(
            1,
            1300,
            EventKind::OutputTextDelta {
                delta: "a".to_string(),
            },
        ));
        state.update(event(
            2,
            1800,
            EventKind::SessionEnded {
                reason: "done".to_string(),
            },
        ));
        assert_eq!(state.ttft_ms(), Some(300));
        assert_eq!(state.e2e_ms(), Some(800));
    }

    #[test]
    fn update_respects_selected_seq_when_auto_follow_disabled() {
        let mut state = TuiState::new(100, 1024);
        state.auto_follow = false;
        state.selected_seq = Some(0);
        state.update(event(
            1,
            1000,
            EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        ));
        assert_eq!(state.selected_seq, Some(0));
    }

    #[test]
    fn update_sets_session_id_once() {
        let mut state = TuiState::new(100, 1024);
        state.update(event(
            0,
            1000,
            EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        ));
        state.update(Event {
            id: "e2".to_string(),
            session_id: "s2".to_string(),
            timestamp_ms: 1100,
            seq: 1,
            kind: EventKind::SessionEnded {
                reason: "done".to_string(),
            },
        });
        assert_eq!(state.session_id.as_deref(), Some("s1"));
    }

    #[test]
    fn push_output_truncates_and_flags() {
        let mut state = TuiState::new(100, 8);
        state.update(event(
            0,
            1000,
            EventKind::OutputTextDelta {
                delta: "abcdefghijk".to_string(),
            },
        ));
        assert!(state.output_truncated);
        assert!(state.output_text.len() <= 8);
    }

    #[test]
    fn push_output_ignores_empty_delta() {
        let mut state = TuiState::new(100, 1024);
        state.output_text = "keep".to_string();
        state.update(event(
            0,
            1000,
            EventKind::OutputTextDelta {
                delta: "".to_string(),
            },
        ));
        assert_eq!(state.output_text, "keep");
    }

    fn artifact(fill: char) -> String {
        std::iter::repeat_n(fill, 64).collect()
    }

    #[test]
    fn overlay_and_status_helpers_toggle_cleanly() {
        let mut state = TuiState::default();
        assert_eq!(state.output_view.as_str(), "rendered");
        assert_eq!(state.theme.as_str(), "default-dark");

        state.toggle_output_view();
        state.toggle_theme();
        assert_eq!(state.output_view.as_str(), "raw");
        assert_eq!(state.theme.as_str(), "default-light");

        state.toggle_activity_overlay();
        assert_eq!(state.overlay, Overlay::Activity);
        state.toggle_activity_overlay();
        assert_eq!(state.overlay, Overlay::None);

        state.toggle_tasks_overlay();
        assert_eq!(state.overlay, Overlay::TaskList);
        state.close_overlay();
        assert_eq!(state.overlay, Overlay::None);

        state.set_status_message("watching");
        state.set_now_ms(2_000);
        assert!(!state.is_stalled(100));
        state.last_event_ms = Some(1_500);
        assert!(state.is_stalled(400));
        assert!(!state.is_stalled(600));
    }

    #[test]
    fn open_selected_detail_prefers_errors_and_toggles_tool_and_task_details() {
        let mut state = TuiState::default();
        state.update(event(
            0,
            100,
            EventKind::ToolStarted {
                tool_id: "tool-1".to_string(),
                name: "ls".to_string(),
                args: json!({"path": "."}),
                timeout_ms: None,
            },
        ));
        state.update(event(
            1,
            110,
            EventKind::ToolTaskSpawned {
                task_id: "task-1".to_string(),
                tool_name: "shell".to_string(),
                args: json!({"cmd": "pwd"}),
                cwd: None,
                title: Some("pwd".to_string()),
                execution_mode: ToolTaskExecutionMode::Pty,
                origin_session_id: None,
                artifacts: None,
            },
        ));
        state.update(event(
            2,
            120,
            EventKind::SessionStarted {
                input: "hello".to_string(),
            },
        ));

        state.last_error_seq = Some(99);
        state.open_selected_detail();
        assert_eq!(state.overlay, Overlay::ErrorDetail { seq: 99 });
        state.open_selected_detail();
        assert_eq!(state.overlay, Overlay::None);

        state.last_error_seq = None;
        state.selected_seq = Some(0);
        state.open_selected_detail();
        assert_eq!(
            state.overlay,
            Overlay::ToolDetail {
                tool_id: "tool-1".to_string()
            }
        );
        state.open_selected_detail();
        assert_eq!(state.overlay, Overlay::None);

        state.selected_seq = Some(1);
        state.open_selected_detail();
        assert_eq!(
            state.overlay,
            Overlay::TaskDetail {
                task_id: "task-1".to_string()
            }
        );
        state.open_selected_detail();
        assert_eq!(state.overlay, Overlay::None);

        state.selected_seq = Some(2);
        state.open_selected_detail();
        assert_eq!(state.overlay, Overlay::None);
    }

    #[test]
    fn update_tracks_timings_derived_state_and_artifacts() {
        let mut state = TuiState::new(100, 1024);
        let a1 = artifact('a');
        let a2 = artifact('b');
        let a3 = artifact('c');
        let a4 = artifact('d');
        let a5 = artifact('e');
        let a6 = artifact('f');
        let a7 = artifact('1');

        for event in [
            event(
                0,
                100,
                EventKind::SessionStarted {
                    input: "hello".to_string(),
                },
            ),
            event(
                1,
                110,
                EventKind::OpenResponsesRequestStarted {
                    endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
                    model: Some("gpt-5".to_string()),
                    request_index: 0,
                    kind: "response.create".to_string(),
                },
            ),
            event(
                2,
                120,
                EventKind::OpenResponsesResponseHeaders {
                    request_index: 0,
                    status: 200,
                    request_id: Some("req_123".to_string()),
                    content_type: Some("text/event-stream".to_string()),
                },
            ),
            event(
                3,
                130,
                EventKind::OpenResponsesResponseFirstByte { request_index: 0 },
            ),
            event(
                4,
                140,
                EventKind::ProviderEvent {
                    provider: "openresponses".to_string(),
                    status: ProviderEventStatus::InvalidJson,
                    event_name: None,
                    data: None,
                    raw: Some("{".to_string()),
                    errors: vec!["bad json".to_string()],
                    response_errors: vec!["schema".to_string()],
                },
            ),
            event(
                5,
                150,
                EventKind::OutputTextDelta {
                    delta: "world".to_string(),
                },
            ),
            event(
                6,
                160,
                EventKind::ToolStarted {
                    tool_id: "tool-1".to_string(),
                    name: "write".to_string(),
                    args: json!({"path": "notes.md"}),
                    timeout_ms: Some(1000),
                },
            ),
            event(
                7,
                165,
                EventKind::ToolStdout {
                    tool_id: "tool-1".to_string(),
                    chunk: "stdout".to_string(),
                },
            ),
            event(
                8,
                170,
                EventKind::ToolStderr {
                    tool_id: "tool-1".to_string(),
                    chunk: "stderr".to_string(),
                },
            ),
            event(
                9,
                180,
                EventKind::ToolEnded {
                    tool_id: "tool-1".to_string(),
                    exit_code: 0,
                    duration_ms: 20,
                    artifacts: Some(json!({"artifact_id": a1, "nested": [a2, "ignore"]})),
                },
            ),
            event(
                10,
                185,
                EventKind::ToolStarted {
                    tool_id: "tool-2".to_string(),
                    name: "shell".to_string(),
                    args: json!({"cmd": "sleep 1"}),
                    timeout_ms: None,
                },
            ),
            event(
                11,
                190,
                EventKind::ToolFailed {
                    tool_id: "tool-2".to_string(),
                    error: "boom".to_string(),
                },
            ),
            event(
                12,
                200,
                EventKind::ToolTaskSpawned {
                    task_id: "task-1".to_string(),
                    tool_name: "shell".to_string(),
                    args: json!({"cmd": "pwd"}),
                    cwd: Some("/tmp".to_string()),
                    title: Some("pwd".to_string()),
                    execution_mode: ToolTaskExecutionMode::Pty,
                    origin_session_id: Some("s1".to_string()),
                    artifacts: Some(json!({"artifact": a3})),
                },
            ),
            event(
                13,
                205,
                EventKind::ToolTaskOutputDelta {
                    task_id: "task-1".to_string(),
                    stream: ToolTaskStream::Stdout,
                    chunk: "line one".to_string(),
                    artifacts: Some(json!([a4])),
                },
            ),
            event(
                14,
                206,
                EventKind::ToolTaskOutputDelta {
                    task_id: "task-1".to_string(),
                    stream: ToolTaskStream::Stderr,
                    chunk: "warn".to_string(),
                    artifacts: None,
                },
            ),
            event(
                15,
                207,
                EventKind::ToolTaskOutputDelta {
                    task_id: "task-1".to_string(),
                    stream: ToolTaskStream::Pty,
                    chunk: "pty".to_string(),
                    artifacts: None,
                },
            ),
            event(
                16,
                210,
                EventKind::ToolTaskStatus {
                    task_id: "task-1".to_string(),
                    status: ToolTaskStatus::Running,
                    exit_code: None,
                    started_at_ms: Some(205),
                    ended_at_ms: None,
                    artifacts: None,
                    error: None,
                },
            ),
            event(
                17,
                220,
                EventKind::ToolTaskStatus {
                    task_id: "task-2".to_string(),
                    status: ToolTaskStatus::Failed,
                    exit_code: Some(9),
                    started_at_ms: Some(219),
                    ended_at_ms: Some(220),
                    artifacts: Some(json!({"artifact": a5})),
                    error: Some("failed".to_string()),
                },
            ),
            event(
                18,
                230,
                EventKind::ContinuityJobSpawned {
                    job_id: "job-1".to_string(),
                    job_kind: "compaction".to_string(),
                    details: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            ),
            event(
                19,
                240,
                EventKind::ContinuityJobEnded {
                    job_id: "job-2".to_string(),
                    job_kind: "audit".to_string(),
                    status: "completed".to_string(),
                    result: None,
                    error: Some("none".to_string()),
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            ),
            event(
                20,
                250,
                EventKind::ContinuityContextSelectionDecided {
                    run_session_id: "run-1".to_string(),
                    message_id: "m1".to_string(),
                    compiler_id: "rip.context_compiler.v1".to_string(),
                    compiler_strategy: "recent_messages_v1".to_string(),
                    limits: json!({"recent_messages_v1_limit": 8}),
                    compaction_checkpoint: None,
                    compaction_checkpoints: Vec::new(),
                    resets: Vec::new(),
                    reason: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            ),
            event(
                21,
                255,
                EventKind::ContinuityContextCompiled {
                    run_session_id: "run-1".to_string(),
                    bundle_artifact_id: a6.clone(),
                    compiler_id: "rip.context_compiler.v1".to_string(),
                    compiler_strategy: "recent_messages_v1".to_string(),
                    from_seq: 1,
                    from_message_id: Some("m1".to_string()),
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            ),
            event(
                22,
                260,
                EventKind::ContinuityCompactionCheckpointCreated {
                    checkpoint_id: "ckpt-1".to_string(),
                    cut_rule_id: "stride_messages_v1".to_string(),
                    summary_kind: "cumulative_v1".to_string(),
                    summary_artifact_id: a7.clone(),
                    from_seq: 1,
                    from_message_id: Some("m1".to_string()),
                    to_seq: 5,
                    to_message_id: Some("m5".to_string()),
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            ),
            event(
                23,
                265,
                EventKind::OpenResponsesRequest {
                    endpoint: "https://openrouter.ai/api/v1/responses".to_string(),
                    model: Some("gpt-5".to_string()),
                    request_index: 0,
                    kind: "response.create".to_string(),
                    body_artifact_id: artifact('9'),
                    body_bytes: 12,
                    total_bytes: 12,
                    truncated: false,
                },
            ),
        ] {
            state.update(event);
        }

        assert_eq!(state.session_id.as_deref(), Some("s1"));
        assert_eq!(state.ttft_ms(), Some(50));
        assert_eq!(state.e2e_ms(), Some(120));
        assert_eq!(state.openresponses_headers_ms(), Some(10));
        assert_eq!(state.openresponses_first_byte_ms(), Some(20));
        assert_eq!(state.openresponses_first_provider_event_ms(), Some(30));
        assert_eq!(
            state.openresponses_endpoint.as_deref(),
            Some("https://openrouter.ai/api/v1/responses")
        );
        assert_eq!(state.openresponses_model.as_deref(), Some("gpt-5"));
        assert_eq!(state.selected_seq, Some(23));
        assert!(state.has_error());
        assert_eq!(state.last_error_seq, Some(17));
        assert!(state.output_text.contains("You: hello"));
        assert!(state.output_text.contains("world"));

        let tool1 = state.tools.get("tool-1").expect("tool-1");
        assert_eq!(tool1.stdout_preview, "stdout");
        assert_eq!(tool1.stderr_preview, "stderr");
        assert!(matches!(
            tool1.status,
            ToolStatus::Ended {
                exit_code: 0,
                duration_ms: 20
            }
        ));
        assert!(tool1.artifact_ids.contains(&artifact('a')));
        assert!(tool1.artifact_ids.contains(&artifact('b')));

        let tool2 = state.tools.get("tool-2").expect("tool-2");
        assert!(matches!(
            &tool2.status,
            ToolStatus::Failed { error } if error == "boom"
        ));

        let task1 = state.tasks.get("task-1").expect("task-1");
        assert_eq!(task1.tool_name, "shell");
        assert_eq!(task1.stdout_preview, "line one");
        assert_eq!(task1.stderr_preview, "warn");
        assert_eq!(task1.pty_preview, "pty");
        assert_eq!(task1.status, ToolTaskStatus::Running);
        assert!(task1.artifact_ids.contains(&artifact('c')));
        assert!(task1.artifact_ids.contains(&artifact('d')));

        let task2 = state.tasks.get("task-2").expect("task-2");
        assert_eq!(task2.tool_name, "unknown");
        assert_eq!(task2.status, ToolTaskStatus::Failed);
        assert_eq!(task2.exit_code, Some(9));
        assert_eq!(task2.error.as_deref(), Some("failed"));
        assert!(task2.artifact_ids.contains(&artifact('e')));

        assert_eq!(
            state.running_tool_ids().collect::<Vec<_>>(),
            Vec::<&str>::new()
        );
        assert_eq!(state.running_task_ids().collect::<Vec<_>>(), vec!["task-1"]);
        assert_eq!(state.running_job_ids().collect::<Vec<_>>(), vec!["job-1"]);

        assert!(matches!(
            state.jobs.get("job-1").expect("job-1").status,
            JobStatus::Running
        ));
        assert!(matches!(
            &state.jobs.get("job-2").expect("job-2").status,
            JobStatus::Ended { status, error }
                if status == "completed" && error.as_deref() == Some("none")
        ));
        assert!(matches!(
            &state.context,
            Some(ContextSummary {
                run_session_id,
                compiler_strategy,
                status: ContextStatus::Compiled,
                bundle_artifact_id: Some(bundle),
            }) if run_session_id == "run-1"
                && compiler_strategy == "recent_messages_v1"
                && bundle == &a6
        ));

        for artifact_id in [
            artifact('a'),
            artifact('b'),
            artifact('c'),
            artifact('d'),
            artifact('e'),
            a6,
            a7,
            artifact('9'),
        ] {
            assert!(state.artifacts.contains(&artifact_id));
        }
    }

    #[test]
    fn helper_functions_handle_errors_artifacts_and_utf8_boundaries() {
        assert!(is_error_event(&EventKind::ToolFailed {
            tool_id: "tool-1".to_string(),
            error: "boom".to_string(),
        }));
        assert!(is_error_event(&EventKind::CheckpointFailed {
            action: CheckpointAction::Create,
            error: "bad".to_string(),
        }));
        assert!(is_error_event(&EventKind::ToolTaskStatus {
            task_id: "task-1".to_string(),
            status: ToolTaskStatus::Failed,
            exit_code: None,
            started_at_ms: None,
            ended_at_ms: None,
            artifacts: None,
            error: None,
        }));
        assert!(is_error_event(&EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Event,
            event_name: None,
            data: None,
            raw: None,
            errors: vec!["oops".to_string()],
            response_errors: Vec::new(),
        }));
        assert!(!is_error_event(&EventKind::SessionEnded {
            reason: "ok".to_string(),
        }));

        let mut preview = String::new();
        push_preview(&mut preview, "", 6);
        push_preview(&mut preview, "ab😀cd😀ef", 6);
        assert!(preview.is_char_boundary(preview.len()));
        assert!(preview.len() <= 8);

        let ids = extract_artifact_ids(&json!({
            "one": artifact('a'),
            "nested": [artifact('b'), {"deep": artifact('c')}],
            "ignore": "short"
        }));
        assert_eq!(ids.len(), 3);
        assert!(looks_like_artifact_id(&artifact('f')));
        assert!(!looks_like_artifact_id("artifact"));
    }
}
