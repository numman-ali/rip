use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rip_kernel::{Event, EventKind, ToolTaskExecutionMode, ToolTaskStatus, ToolTaskStream};
use rip_log::{write_snapshot, EventLog};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::{broadcast, watch, Mutex, RwLock};
use utoipa::ToSchema;
use uuid::Uuid;

const EVENT_CHANNEL_CAPACITY: usize = 16_384;
const OUTPUT_EVENT_MAX_BYTES: usize = 8 * 1024;

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
        let log = match stream {
            TaskOutputStream::Stdout => &self.logs.stdout,
            TaskOutputStream::Stderr => &self.logs.stderr,
        };
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
}

#[derive(Clone)]
pub(crate) struct TaskEngine {
    config: TaskEngineConfig,
    event_log: Arc<EventLog>,
    snapshot_dir: Arc<PathBuf>,
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
        event_log: Arc<EventLog>,
        snapshot_dir: Arc<PathBuf>,
    ) -> Self {
        Self {
            config,
            event_log,
            snapshot_dir,
        }
    }

    pub(crate) fn config(&self) -> &TaskEngineConfig {
        &self.config
    }

    pub(crate) fn create_task(&self, payload: &TaskSpawnPayload) -> TaskHandle {
        let task_id = Uuid::new_v4().to_string();
        let (sender, _receiver) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

        let stdout = TaskLog::new(&self.config.workspace_root);
        let stderr = TaskLog::new(&self.config.workspace_root);
        let logs = Arc::new(TaskLogs { stdout, stderr });
        let execution_mode: ToolTaskExecutionMode = payload
            .execution_mode
            .unwrap_or(ApiToolTaskExecutionMode::Pipes)
            .into();

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
                artifacts: Some(json!({
                    "stdout": logs.stdout.as_ref_json(),
                    "stderr": logs.stderr.as_ref_json(),
                })),
                error: None,
            })),
            cancel_tx: watch::channel(None).0,
            logs,
        }
    }

    pub(crate) fn spawn_task(&self, handle: TaskHandle, payload: TaskSpawnPayload) {
        let event_log = self.event_log.clone();
        let snapshot_dir = self.snapshot_dir.clone();
        let config = self.config.clone();
        tokio::spawn(async move {
            run_task(handle, payload, config, event_log, snapshot_dir).await;
        });
    }
}

#[derive(Debug, Clone)]
struct TaskLog {
    artifact_id: String,
    path: String,
}

impl TaskLog {
    fn new(workspace_root: &Path) -> Self {
        let artifact_id = new_artifact_id();
        let path = workspace_root
            .join(".rip")
            .join("artifacts")
            .join("blobs")
            .join(&artifact_id);
        Self {
            artifact_id,
            path: normalize_rel_path(workspace_root, &path),
        }
    }

    fn as_ref_json(&self) -> Value {
        json!({
            "id": self.artifact_id,
            "path": self.path,
        })
    }
}

#[derive(Debug)]
struct TaskLogs {
    stdout: TaskLog,
    stderr: TaskLog,
}

#[derive(Debug, Deserialize)]
struct ShellArgs {
    command: String,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    artifact_max_bytes: Option<usize>,
    max_bytes: Option<usize>,
}

