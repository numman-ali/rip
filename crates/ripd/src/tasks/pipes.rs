use rip_kernel::{EventKind, ToolTaskStatus, ToolTaskStream};
use serde_json::json;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::logs::{resolve_path, TaskLogSummary, TaskLogWriter};
use super::{fail_task, now_ms, ApiToolTaskStatus, TaskEmitter, TaskHandle, TaskRunContext};

#[cfg(unix)]
fn kill_process_group(pid: u32) {
    use std::os::raw::c_int;

    extern "C" {
        fn kill(pid: i32, sig: c_int) -> c_int;
    }

    const SIGKILL: c_int = 9;
    unsafe {
        let _ = kill(-(pid as i32), SIGKILL);
    }
}

pub(super) async fn run_pipes_task(handle: &TaskHandle, ctx: TaskRunContext) {
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
    let stdout = match logs.stdout.as_ref() {
        Some(log) => log,
        None => {
            fail_task(handle, &emitter, "stdout log missing".to_string()).await;
            return;
        }
    };
    let stderr = match logs.stderr.as_ref() {
        Some(log) => log,
        None => {
            fail_task(handle, &emitter, "stderr log missing".to_string()).await;
            return;
        }
    };

    let mut stdout_writer = match TaskLogWriter::new(
        &config,
        &stdout.artifact_id,
        &stdout.path,
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

    let mut stderr_writer = match TaskLogWriter::new(
        &config,
        &stderr.artifact_id,
        &stderr.path,
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

    let (program, program_args) = super::resolve_shell_program(&args.command);
    let mut cmd = Command::new(program);
    cmd.args(program_args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    #[cfg(unix)]
    cmd.process_group(0);

    if let Some(cwd) = args.cwd.as_deref() {
        match resolve_path(&config.workspace_root, cwd) {
            Ok(path) => cmd.current_dir(path),
            Err(err) => {
                fail_task(handle, &emitter, err).await;
                return;
            }
        };
    }
    if let Some(envs) = &args.env {
        cmd.envs(envs);
    }

    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(err) => {
            fail_task(handle, &emitter, format!("spawn failed: {err}")).await;
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

    let stdout_stream = child.stdout.take();
    let stderr_stream = child.stderr.take();

    let stdout_emitter = emitter.clone();
    let stdout_task_id = handle.task_id.clone();
    let stdout_handle = tokio::spawn(async move {
        pump_output_stream(
            stdout_stream,
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
            stderr_stream,
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
            #[cfg(unix)]
            if let Some(pid) = child.id() {
                kill_process_group(pid);
            }
            let _ = child.start_kill();
            child.wait().await
        }
    };

    let stdout_summary = stdout_handle.await.unwrap_or_else(|_| {
        TaskLogSummary::failed(
            stdout.artifact_id.clone(),
            stdout.path.clone(),
            "stdout join failed".to_string(),
        )
    });
    let stderr_summary = stderr_handle.await.unwrap_or_else(|_| {
        TaskLogSummary::failed(
            stderr.artifact_id.clone(),
            stderr.path.clone(),
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
            super::logs::truncate_utf8(chunk, max_preview_bytes.min(super::OUTPUT_EVENT_MAX_BYTES));
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
