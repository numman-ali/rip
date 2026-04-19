//! Ambient run-status summaries.
//!
//! `TuiState` tracks currently-running tools, tasks, jobs, and the
//! compiled-context snapshot as append-only maps keyed by the runtime
//! identifier in the source frame. These small record types describe
//! the shape of each map's value; the state module owns both the maps
//! and the ingestion rules that keep them current.

use std::collections::BTreeSet;

use rip_kernel::{ToolTaskExecutionMode, ToolTaskStatus};
use serde_json::Value;

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