async fn run_task(
    handle: TaskHandle,
    payload: TaskSpawnPayload,
    config: TaskEngineConfig,
    event_log: Arc<EventLog>,
    snapshot_dir: Arc<PathBuf>,
) {
    let emitter = TaskEmitter::new(&handle, event_log.clone());
    let mut cancel_rx = handle.cancel_tx.subscribe();

    let execution_mode: ToolTaskExecutionMode = payload
        .execution_mode
        .unwrap_or(ApiToolTaskExecutionMode::Pipes)
        .into();
    if execution_mode != ToolTaskExecutionMode::Pipes {
        fail_task(
            &handle,
            &emitter,
            format!("execution_mode {execution_mode:?} not supported yet"),
        )
        .await;
        finalize_snapshot(&handle, &snapshot_dir).await;
        return;
    }

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

    let mut stdout_writer = match TaskLogWriter::new(
        &config,
        &handle.logs.stdout.artifact_id,
        &handle.logs.stdout.path,
        artifact_max_bytes,
    )
    .await
    {
        Ok(writer) => writer,
        Err(err) => {
            fail_task(&handle, &emitter, err).await;
            finalize_snapshot(&handle, &snapshot_dir).await;
            return;
        }
    };

    let mut stderr_writer = match TaskLogWriter::new(
        &config,
        &handle.logs.stderr.artifact_id,
        &handle.logs.stderr.path,
        artifact_max_bytes,
    )
    .await
    {
        Ok(writer) => writer,
        Err(err) => {
            fail_task(&handle, &emitter, err).await;
            finalize_snapshot(&handle, &snapshot_dir).await;
            return;
        }
    };

    let (program, program_args) = resolve_shell_program(&args.command);
    let mut cmd = Command::new(program);
    cmd.args(program_args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    if let Some(cwd) = args.cwd.as_deref() {
        match resolve_path(&config.workspace_root, cwd) {
            Ok(path) => cmd.current_dir(path),
            Err(err) => {
                fail_task(&handle, &emitter, err).await;
                finalize_snapshot(&handle, &snapshot_dir).await;
                return;
            }
        };
    }
    if let Some(envs) = &args.env {
        cmd.envs(envs);
    }

    let spawn_time_ms = now_ms();
    emitter
        .emit(EventKind::ToolTaskSpawned {
            task_id: handle.task_id.clone(),
            tool_name: payload.tool.clone(),
            args: payload.args.clone(),
            cwd: args.cwd.clone(),
            title: payload.title.clone(),
            execution_mode: ToolTaskExecutionMode::Pipes,
            origin_session_id: payload.origin_session_id.clone(),
            artifacts: Some(json!({
                "logs": {
                    "stdout": handle.logs.stdout.as_ref_json(),
                    "stderr": handle.logs.stderr.as_ref_json(),
                    "artifact_max_bytes": artifact_max_bytes,
                }
            })),
        })
        .await;

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            fail_task(&handle, &emitter, format!("spawn failed: {err}")).await;
            finalize_snapshot(&handle, &snapshot_dir).await;
            return;
        }
    };

    {
        let mut status = handle.status.write().await;
        status.status = ApiToolTaskStatus::Running;
        status.started_at_ms = Some(spawn_time_ms);
    }
    emitter
        .emit(EventKind::ToolTaskStatus {
            task_id: handle.task_id.clone(),
            status: ToolTaskStatus::Running,
            exit_code: None,
            started_at_ms: Some(spawn_time_ms),
            ended_at_ms: None,
            artifacts: None,
            error: None,
        })
        .await;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdout_emitter = emitter.clone();
    let stdout_task_id = handle.task_id.clone();
    let stdout_handle = tokio::spawn(async move {
        pump_output_stream(
            stdout,
            ToolTaskStream::Stdout,
            &stdout_task_id,
            &stdout_emitter,
            &mut stdout_writer,
            max_bytes,
        )
        .await;
        stdout_writer.finish()
    });

    let stderr_emitter = emitter.clone();
    let stderr_task_id = handle.task_id.clone();
    let stderr_handle = tokio::spawn(async move {
        pump_output_stream(
            stderr,
            ToolTaskStream::Stderr,
            &stderr_task_id,
            &stderr_emitter,
            &mut stderr_writer,
            max_bytes,
        )
        .await;
        stderr_writer.finish()
    });

    let mut cancel_reason: Option<String> = None;
    let exit_status = tokio::select! {
        status = child.wait() => status,
        _ = cancel_rx.changed() => {
            cancel_reason = cancel_rx.borrow().clone();
            if let Some(reason) = cancel_reason.as_deref() {
                emitter.emit(EventKind::ToolTaskCancelRequested { task_id: handle.task_id.clone(), reason: reason.to_string() }).await;
            }
            let _ = child.start_kill();
            child.wait().await
        }
    };

    let stdout_summary = stdout_handle.await.unwrap_or_else(|_| {
        TaskLogSummary::failed(
            handle.logs.stdout.artifact_id.clone(),
            handle.logs.stdout.path.clone(),
            "stdout join failed".to_string(),
        )
    });
    let stderr_summary = stderr_handle.await.unwrap_or_else(|_| {
        TaskLogSummary::failed(
            handle.logs.stderr.artifact_id.clone(),
            handle.logs.stderr.path.clone(),
            "stderr join failed".to_string(),
        )
    });

    let ended_at_ms = now_ms();
    let (status, exit_code, error) = match exit_status {
        Ok(status) => {
            if cancel_reason.is_some() {
                (ToolTaskStatus::Cancelled, status.code().or(Some(1)), None)
            } else {
                (ToolTaskStatus::Exited, status.code(), None)
            }
        }
        Err(err) => (
            ToolTaskStatus::Failed,
            None,
            Some(format!("wait failed: {err}")),
        ),
    };

    let artifacts = Some(json!({
        "logs": {
            "stdout": stdout_summary.as_json(),
            "stderr": stderr_summary.as_json(),
        }
    }));

    if let Some(reason) = cancel_reason.as_deref() {
        emitter
            .emit(EventKind::ToolTaskCancelled {
                task_id: handle.task_id.clone(),
                reason: reason.to_string(),
                wall_time_ms: Some(ended_at_ms.saturating_sub(spawn_time_ms)),
            })
            .await;
    }

    {
        let mut current = handle.status.write().await;
        current.status = ApiToolTaskStatus::from(status);
        current.exit_code = exit_code;
        current.ended_at_ms = Some(ended_at_ms);
        current.artifacts = artifacts.clone();
        current.error = error.clone();
    }

    emitter
        .emit(EventKind::ToolTaskStatus {
            task_id: handle.task_id.clone(),
            status,
            exit_code,
            started_at_ms: Some(spawn_time_ms),
            ended_at_ms: Some(ended_at_ms),
            artifacts,
            error,
        })
        .await;

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

