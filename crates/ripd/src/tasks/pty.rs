use std::io::{Read, Write};
use std::sync::{Arc, Mutex as StdMutex};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use rip_kernel::{EventKind, ToolTaskStatus, ToolTaskStream};
use serde_json::json;

use super::logs::{resolve_path, TaskLogSummary, TaskLogWriter};
use super::{
    fail_task, now_ms, ApiToolTaskStatus, TaskControl, TaskEmitter, TaskHandle, TaskRunContext,
};

pub(super) async fn run_pty_task(handle: &TaskHandle, ctx: TaskRunContext) {
    let TaskRunContext {
        config,
        emitter,
        args,
        artifact_max_bytes,
        max_bytes,
        spawn_time_ms,
        cancel_rx,
    } = ctx;
    let mut cancel_rx = cancel_rx;

    let logs = &handle.logs;
    let pty_log = match logs.pty.as_ref() {
        Some(log) => log,
        None => {
            fail_task(handle, &emitter, "pty log missing".to_string()).await;
            return;
        }
    };

    let mut pty_writer = match TaskLogWriter::new(
        &config,
        &pty_log.artifact_id,
        &pty_log.path,
        artifact_max_bytes,
    )
    .await
    {
        Ok(writer) => writer,
        Err(err) => {
            fail_task(handle, &emitter, err).await;
            return;
        }
    };

    let pty_system = native_pty_system();
    let size = PtySize {
        rows: args.rows.unwrap_or(24),
        cols: args.cols.unwrap_or(80),
        pixel_width: 0,
        pixel_height: 0,
    };
    let pair = match pty_system.openpty(size) {
        Ok(pair) => pair,
        Err(err) => {
            fail_task(handle, &emitter, format!("pty open failed: {err}")).await;
            return;
        }
    };

    let (program, program_args) = super::resolve_shell_program(&args.command);
    let mut cmd = CommandBuilder::new(program);
    cmd.args(program_args);
    if let Some(cwd) = args.cwd.as_deref() {
        match resolve_path(&config.workspace_root, cwd) {
            Ok(path) => cmd.cwd(path),
            Err(err) => {
                fail_task(handle, &emitter, err).await;
                return;
            }
        };
    }
    if let Some(envs) = &args.env {
        for (key, value) in envs {
            cmd.env(key, value);
        }
    }

    let mut child = match pair.slave.spawn_command(cmd) {
        Ok(child) => child,
        Err(err) => {
            fail_task(handle, &emitter, format!("pty spawn failed: {err}")).await;
            return;
        }
    };

    let killer = Arc::new(StdMutex::new(child.clone_killer()));

    let reader = match pair.master.try_clone_reader() {
        Ok(reader) => reader,
        Err(err) => {
            fail_task(handle, &emitter, format!("pty reader failed: {err}")).await;
            return;
        }
    };
    let writer = match pair.master.take_writer() {
        Ok(writer) => Arc::new(StdMutex::new(writer)),
        Err(err) => {
            fail_task(handle, &emitter, format!("pty writer failed: {err}")).await;
            return;
        }
    };

    let (control_tx, mut control_rx) = tokio::sync::mpsc::channel::<TaskControl>(1024);
    {
        let mut guard = handle.control_tx.lock().await;
        *guard = Some(control_tx);
    }

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

    let mut master = pair.master;

    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    let output_thread = tokio::task::spawn_blocking(move || {
        let mut reader = reader;
        let mut buf = [0u8; 8192];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            };
            if output_tx.blocking_send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });

    let mut wait_handle = tokio::task::spawn_blocking(move || child.wait());

    let mut cancel_reason: Option<String> = None;
    let task_id = handle.task_id.clone();
    let mut exit_status: Option<
        Result<Result<portable_pty::ExitStatus, std::io::Error>, tokio::task::JoinError>,
    > = None;
    let mut output_closed = false;

    while !(exit_status.is_some() && output_closed) {
        tokio::select! {
            status = &mut wait_handle, if exit_status.is_none() => {
                exit_status = Some(status);
            }
            _ = cancel_rx.changed(), if cancel_reason.is_none() => {
                cancel_reason = cancel_rx.borrow().clone();
                if let Some(reason) = cancel_reason.as_deref() {
                    emitter
                        .emit(EventKind::ToolTaskCancelRequested { task_id: task_id.clone(), reason: reason.to_string() })
                        .await;
                }
                let killer = killer.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    let mut guard = killer.lock().expect("killer lock");
                    let _ = guard.kill();
                }).await;
            }
            maybe_control = control_rx.recv() => {
                let Some(message) = maybe_control else {
                    continue;
                };
                drain_output(&task_id, &emitter, &mut pty_writer, max_bytes, &mut output_rx).await;
                handle_control(&task_id, &emitter, &writer, &killer, &mut master, message).await;
            }
            maybe_chunk = output_rx.recv(), if !output_closed => {
                match maybe_chunk {
                    Some(chunk) => emit_output(&task_id, &emitter, &mut pty_writer, max_bytes, &chunk).await,
                    None => output_closed = true,
                }
            }
        }
    }

    let output_join = output_thread.await;

    {
        let mut guard = handle.control_tx.lock().await;
        guard.take();
    }

    let ended_at_ms = now_ms();
    let (status, exit_code, error) = match exit_status {
        Some(exit_status) => match exit_status {
            Ok(status) => match status {
                Ok(status) => {
                    let code = status.exit_code() as i32;
                    if cancel_reason.is_some() {
                        (ToolTaskStatus::Cancelled, Some(code), None)
                    } else {
                        (ToolTaskStatus::Exited, Some(code), None)
                    }
                }
                Err(err) => (
                    ToolTaskStatus::Failed,
                    None,
                    Some(format!("wait failed: {err}")),
                ),
            },
            Err(err) => (
                ToolTaskStatus::Failed,
                None,
                Some(format!("wait join failed: {err}")),
            ),
        },
        None => (
            ToolTaskStatus::Failed,
            None,
            Some("wait result missing".to_string()),
        ),
    };

    let pty_summary = match output_join {
        Ok(_) => pty_writer.finish(),
        Err(_) => TaskLogSummary::failed(
            pty_log.artifact_id.clone(),
            pty_log.path.clone(),
            "pty output join failed".to_string(),
        ),
    };

    let artifacts = Some(json!({
        "logs": {
            "pty": pty_summary.as_json(),
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
}

async fn drain_output(
    task_id: &str,
    emitter: &TaskEmitter,
    writer: &mut TaskLogWriter,
    max_bytes: usize,
    output_rx: &mut tokio::sync::mpsc::Receiver<Vec<u8>>,
) {
    loop {
        match output_rx.try_recv() {
            Ok(chunk) => emit_output(task_id, emitter, writer, max_bytes, &chunk).await,
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
        }
    }
}

async fn emit_output(
    task_id: &str,
    emitter: &TaskEmitter,
    writer: &mut TaskLogWriter,
    max_bytes: usize,
    chunk: &[u8],
) {
    let artifacts = match writer.append(chunk).await {
        Ok(value) => Some(json!({ "log": value })),
        Err(_) => None,
    };
    let (preview, _truncated, _used) =
        super::logs::truncate_utf8(chunk, max_bytes.min(super::OUTPUT_EVENT_MAX_BYTES));
    if preview.is_empty() {
        return;
    }
    emitter
        .emit(EventKind::ToolTaskOutputDelta {
            task_id: task_id.to_string(),
            stream: ToolTaskStream::Pty,
            chunk: preview,
            artifacts,
        })
        .await;
}

async fn handle_control(
    task_id: &str,
    emitter: &TaskEmitter,
    stdin: &Arc<StdMutex<Box<dyn Write + Send>>>,
    killer: &Arc<StdMutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>>,
    master: &mut Box<dyn portable_pty::MasterPty + Send>,
    message: TaskControl,
) {
    match message {
        TaskControl::WriteStdin { chunk_b64, bytes } => {
            let stdin = stdin.clone();
            let write_result = tokio::task::spawn_blocking(move || {
                let mut guard = stdin.lock().expect("stdin writer lock");
                guard.write_all(&bytes).and_then(|_| guard.flush())
            })
            .await;
            let wrote = matches!(write_result, Ok(Ok(())));
            if wrote {
                emitter
                    .emit(EventKind::ToolTaskStdinWritten {
                        task_id: task_id.to_string(),
                        chunk_b64,
                    })
                    .await;
            }
        }
        TaskControl::Resize { rows, cols } => {
            let resized = master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .is_ok();
            if resized {
                emitter
                    .emit(EventKind::ToolTaskResized {
                        task_id: task_id.to_string(),
                        rows,
                        cols,
                    })
                    .await;
            }
        }
        TaskControl::Signal { signal } => {
            let action = normalize_signal(&signal);
            let Some(action) = action else {
                return;
            };

            let applied = match action {
                SignalAction::CtrlC => {
                    let stdin = stdin.clone();
                    let write_result = tokio::task::spawn_blocking(move || {
                        let mut guard = stdin.lock().expect("stdin writer lock");
                        guard.write_all(&[0x03]).and_then(|_| guard.flush())
                    })
                    .await;
                    matches!(write_result, Ok(Ok(())))
                }
                SignalAction::CtrlBackslash => {
                    let stdin = stdin.clone();
                    let write_result = tokio::task::spawn_blocking(move || {
                        let mut guard = stdin.lock().expect("stdin writer lock");
                        guard.write_all(&[0x1c]).and_then(|_| guard.flush())
                    })
                    .await;
                    matches!(write_result, Ok(Ok(())))
                }
                SignalAction::Kill => {
                    let killer = killer.clone();
                    let killed = tokio::task::spawn_blocking(move || {
                        let mut guard = killer.lock().expect("killer lock");
                        guard.kill().is_ok()
                    })
                    .await;
                    matches!(killed, Ok(true))
                }
            };

            if applied {
                emitter
                    .emit(EventKind::ToolTaskSignalled {
                        task_id: task_id.to_string(),
                        signal,
                    })
                    .await;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SignalAction {
    CtrlC,
    CtrlBackslash,
    Kill,
}

fn normalize_signal(signal: &str) -> Option<SignalAction> {
    let raw = signal.trim();
    if raw.is_empty() {
        return None;
    }
    let upper = raw.to_ascii_uppercase();
    let normalized = upper.strip_prefix("SIG").unwrap_or(upper.as_str());
    match normalized {
        "INT" => Some(SignalAction::CtrlC),
        "QUIT" => Some(SignalAction::CtrlBackslash),
        "TERM" | "HUP" | "KILL" => Some(SignalAction::Kill),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rip_log::EventLog;
    use serde_json::json;
    use tempfile::tempdir;

    use super::super::logs::TaskLog;
    use super::super::{
        ApiToolTaskExecutionMode, ApiToolTaskStatus, ShellArgs, TaskEmitter, TaskEngine,
        TaskEngineConfig, TaskLogs, TaskRunContext, TaskSpawnPayload,
    };
    use super::{normalize_signal, run_pty_task, SignalAction};

    #[test]
    fn normalize_signal_accepts_known_values() {
        assert_eq!(normalize_signal(""), None);
        assert_eq!(normalize_signal("   "), None);
        assert_eq!(normalize_signal("SIGINT"), Some(SignalAction::CtrlC));
        assert_eq!(normalize_signal("int"), Some(SignalAction::CtrlC));
        assert_eq!(normalize_signal("QUIT"), Some(SignalAction::CtrlBackslash));
        assert_eq!(normalize_signal("SIGTERM"), Some(SignalAction::Kill));
        assert_eq!(normalize_signal("SIGUSR1"), None);
    }

    #[tokio::test]
    async fn run_pty_task_fails_when_pty_log_missing() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_root).expect("workspace");

        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let snapshot_dir = Arc::new(data_dir.join("task_snapshots"));
        let workspace_lock = Arc::new(crate::workspace_lock::WorkspaceLock::new());
        let config = TaskEngineConfig {
            workspace_root,
            artifact_max_bytes: 128,
            max_bytes: 64,
        };
        let engine = TaskEngine::new(
            config.clone(),
            workspace_lock,
            event_log.clone(),
            snapshot_dir,
        );

        let payload = TaskSpawnPayload {
            tool: "bash".to_string(),
            args: json!({"command": ""}),
            title: None,
            execution_mode: Some(ApiToolTaskExecutionMode::Pty),
            origin_session_id: None,
        };
        let mut handle = engine.create_task(&payload);
        handle.logs = Arc::new(TaskLogs {
            stdout: None,
            stderr: None,
            pty: None,
        });

        let emitter = TaskEmitter::new(&handle, event_log);
        let args = ShellArgs {
            command: "".to_string(),
            cwd: None,
            env: None,
            artifact_max_bytes: None,
            max_bytes: None,
            rows: None,
            cols: None,
        };
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(None);

        run_pty_task(
            &handle,
            TaskRunContext {
                config,
                emitter,
                args,
                artifact_max_bytes: 128,
                max_bytes: 64,
                spawn_time_ms: 0,
                cancel_rx,
            },
        )
        .await;

        let status = handle.status().await;
        assert_eq!(status.status, ApiToolTaskStatus::Failed);
        assert_eq!(status.error.as_deref(), Some("pty log missing"));
    }

    #[tokio::test]
    async fn run_pty_task_fails_when_log_writer_cannot_open() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_root).expect("workspace");

        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let snapshot_dir = Arc::new(data_dir.join("task_snapshots"));
        let workspace_lock = Arc::new(crate::workspace_lock::WorkspaceLock::new());
        let config = TaskEngineConfig {
            workspace_root,
            artifact_max_bytes: 128,
            max_bytes: 64,
        };
        let engine = TaskEngine::new(
            config.clone(),
            workspace_lock,
            event_log.clone(),
            snapshot_dir,
        );

        // Ensure the base artifacts dir exists so failure comes from the nested path (`bad/id`).
        std::fs::create_dir_all(config.artifacts_blobs_dir()).expect("artifacts dir");

        let payload = TaskSpawnPayload {
            tool: "bash".to_string(),
            args: json!({"command": ""}),
            title: None,
            execution_mode: Some(ApiToolTaskExecutionMode::Pty),
            origin_session_id: None,
        };
        let mut handle = engine.create_task(&payload);
        handle.logs = Arc::new(TaskLogs {
            stdout: None,
            stderr: None,
            pty: Some(TaskLog {
                artifact_id: "bad/id".to_string(),
                path: "bad/id".to_string(),
            }),
        });

        let emitter = TaskEmitter::new(&handle, event_log);
        let args = ShellArgs {
            command: "".to_string(),
            cwd: None,
            env: None,
            artifact_max_bytes: None,
            max_bytes: None,
            rows: None,
            cols: None,
        };
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(None);

        run_pty_task(
            &handle,
            TaskRunContext {
                config,
                emitter,
                args,
                artifact_max_bytes: 128,
                max_bytes: 64,
                spawn_time_ms: 0,
                cancel_rx,
            },
        )
        .await;

        let status = handle.status().await;
        assert_eq!(status.status, ApiToolTaskStatus::Failed);
        assert!(status
            .error
            .as_deref()
            .unwrap_or("")
            .starts_with("artifact create failed:"));
    }
}
