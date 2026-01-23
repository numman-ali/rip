mod logs;
mod pipes;
mod pty;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rip_kernel::{Event, EventKind, ToolTaskExecutionMode, ToolTaskStatus};
use rip_log::{write_snapshot, EventLog};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, watch, Mutex, RwLock};
use utoipa::ToSchema;
use uuid::Uuid;

use self::logs::{base64_decode, read_artifact_range, TaskLogs};
use crate::workspace_lock::WorkspaceLock;

const EVENT_CHANNEL_CAPACITY: usize = 16_384;
const OUTPUT_EVENT_MAX_BYTES: usize = 8 * 1024;
const STDIN_WRITE_MAX_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub(crate) struct TaskSpawnPayload {
    pub(crate) tool: String,
    #[serde(default)]
    pub(crate) args: Value,
    pub(crate) title: Option<String>,
    pub(crate) execution_mode: Option<ApiToolTaskExecutionMode>,
    pub(crate) origin_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct TaskCreated {
    pub(crate) task_id: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct TaskStatusResponse {
    pub(crate) task_id: String,
    pub(crate) status: ApiToolTaskStatus,
    pub(crate) tool: String,
    pub(crate) title: Option<String>,
    pub(crate) execution_mode: ApiToolTaskExecutionMode,
    pub(crate) exit_code: Option<i32>,
    pub(crate) started_at_ms: Option<u64>,
    pub(crate) ended_at_ms: Option<u64>,
    pub(crate) artifacts: Option<Value>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub(crate) struct TaskCancelPayload {
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub(crate) struct TaskWriteStdinPayload {
    pub(crate) chunk_b64: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub(crate) struct TaskResizePayload {
    pub(crate) rows: u16,
    pub(crate) cols: u16,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub(crate) struct TaskSignalPayload {
    pub(crate) signal: String,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub(crate) struct TaskOutputQuery {
    pub(crate) stream: TaskOutputStream,
    pub(crate) offset_bytes: Option<u64>,
    pub(crate) max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct TaskOutputResponse {
    pub(crate) task_id: String,
    pub(crate) stream: TaskOutputStream,
    pub(crate) content: String,
    pub(crate) offset_bytes: u64,
    pub(crate) bytes: usize,
    pub(crate) total_bytes: u64,
    pub(crate) truncated: bool,
    pub(crate) artifact_id: String,
    pub(crate) path: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TaskOutputStream {
    Stdout,
    Stderr,
    Pty,
}

#[derive(Debug, Clone)]
pub(crate) struct TaskHandle {
    pub(crate) task_id: String,
    sender: broadcast::Sender<Event>,
    events: Arc<Mutex<Vec<Event>>>,
    seq: Arc<Mutex<u64>>,
    status: Arc<RwLock<TaskStatusResponse>>,
    cancel_tx: watch::Sender<Option<String>>,
    logs: Arc<TaskLogs>,
    control_tx: Arc<Mutex<Option<mpsc::Sender<TaskControl>>>>,
}

#[derive(Debug)]
enum TaskControl {
    WriteStdin { chunk_b64: String, bytes: Vec<u8> },
    Resize { rows: u16, cols: u16 },
    Signal { signal: String },
}

impl TaskHandle {
    pub(crate) fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    pub(crate) async fn events_snapshot(&self) -> Vec<Event> {
        self.events.lock().await.clone()
    }

    pub(crate) fn cancel(&self, reason: String) -> bool {
        self.cancel_tx.send_replace(Some(reason)).is_none()
    }

    pub(crate) async fn status(&self) -> TaskStatusResponse {
        self.status.read().await.clone()
    }

    pub(crate) async fn output(
        &self,
        config: &TaskEngineConfig,
        stream: TaskOutputStream,
        offset_bytes: u64,
        max_bytes: usize,
    ) -> Result<TaskOutputResponse, String> {
        let log = self
            .logs
            .log_for_output(stream)
            .ok_or_else(|| "output stream not available for this task".to_string())?;
        let (content, bytes, total_bytes, truncated) =
            read_artifact_range(config, &log.artifact_id, offset_bytes, max_bytes)?;
        Ok(TaskOutputResponse {
            task_id: self.task_id.clone(),
            stream,
            content,
            offset_bytes,
            bytes,
            total_bytes,
            truncated,
            artifact_id: log.artifact_id.clone(),
            path: log.path.clone(),
        })
    }

    pub(crate) async fn write_stdin(&self, payload: TaskWriteStdinPayload) -> Result<(), String> {
        if self.status.read().await.execution_mode != ApiToolTaskExecutionMode::Pty {
            return Err("write_stdin is only supported for pty tasks".to_string());
        }
        let bytes = base64_decode(&payload.chunk_b64)?;
        if bytes.len() > STDIN_WRITE_MAX_BYTES {
            return Err(format!(
                "stdin chunk too large (max {STDIN_WRITE_MAX_BYTES} bytes)"
            ));
        }

        let sender = self
            .control_tx
            .lock()
            .await
            .clone()
            .ok_or_else(|| "task is not ready for interactive IO".to_string())?;
        sender
            .send(TaskControl::WriteStdin {
                chunk_b64: payload.chunk_b64,
                bytes,
            })
            .await
            .map_err(|_| "task is no longer accepting stdin".to_string())
    }

    pub(crate) async fn resize(&self, payload: TaskResizePayload) -> Result<(), String> {
        if self.status.read().await.execution_mode != ApiToolTaskExecutionMode::Pty {
            return Err("resize is only supported for pty tasks".to_string());
        }
        if payload.rows == 0 || payload.cols == 0 {
            return Err("rows and cols must be > 0".to_string());
        }
        let sender = self
            .control_tx
            .lock()
            .await
            .clone()
            .ok_or_else(|| "task is not ready for interactive IO".to_string())?;
        sender
            .send(TaskControl::Resize {
                rows: payload.rows,
                cols: payload.cols,
            })
            .await
            .map_err(|_| "task is no longer accepting resize".to_string())
    }

    pub(crate) async fn signal(&self, payload: TaskSignalPayload) -> Result<(), String> {
        if self.status.read().await.execution_mode != ApiToolTaskExecutionMode::Pty {
            return Err("signal is only supported for pty tasks".to_string());
        }
        let raw = payload.signal.trim();
        if raw.is_empty() {
            return Err("signal must be non-empty".to_string());
        }
        let upper = raw.to_ascii_uppercase();
        let normalized = upper.strip_prefix("SIG").unwrap_or(upper.as_str());
        if !matches!(normalized, "INT" | "QUIT" | "TERM" | "HUP" | "KILL") {
            return Err("unsupported signal".to_string());
        }
        let sender = self
            .control_tx
            .lock()
            .await
            .clone()
            .ok_or_else(|| "task is not ready for interactive IO".to_string())?;
        sender
            .send(TaskControl::Signal {
                signal: payload.signal,
            })
            .await
            .map_err(|_| "task is no longer accepting signals".to_string())
    }
}

#[derive(Clone)]
pub(crate) struct TaskEngine {
    config: TaskEngineConfig,
    event_log: Arc<EventLog>,
    snapshot_dir: Arc<PathBuf>,
    workspace_lock: Arc<WorkspaceLock>,
}

#[derive(Debug, Clone)]
pub(crate) struct TaskEngineConfig {
    pub(crate) workspace_root: PathBuf,
    pub(crate) artifact_max_bytes: usize,
    pub(crate) max_bytes: usize,
}

impl TaskEngineConfig {
    fn artifacts_blobs_dir(&self) -> PathBuf {
        self.workspace_root
            .join(".rip")
            .join("artifacts")
            .join("blobs")
    }
}

impl TaskEngine {
    pub(crate) fn new(
        config: TaskEngineConfig,
        workspace_lock: Arc<WorkspaceLock>,
        event_log: Arc<EventLog>,
        snapshot_dir: Arc<PathBuf>,
    ) -> Self {
        Self {
            config,
            event_log,
            snapshot_dir,
            workspace_lock,
        }
    }

    pub(crate) fn config(&self) -> &TaskEngineConfig {
        &self.config
    }

    pub(crate) fn create_task(&self, payload: &TaskSpawnPayload) -> TaskHandle {
        let task_id = Uuid::new_v4().to_string();
        let (sender, _receiver) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

        let execution_mode: ToolTaskExecutionMode = payload
            .execution_mode
            .unwrap_or(ApiToolTaskExecutionMode::Pipes)
            .into();
        let logs = Arc::new(TaskLogs::new(&self.config.workspace_root, execution_mode));

        TaskHandle {
            task_id: task_id.clone(),
            sender,
            events: Arc::new(Mutex::new(Vec::new())),
            seq: Arc::new(Mutex::new(0)),
            status: Arc::new(RwLock::new(TaskStatusResponse {
                task_id,
                status: ApiToolTaskStatus::Queued,
                tool: payload.tool.clone(),
                title: payload.title.clone(),
                execution_mode: execution_mode.into(),
                exit_code: None,
                started_at_ms: None,
                ended_at_ms: None,
                artifacts: Some(json!({ "logs": logs.refs_json() })),
                error: None,
            })),
            cancel_tx: watch::channel(None).0,
            logs,
            control_tx: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn spawn_task(&self, handle: TaskHandle, payload: TaskSpawnPayload) {
        let event_log = self.event_log.clone();
        let snapshot_dir = self.snapshot_dir.clone();
        let config = self.config.clone();
        let workspace_lock = self.workspace_lock.clone();
        tokio::spawn(async move {
            run_task(
                handle,
                payload,
                config,
                workspace_lock,
                event_log,
                snapshot_dir,
            )
            .await;
        });
    }
}

#[derive(Debug, Deserialize)]
struct ShellArgs {
    command: String,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    artifact_max_bytes: Option<usize>,
    max_bytes: Option<usize>,
    rows: Option<u16>,
    cols: Option<u16>,
}

pub(super) struct TaskRunContext {
    config: TaskEngineConfig,
    emitter: TaskEmitter,
    args: ShellArgs,
    artifact_max_bytes: usize,
    max_bytes: usize,
    spawn_time_ms: u64,
    cancel_rx: watch::Receiver<Option<String>>,
}

async fn run_task(
    handle: TaskHandle,
    payload: TaskSpawnPayload,
    config: TaskEngineConfig,
    workspace_lock: Arc<WorkspaceLock>,
    event_log: Arc<EventLog>,
    snapshot_dir: Arc<PathBuf>,
) {
    let emitter = TaskEmitter::new(&handle, event_log.clone());
    let execution_mode: ToolTaskExecutionMode = payload
        .execution_mode
        .unwrap_or(ApiToolTaskExecutionMode::Pipes)
        .into();
    let cancel_rx = handle.cancel_tx.subscribe();

    if payload.tool != "bash" && payload.tool != "shell" {
        fail_task(
            &handle,
            &emitter,
            format!("unsupported tool for task_spawn: {}", payload.tool),
        )
        .await;
        finalize_snapshot(&handle, &snapshot_dir).await;
        return;
    }

    let args: ShellArgs = match serde_json::from_value(payload.args.clone()) {
        Ok(args) => args,
        Err(err) => {
            fail_task(&handle, &emitter, format!("invalid args: {err}")).await;
            finalize_snapshot(&handle, &snapshot_dir).await;
            return;
        }
    };

    let artifact_max_bytes = args.artifact_max_bytes.unwrap_or(config.artifact_max_bytes);
    let max_bytes = args.max_bytes.unwrap_or(config.max_bytes);

    if tokio::fs::create_dir_all(config.artifacts_blobs_dir())
        .await
        .is_err()
    {
        fail_task(
            &handle,
            &emitter,
            "failed to create artifacts dir".to_string(),
        )
        .await;
        finalize_snapshot(&handle, &snapshot_dir).await;
        return;
    }

    let spawn_time_ms = now_ms();
    emitter
        .emit(EventKind::ToolTaskSpawned {
            task_id: handle.task_id.clone(),
            tool_name: payload.tool.clone(),
            args: payload.args.clone(),
            cwd: args.cwd.clone(),
            title: payload.title.clone(),
            execution_mode,
            origin_session_id: payload.origin_session_id.clone(),
            artifacts: Some(json!({
                "logs": handle.logs.refs_json(),
                "artifact_max_bytes": artifact_max_bytes,
                "max_bytes": max_bytes,
            })),
        })
        .await;

    let _workspace_guard = workspace_lock.acquire().await;
    match execution_mode {
        ToolTaskExecutionMode::Pipes => {
            pipes::run_pipes_task(
                &handle,
                TaskRunContext {
                    config,
                    emitter,
                    args,
                    artifact_max_bytes,
                    max_bytes,
                    spawn_time_ms,
                    cancel_rx,
                },
            )
            .await
        }
        ToolTaskExecutionMode::Pty => {
            pty::run_pty_task(
                &handle,
                TaskRunContext {
                    config,
                    emitter,
                    args,
                    artifact_max_bytes,
                    max_bytes,
                    spawn_time_ms,
                    cancel_rx,
                },
            )
            .await
        }
    }

    finalize_snapshot(&handle, &snapshot_dir).await;
}

async fn fail_task(handle: &TaskHandle, emitter: &TaskEmitter, error: String) {
    let ended_at_ms = now_ms();
    {
        let mut current = handle.status.write().await;
        current.status = ApiToolTaskStatus::Failed;
        current.ended_at_ms = Some(ended_at_ms);
        current.error = Some(error.clone());
    }
    emitter
        .emit(EventKind::ToolTaskStatus {
            task_id: handle.task_id.clone(),
            status: ToolTaskStatus::Failed,
            exit_code: None,
            started_at_ms: None,
            ended_at_ms: Some(ended_at_ms),
            artifacts: None,
            error: Some(error),
        })
        .await;
}

async fn finalize_snapshot(handle: &TaskHandle, snapshot_dir: &Path) {
    let guard = handle.events.lock().await;
    let _ = write_snapshot(snapshot_dir, &handle.task_id, &guard);
}

#[derive(Clone)]
struct TaskEmitter {
    task_id: String,
    sender: broadcast::Sender<Event>,
    events: Arc<Mutex<Vec<Event>>>,
    seq: Arc<Mutex<u64>>,
    event_log: Arc<EventLog>,
}

impl TaskEmitter {
    fn new(handle: &TaskHandle, event_log: Arc<EventLog>) -> Self {
        Self {
            task_id: handle.task_id.clone(),
            sender: handle.sender.clone(),
            events: handle.events.clone(),
            seq: handle.seq.clone(),
            event_log,
        }
    }

    async fn emit(&self, kind: EventKind) {
        let mut seq = self.seq.lock().await;
        let event = Event {
            id: Uuid::new_v4().to_string(),
            session_id: self.task_id.clone(),
            timestamp_ms: now_ms(),
            seq: *seq,
            kind,
        };
        *seq += 1;

        let _ = self.sender.send(event.clone());
        let mut guard = self.events.lock().await;
        guard.push(event.clone());
        let _ = self.event_log.append(&event);
    }
}

fn resolve_shell_program(command: &str) -> (String, Vec<String>) {
    if command.is_empty() {
        return ("bash".to_string(), vec!["-c".to_string(), "".to_string()]);
    }
    #[cfg(windows)]
    {
        let program = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd".to_string());
        return (program, vec!["/C".to_string(), command.to_string()]);
    }
    #[cfg(not(windows))]
    {
        let program = "bash".to_string();
        let args = vec!["-c".to_string(), command.to_string()];
        (program, args)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApiToolTaskExecutionMode {
    Pipes,
    Pty,
}

impl From<ApiToolTaskExecutionMode> for ToolTaskExecutionMode {
    fn from(value: ApiToolTaskExecutionMode) -> Self {
        match value {
            ApiToolTaskExecutionMode::Pipes => ToolTaskExecutionMode::Pipes,
            ApiToolTaskExecutionMode::Pty => ToolTaskExecutionMode::Pty,
        }
    }
}

impl From<ToolTaskExecutionMode> for ApiToolTaskExecutionMode {
    fn from(value: ToolTaskExecutionMode) -> Self {
        match value {
            ToolTaskExecutionMode::Pipes => ApiToolTaskExecutionMode::Pipes,
            ToolTaskExecutionMode::Pty => ApiToolTaskExecutionMode::Pty,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ApiToolTaskStatus {
    Queued,
    Running,
    Exited,
    Cancelled,
    Failed,
}

impl From<ToolTaskStatus> for ApiToolTaskStatus {
    fn from(value: ToolTaskStatus) -> Self {
        match value {
            ToolTaskStatus::Queued => ApiToolTaskStatus::Queued,
            ToolTaskStatus::Running => ApiToolTaskStatus::Running,
            ToolTaskStatus::Exited => ApiToolTaskStatus::Exited,
            ToolTaskStatus::Cancelled => ApiToolTaskStatus::Cancelled,
            ToolTaskStatus::Failed => ApiToolTaskStatus::Failed,
        }
    }
}