#[derive(Debug, Clone)]
struct TaskLogSummary {
    artifact_id: String,
    path: String,
    bytes_total: u64,
    bytes_stored: u64,
    truncated: bool,
    error: Option<String>,
}

impl TaskLogSummary {
    fn failed(artifact_id: String, path: String, error: String) -> Self {
        Self {
            artifact_id,
            path,
            bytes_total: 0,
            bytes_stored: 0,
            truncated: false,
            error: Some(error),
        }
    }

    fn as_json(&self) -> Value {
        json!({
            "id": self.artifact_id,
            "path": self.path,
            "bytes_total": self.bytes_total,
            "bytes_stored": self.bytes_stored,
            "truncated": self.truncated,
            "error": self.error,
        })
    }
}

struct TaskLogWriter {
    artifact_id: String,
    rel_path: String,
    file: tokio::fs::File,
    max_bytes: u64,
    bytes_total: u64,
    bytes_stored: u64,
    truncated: bool,
}

impl TaskLogWriter {
    async fn new(
        config: &TaskEngineConfig,
        artifact_id: &str,
        rel_path: &str,
        max_bytes: usize,
    ) -> Result<Self, String> {
        let path = config.artifacts_blobs_dir().join(artifact_id);
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .map_err(|err| format!("artifact create failed: {err}"))?;
        Ok(Self {
            artifact_id: artifact_id.to_string(),
            rel_path: rel_path.to_string(),
            file,
            max_bytes: max_bytes as u64,
            bytes_total: 0,
            bytes_stored: 0,
            truncated: false,
        })
    }

    async fn append(&mut self, chunk: &[u8]) -> Result<Value, ()> {
        let offset = self.bytes_stored;
        self.bytes_total = self.bytes_total.saturating_add(chunk.len() as u64);

        let remaining = self.max_bytes.saturating_sub(self.bytes_stored) as usize;
        let take = remaining.min(chunk.len());
        if take > 0 {
            if self.file.write_all(&chunk[..take]).await.is_err() {
                return Err(());
            }
            self.bytes_stored = self.bytes_stored.saturating_add(take as u64);
        }
        if (take as u64) < chunk.len() as u64 {
            self.truncated = true;
        }

        Ok(json!({
            "id": self.artifact_id,
            "path": self.rel_path,
            "offset_bytes": offset,
            "bytes": take,
            "bytes_total": self.bytes_total,
            "bytes_stored": self.bytes_stored,
            "truncated": self.truncated,
        }))
    }

    fn finish(self) -> TaskLogSummary {
        TaskLogSummary {
            artifact_id: self.artifact_id,
            path: self.rel_path,
            bytes_total: self.bytes_total,
            bytes_stored: self.bytes_stored,
            truncated: self.truncated,
            error: None,
        }
    }
}

async fn pump_output_stream(
    mut stream: Option<impl tokio::io::AsyncRead + Unpin>,
    stream_kind: ToolTaskStream,
    task_id: &str,
    emitter: &TaskEmitter,
    writer: &mut TaskLogWriter,
    max_preview_bytes: usize,
) {
    let mut buf = vec![0u8; 8192];
    while let Some(reader) = stream.as_mut() {
        let n = match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        let chunk = &buf[..n];

        let artifacts = match writer.append(chunk).await {
            Ok(value) => Some(json!({ "log": value })),
            Err(_) => None,
        };

        let (preview, _truncated, _used) =
            truncate_utf8(chunk, max_preview_bytes.min(OUTPUT_EVENT_MAX_BYTES));
        if preview.is_empty() {
            continue;
        }

        emitter
            .emit(EventKind::ToolTaskOutputDelta {
                task_id: task_id.to_string(),
                stream: stream_kind,
                chunk: preview,
                artifacts,
            })
            .await;
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

fn truncate_utf8(bytes: &[u8], max_bytes: usize) -> (String, bool, usize) {
    if bytes.len() <= max_bytes {
        return (
            String::from_utf8_lossy(bytes).into_owned(),
            false,
            bytes.len(),
        );
    }

    let mut end = max_bytes;
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }
    (
        String::from_utf8_lossy(&bytes[..end]).into_owned(),
        true,
        end,
    )
}

fn resolve_path(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Err("absolute paths are not allowed".to_string());
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("path escapes workspace root".to_string());
    }
    Ok(root.join(path))
}

fn normalize_rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn new_artifact_id() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn read_artifact_range(
    config: &TaskEngineConfig,
    id: &str,
    offset_bytes: u64,
    max_bytes: usize,
) -> Result<(String, usize, u64, bool), String> {
    if !is_lower_hex_64(id) {
        return Err("invalid artifact id".to_string());
    }

    let path = config.artifacts_blobs_dir().join(id);
    let meta =
        std::fs::metadata(&path).map_err(|err| format!("read artifact meta failed: {err}"))?;
    let total_bytes = meta.len();

    let mut file =
        std::fs::File::open(&path).map_err(|err| format!("read artifact failed: {err}"))?;
    if offset_bytes > 0 {
        use std::io::Seek;
        file.seek(std::io::SeekFrom::Start(offset_bytes))
            .map_err(|err| format!("read artifact failed: {err}"))?;
    }

    let mut buf = vec![0u8; max_bytes];
    use std::io::Read;
    let read_bytes = file
        .read(&mut buf)
        .map_err(|err| format!("read artifact failed: {err}"))?;
    buf.truncate(read_bytes);

    let (content, utf8_truncated, used_bytes) = truncate_utf8(&buf, max_bytes);
    let truncated = utf8_truncated || (offset_bytes + read_bytes as u64) < total_bytes;
    Ok((content, used_bytes, total_bytes, truncated))
}

fn is_lower_hex_64(value: &str) -> bool {
    if value.len() != 64 {
        return false;
    }
    value
        .as_bytes()
        .iter()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn build_engine(
        dir: &tempfile::TempDir,
    ) -> (TaskEngine, TaskEngineConfig, Arc<EventLog>, Arc<PathBuf>) {
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_root).expect("workspace");
        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let snapshot_dir = Arc::new(data_dir.join("task_snapshots"));
        let config = TaskEngineConfig {
            workspace_root,
            artifact_max_bytes: 1024,
            max_bytes: 128,
        };
        let engine = TaskEngine::new(config.clone(), event_log.clone(), snapshot_dir.clone());
        (engine, config, event_log, snapshot_dir)
    }

    #[test]
    fn new_artifact_id_is_lower_hex_64() {
        let id = new_artifact_id();
        assert!(is_lower_hex_64(&id));
    }

    #[test]
    fn truncate_utf8_truncates_safely() {
        let bytes = "hello🙂world".as_bytes();
        let (truncated, did_truncate, used) = truncate_utf8(bytes, 8);
        assert!(did_truncate);
        assert_eq!(used, truncated.len());
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }

    #[test]
    fn resolve_path_rejects_absolute_and_parent() {
        let dir = tempdir().expect("tmp");
        assert!(resolve_path(dir.path(), "/tmp").is_err());
        assert!(resolve_path(dir.path(), "../escape").is_err());
    }

    #[test]
    fn is_lower_hex_64_validates() {
        assert!(is_lower_hex_64(&"a".repeat(64)));
        assert!(!is_lower_hex_64(&"A".repeat(64)));
        assert!(!is_lower_hex_64("short"));
    }

    #[test]
    fn read_artifact_range_rejects_invalid_id() {
        let dir = tempdir().expect("tmp");
        let config = TaskEngineConfig {
            workspace_root: dir.path().to_path_buf(),
            artifact_max_bytes: 128,
            max_bytes: 64,
        };
        let err = read_artifact_range(&config, "not-hex", 0, 10).expect_err("err");
        assert!(err.contains("invalid artifact id"));
    }

    #[tokio::test]
    async fn create_task_initializes_status_and_log_refs() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_root).expect("workspace");
        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let snapshot_dir = Arc::new(data_dir.join("task_snapshots"));
        let engine = TaskEngine::new(
            TaskEngineConfig {
                workspace_root: workspace_root.clone(),
                artifact_max_bytes: 1024,
                max_bytes: 128,
            },
            event_log,
            snapshot_dir,
        );

        let payload = TaskSpawnPayload {
            tool: "bash".to_string(),
            args: json!({"command":"printf 'hi\\n'"}),
            title: Some("title".to_string()),
            execution_mode: None,
            origin_session_id: None,
        };
        let handle = engine.create_task(&payload);
        let status = handle.status().await;

        assert_eq!(status.task_id, handle.task_id);
        assert_eq!(status.status, ApiToolTaskStatus::Queued);
        assert_eq!(status.tool, "bash");
        assert_eq!(status.title.as_deref(), Some("title"));
        assert_eq!(status.execution_mode, ApiToolTaskExecutionMode::Pipes);

        let artifacts = status.artifacts.expect("artifacts");
        let stdout_id = artifacts
            .get("stdout")
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_str())
            .unwrap_or("");
        assert!(is_lower_hex_64(stdout_id));
    }

    #[tokio::test]
    async fn task_log_writer_truncates_when_over_max() {
        let dir = tempdir().expect("tmp");
        let config = TaskEngineConfig {
            workspace_root: dir.path().to_path_buf(),
            artifact_max_bytes: 4,
            max_bytes: 64,
        };
        tokio::fs::create_dir_all(config.artifacts_blobs_dir())
            .await
            .expect("mkdir");
        let id = "a".repeat(64);
        let rel_path = normalize_rel_path(
            &config.workspace_root,
            &config.artifacts_blobs_dir().join(&id),
        );

        let mut writer = TaskLogWriter::new(&config, &id, &rel_path, 4)
            .await
            .expect("writer");
        let delta = writer.append(b"hello").await.expect("delta");
        assert_eq!(delta.get("bytes").and_then(|v| v.as_u64()), Some(4));
        assert_eq!(delta.get("offset_bytes").and_then(|v| v.as_u64()), Some(0));
        assert_eq!(delta.get("bytes_total").and_then(|v| v.as_u64()), Some(5));
        assert_eq!(delta.get("bytes_stored").and_then(|v| v.as_u64()), Some(4));
        assert_eq!(delta.get("truncated").and_then(|v| v.as_bool()), Some(true));
    }

    #[tokio::test]
    async fn run_task_rejects_pty_execution_mode() {
        let dir = tempdir().expect("tmp");
        let (engine, config, event_log, snapshot_dir) = build_engine(&dir);

        let payload = TaskSpawnPayload {
            tool: "bash".to_string(),
            args: json!({"command":"true"}),
            title: None,
            execution_mode: Some(ApiToolTaskExecutionMode::Pty),
            origin_session_id: None,
        };
        let handle = engine.create_task(&payload);
        run_task(
            handle.clone(),
            payload,
            config,
            event_log,
            snapshot_dir.clone(),
        )
        .await;

        let status = handle.status().await;
        assert_eq!(status.status, ApiToolTaskStatus::Failed);
        assert!(status.error.unwrap_or_default().contains("execution_mode"));

        let snapshot_path = snapshot_dir.join(format!("{}.json", handle.task_id));
        assert!(snapshot_path.exists(), "expected task snapshot");
    }

    #[tokio::test]
    async fn run_task_rejects_unsupported_tool() {
        let dir = tempdir().expect("tmp");
        let (engine, config, event_log, snapshot_dir) = build_engine(&dir);

        let payload = TaskSpawnPayload {
            tool: "ls".to_string(),
            args: json!({}),
            title: None,
            execution_mode: Some(ApiToolTaskExecutionMode::Pipes),
            origin_session_id: None,
        };
        let handle = engine.create_task(&payload);
        run_task(handle.clone(), payload, config, event_log, snapshot_dir).await;

        let status = handle.status().await;
        assert_eq!(status.status, ApiToolTaskStatus::Failed);
        assert!(status
            .error
            .unwrap_or_default()
            .contains("unsupported tool"));
    }

    #[tokio::test]
    async fn run_task_rejects_invalid_args() {
        let dir = tempdir().expect("tmp");
        let (engine, config, event_log, snapshot_dir) = build_engine(&dir);

        let payload = TaskSpawnPayload {
            tool: "bash".to_string(),
            args: json!({"command": 123}),
            title: None,
            execution_mode: Some(ApiToolTaskExecutionMode::Pipes),
            origin_session_id: None,
        };
        let handle = engine.create_task(&payload);
        run_task(handle.clone(), payload, config, event_log, snapshot_dir).await;

        let status = handle.status().await;
        assert_eq!(status.status, ApiToolTaskStatus::Failed);
        assert!(status.error.unwrap_or_default().contains("invalid args"));
    }

    #[tokio::test]
    async fn run_task_rejects_cwd_escape() {
        let dir = tempdir().expect("tmp");
        let (engine, config, event_log, snapshot_dir) = build_engine(&dir);

        let payload = TaskSpawnPayload {
            tool: "bash".to_string(),
            args: json!({"command":"true", "cwd":"../escape"}),
            title: None,
            execution_mode: Some(ApiToolTaskExecutionMode::Pipes),
            origin_session_id: None,
        };
        let handle = engine.create_task(&payload);
        run_task(handle.clone(), payload, config, event_log, snapshot_dir).await;

        let status = handle.status().await;
        assert_eq!(status.status, ApiToolTaskStatus::Failed);
        assert!(status.error.unwrap_or_default().contains("workspace root"));
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn run_task_writes_stdout_and_stderr_logs() {
        let dir = tempdir().expect("tmp");
        let (engine, config, event_log, snapshot_dir) = build_engine(&dir);

        let payload = TaskSpawnPayload {
            tool: "bash".to_string(),
            args: json!({"command":"printf 'out\\n'; printf 'err\\n' >&2"}),
            title: None,
            execution_mode: Some(ApiToolTaskExecutionMode::Pipes),
            origin_session_id: None,
        };
        let handle = engine.create_task(&payload);
        run_task(
            handle.clone(),
            payload,
            config.clone(),
            event_log,
            snapshot_dir,
        )
        .await;

        let status = handle.status().await;
        assert_eq!(status.status, ApiToolTaskStatus::Exited);

        let stdout = handle
            .output(&config, TaskOutputStream::Stdout, 0, 64)
            .await
            .expect("stdout");
        assert!(stdout.content.contains("out"));

        let stderr = handle
            .output(&config, TaskOutputStream::Stderr, 0, 64)
            .await
            .expect("stderr");
        assert!(stderr.content.contains("err"));

        let range = handle
            .output(&config, TaskOutputStream::Stdout, 1, 64)
            .await
            .expect("range");
        assert!(range.content.starts_with("ut"));
    }

    #[test]
    fn read_artifact_range_seeks_and_marks_truncated() {
        let dir = tempdir().expect("tmp");
        let config = TaskEngineConfig {
            workspace_root: dir.path().to_path_buf(),
            artifact_max_bytes: 128,
            max_bytes: 64,
        };
        std::fs::create_dir_all(config.artifacts_blobs_dir()).expect("mkdir");

        let id = "a".repeat(64);
        let path = config.artifacts_blobs_dir().join(&id);
        std::fs::write(&path, "abcdef").expect("write");

        let (content, bytes, total, truncated) =
            read_artifact_range(&config, &id, 2, 2).expect("range");
        assert_eq!(content, "cd");
        assert_eq!(bytes, 2);
        assert_eq!(total, 6);
        assert!(truncated);
    }

    #[cfg(not(windows))]
    #[test]
    fn resolve_shell_program_empty_command_defaults() {
        let (program, args) = resolve_shell_program("");
        assert_eq!(program, "bash");
        assert_eq!(args, vec!["-c".to_string(), "".to_string()]);
    }

    #[test]
    fn api_tool_task_execution_mode_conversions_roundtrip() {
        let pipes: ToolTaskExecutionMode = ApiToolTaskExecutionMode::Pipes.into();
        let pty: ToolTaskExecutionMode = ApiToolTaskExecutionMode::Pty.into();
        assert_eq!(pipes, ToolTaskExecutionMode::Pipes);
        assert_eq!(pty, ToolTaskExecutionMode::Pty);

        let pipes_api: ApiToolTaskExecutionMode = ToolTaskExecutionMode::Pipes.into();
        let pty_api: ApiToolTaskExecutionMode = ToolTaskExecutionMode::Pty.into();
        assert_eq!(pipes_api, ApiToolTaskExecutionMode::Pipes);
        assert_eq!(pty_api, ApiToolTaskExecutionMode::Pty);
    }

    #[test]
    fn api_tool_task_status_from_covers_all_variants() {
        assert_eq!(
            ApiToolTaskStatus::from(ToolTaskStatus::Queued),
            ApiToolTaskStatus::Queued
        );
        assert_eq!(
            ApiToolTaskStatus::from(ToolTaskStatus::Running),
            ApiToolTaskStatus::Running
        );
        assert_eq!(
            ApiToolTaskStatus::from(ToolTaskStatus::Exited),
            ApiToolTaskStatus::Exited
        );
        assert_eq!(
            ApiToolTaskStatus::from(ToolTaskStatus::Cancelled),
            ApiToolTaskStatus::Cancelled
        );
        assert_eq!(
            ApiToolTaskStatus::from(ToolTaskStatus::Failed),
            ApiToolTaskStatus::Failed
        );
    }
}
