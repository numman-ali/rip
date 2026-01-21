use std::path::PathBuf;
use std::sync::Arc;

use rip_kernel::EventKind;
use rip_log::{verify_snapshot, EventLog};
use serde_json::json;
use tempfile::tempdir;
use tokio::time::{sleep, timeout, Duration};

use super::logs::{
    base64_decode, is_lower_hex_64, new_artifact_id, normalize_rel_path, read_artifact_range,
    resolve_path, truncate_utf8, TaskLogWriter,
};
use super::*;

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

#[test]
fn base64_decode_matches_known_vectors() {
    assert_eq!(base64_decode("").expect("empty"), b"");
    assert_eq!(base64_decode("aGk=").expect("hi"), b"hi");
    assert_eq!(base64_decode("aGkK").expect("hi\\n"), b"hi\n");
    assert!(
        base64_decode("aGk").is_err(),
        "length must be multiple of 4"
    );
    assert!(base64_decode("aGk*").is_err(), "invalid chars rejected");
}

#[test]
fn base64_decode_rejects_invalid_padding() {
    let err = base64_decode("AA=A").expect_err("invalid");
    assert!(err.contains("invalid base64 padding"));
}

#[tokio::test]
async fn create_task_initializes_status_and_log_refs_for_pipes() {
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
    let logs = artifacts.get("logs").expect("logs");
    let stdout_id = logs
        .get("stdout")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(is_lower_hex_64(stdout_id));
}

#[tokio::test]
async fn create_task_initializes_status_and_log_refs_for_pty() {
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
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);
    let status = handle.status().await;
    assert_eq!(status.execution_mode, ApiToolTaskExecutionMode::Pty);

    let artifacts = status.artifacts.expect("artifacts");
    let logs = artifacts.get("logs").expect("logs");
    let pty_id = logs
        .get("pty")
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(is_lower_hex_64(pty_id));
    assert!(logs.get("stdout").is_none());
    assert!(logs.get("stderr").is_none());
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
async fn task_log_writer_new_reports_create_error() {
    let dir = tempdir().expect("tmp");
    let config = TaskEngineConfig {
        workspace_root: dir.path().to_path_buf(),
        artifact_max_bytes: 128,
        max_bytes: 64,
    };
    tokio::fs::create_dir_all(config.artifacts_blobs_dir())
        .await
        .expect("mkdir");

    let err = TaskLogWriter::new(&config, "bad/id", "bad/id", 128)
        .await
        .err()
        .expect("err");
    assert!(err.contains("artifact create failed"));
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

#[test]
fn pty_control_fixture_replays_and_reads_artifact() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let fixture_dir = root.join("fixtures").join("tasks").join("pty_control");
    let log_path = fixture_dir.join("events.jsonl");
    let snapshot_path = fixture_dir
        .join("task_snapshots")
        .join("task_pty_control.json");
    let workspace_root = fixture_dir.join("workspace");

    let log = EventLog::new(log_path).expect("log");
    verify_snapshot(&log, snapshot_path).expect("snapshot");

    let config = TaskEngineConfig {
        workspace_root,
        artifact_max_bytes: 1024,
        max_bytes: 128,
    };
    let (content, bytes, total, truncated) =
        read_artifact_range(&config, &"a".repeat(64), 0, 64).expect("artifact");
    assert_eq!(content, "hi\n");
    assert_eq!(bytes, 3);
    assert_eq!(total, 3);
    assert!(!truncated);
}

#[test]
fn pipes_output_exit_fixture_replays_and_reads_artifacts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let fixture_dir = root
        .join("fixtures")
        .join("tasks")
        .join("pipes_output_exit");
    let log_path = fixture_dir.join("events.jsonl");
    let snapshot_path = fixture_dir
        .join("task_snapshots")
        .join("task_pipes_output_exit.json");
    let workspace_root = fixture_dir.join("workspace");

    let log = EventLog::new(log_path).expect("log");
    verify_snapshot(&log, snapshot_path).expect("snapshot");

    let config = TaskEngineConfig {
        workspace_root,
        artifact_max_bytes: 1024,
        max_bytes: 128,
    };

    let (stdout, stdout_bytes, stdout_total, stdout_truncated) =
        read_artifact_range(&config, &"b".repeat(64), 0, 64).expect("stdout");
    assert_eq!(stdout, "hello\n");
    assert_eq!(stdout_bytes, 6);
    assert_eq!(stdout_total, 6);
    assert!(!stdout_truncated);

    let (stderr, stderr_bytes, stderr_total, stderr_truncated) =
        read_artifact_range(&config, &"c".repeat(64), 0, 64).expect("stderr");
    assert_eq!(stderr, "err\n");
    assert_eq!(stderr_bytes, 4);
    assert_eq!(stderr_total, 4);
    assert!(!stderr_truncated);
}

#[test]
fn pipes_cancel_fixture_replays_and_reads_artifacts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let fixture_dir = root.join("fixtures").join("tasks").join("pipes_cancel");
    let log_path = fixture_dir.join("events.jsonl");
    let snapshot_path = fixture_dir
        .join("task_snapshots")
        .join("task_pipes_cancel.json");
    let workspace_root = fixture_dir.join("workspace");

    let log = EventLog::new(log_path).expect("log");
    verify_snapshot(&log, snapshot_path).expect("snapshot");

    let config = TaskEngineConfig {
        workspace_root,
        artifact_max_bytes: 1024,
        max_bytes: 128,
    };

    let (stdout, stdout_bytes, stdout_total, stdout_truncated) =
        read_artifact_range(&config, &"d".repeat(64), 0, 64).expect("stdout");
    assert_eq!(stdout, "tick\n");
    assert_eq!(stdout_bytes, 5);
    assert_eq!(stdout_total, 5);
    assert!(!stdout_truncated);

    let (stderr, stderr_bytes, stderr_total, stderr_truncated) =
        read_artifact_range(&config, &"e".repeat(64), 0, 64).expect("stderr");
    assert_eq!(stderr, "warn\n");
    assert_eq!(stderr_bytes, 5);
    assert_eq!(stderr_total, 5);
    assert!(!stderr_truncated);
}

#[tokio::test]
async fn write_stdin_rejects_non_pty_task() {
    let dir = tempdir().expect("tmp");
    let (engine, _config, _event_log, _snapshot_dir) = build_engine(&dir);
    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"printf 'hi\\n'"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pipes),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);

    let err = handle
        .write_stdin(TaskWriteStdinPayload {
            chunk_b64: "aGkK".to_string(),
        })
        .await
        .expect_err("err");
    assert!(err.contains("only supported for pty"));
}

#[tokio::test]
async fn write_stdin_rejects_before_task_ready() {
    let dir = tempdir().expect("tmp");
    let (engine, _config, _event_log, _snapshot_dir) = build_engine(&dir);
    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"stty -echo; cat"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);

    let err = handle
        .write_stdin(TaskWriteStdinPayload {
            chunk_b64: "aGkK".to_string(),
        })
        .await
        .expect_err("err");
    assert!(err.contains("not ready for interactive IO"));
}

#[tokio::test]
async fn write_stdin_rejects_invalid_base64() {
    let dir = tempdir().expect("tmp");
    let (engine, _config, _event_log, _snapshot_dir) = build_engine(&dir);
    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"stty -echo; cat"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);

    let err = handle
        .write_stdin(TaskWriteStdinPayload {
            chunk_b64: "aGk".to_string(),
        })
        .await
        .expect_err("err");
    assert!(err.contains("invalid base64"));
}

#[tokio::test]
async fn write_stdin_rejects_oversize_chunk() {
    let dir = tempdir().expect("tmp");
    let (engine, _config, _event_log, _snapshot_dir) = build_engine(&dir);
    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"stty -echo; cat"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);

    let oversized = "AAAA".repeat(2731); // 3 bytes per 4 chars => 8193 bytes
    let err = handle
        .write_stdin(TaskWriteStdinPayload {
            chunk_b64: oversized,
        })
        .await
        .expect_err("err");
    assert!(err.contains("stdin chunk too large"));
}

#[tokio::test]
async fn resize_rejects_non_pty_task() {
    let dir = tempdir().expect("tmp");
    let (engine, _config, _event_log, _snapshot_dir) = build_engine(&dir);
    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"printf 'hi\\n'"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pipes),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);

    let err = handle
        .resize(TaskResizePayload { rows: 10, cols: 10 })
        .await
        .expect_err("err");
    assert!(err.contains("only supported for pty"));
}

#[tokio::test]
async fn resize_rejects_zero_dimensions() {
    let dir = tempdir().expect("tmp");
    let (engine, _config, _event_log, _snapshot_dir) = build_engine(&dir);
    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"stty -echo; cat"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);

    let err = handle
        .resize(TaskResizePayload { rows: 0, cols: 10 })
        .await
        .expect_err("err");
    assert!(err.contains("rows and cols"));
}

#[tokio::test]
async fn signal_rejects_non_pty_task() {
    let dir = tempdir().expect("tmp");
    let (engine, _config, _event_log, _snapshot_dir) = build_engine(&dir);
    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"printf 'hi\\n'"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pipes),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);

    let err = handle
        .signal(TaskSignalPayload {
            signal: "SIGTERM".to_string(),
        })
        .await
        .expect_err("err");
    assert!(err.contains("only supported for pty"));
}

#[tokio::test]
async fn signal_rejects_empty_and_unsupported() {
    let dir = tempdir().expect("tmp");
    let (engine, _config, _event_log, _snapshot_dir) = build_engine(&dir);
    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"stty -echo; cat"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);

    let empty = handle
        .signal(TaskSignalPayload {
            signal: "   ".to_string(),
        })
        .await
        .expect_err("err");
    assert!(empty.contains("non-empty"));

    let unsupported = handle
        .signal(TaskSignalPayload {
            signal: "SIGUSR1".to_string(),
        })
        .await
        .expect_err("err");
    assert!(unsupported.contains("unsupported"));
}

#[tokio::test]
async fn signal_rejects_before_task_ready() {
    let dir = tempdir().expect("tmp");
    let (engine, _config, _event_log, _snapshot_dir) = build_engine(&dir);
    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"stty -echo; cat"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);

    let err = handle
        .signal(TaskSignalPayload {
            signal: "TERM".to_string(),
        })
        .await
        .expect_err("err");
    assert!(err.contains("not ready for interactive IO"));
}

#[tokio::test]
async fn output_rejects_unavailable_stream() {
    let dir = tempdir().expect("tmp");
    let (engine, config, _event_log, _snapshot_dir) = build_engine(&dir);
    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"printf 'hi\\n'"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);

    let err = handle
        .output(&config, TaskOutputStream::Stdout, 0, 64)
        .await
        .expect_err("err");
    assert!(err.contains("output stream not available"));
}

#[cfg(not(windows))]
#[tokio::test]
async fn pty_task_supports_stdin_resize_and_signal() {
    let dir = tempdir().expect("tmp");
    let (engine, config, event_log, snapshot_dir) = build_engine(&dir);

    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"stty -echo; cat", "rows": 24, "cols": 80}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
        origin_session_id: None,
    };

    let handle = engine.create_task(&payload);
    let driver = tokio::spawn(run_task(
        handle.clone(),
        payload,
        config.clone(),
        event_log,
        snapshot_dir.clone(),
    ));

    timeout(Duration::from_secs(2), async {
        loop {
            if handle
                .write_stdin(TaskWriteStdinPayload {
                    chunk_b64: "aGkK".to_string(),
                })
                .await
                .is_ok()
            {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("stdin ready");

    handle
        .resize(TaskResizePayload {
            rows: 30,
            cols: 100,
        })
        .await
        .expect("resize");

    timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(output) = handle.output(&config, TaskOutputStream::Pty, 0, 64).await {
                if output.content.contains("hi") {
                    break;
                }
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("pty output");

    handle
        .signal(TaskSignalPayload {
            signal: "SIGTERM".to_string(),
        })
        .await
        .expect("signal");

    driver.await.expect("task join");

    let status = handle.status().await;
    assert!(matches!(
        status.status,
        ApiToolTaskStatus::Exited | ApiToolTaskStatus::Failed | ApiToolTaskStatus::Cancelled
    ));

    let snapshot_path = snapshot_dir.join(format!("{}.json", handle.task_id));
    assert!(snapshot_path.exists(), "expected task snapshot");

    let output = handle
        .output(&config, TaskOutputStream::Pty, 0, 64)
        .await
        .expect("pty output");
    assert!(output.content.contains("hi"));
}

#[cfg(not(windows))]
#[tokio::test]
async fn pipes_task_applies_cwd_and_env() {
    let dir = tempdir().expect("tmp");
    let (engine, config, event_log, snapshot_dir) = build_engine(&dir);

    let cwd = config.workspace_root.join("subdir");
    std::fs::create_dir_all(&cwd).expect("cwd");

    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({
            "command":"echo \"$RIP_TEST_VAR\"; pwd",
            "cwd":"subdir",
            "env": {"RIP_TEST_VAR":"hello"}
        }),
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

    let stdout = handle
        .output(&config, TaskOutputStream::Stdout, 0, 256)
        .await
        .expect("stdout");
    assert!(stdout.content.contains("hello"));
    assert!(stdout.content.contains("/subdir"));
}

#[cfg(not(windows))]
#[tokio::test]
async fn pty_task_applies_cwd_and_env() {
    let dir = tempdir().expect("tmp");
    let (engine, config, event_log, snapshot_dir) = build_engine(&dir);

    let cwd = config.workspace_root.join("subdir");
    std::fs::create_dir_all(&cwd).expect("cwd");

    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({
            "command":"echo \"$RIP_TEST_VAR\"; pwd",
            "cwd":"subdir",
            "env": {"RIP_TEST_VAR":"hello"}
        }),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
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

    let pty = handle
        .output(&config, TaskOutputStream::Pty, 0, 256)
        .await
        .expect("pty");
    assert!(pty.content.contains("hello"));
    assert!(pty.content.contains("/subdir"));
}

#[cfg(not(windows))]
#[tokio::test]
async fn pipes_task_can_be_cancelled() {
    let dir = tempdir().expect("tmp");
    let (engine, config, event_log, snapshot_dir) = build_engine(&dir);

    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"printf 'tick\\n'; sleep 10"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pipes),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);
    let mut driver = tokio::spawn(run_task(
        handle.clone(),
        payload,
        config.clone(),
        event_log,
        snapshot_dir,
    ));

    timeout(Duration::from_secs(2), async {
        loop {
            if handle.status().await.status == ApiToolTaskStatus::Running {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("running");

    handle.cancel("cancel".to_string());
    timeout(Duration::from_secs(2), &mut driver)
        .await
        .expect("join")
        .expect("task join");

    let status = handle.status().await;
    assert_eq!(status.status, ApiToolTaskStatus::Cancelled);
}

#[cfg(not(windows))]
#[tokio::test]
async fn pty_task_can_be_cancelled() {
    let dir = tempdir().expect("tmp");
    let (engine, config, event_log, snapshot_dir) = build_engine(&dir);

    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"stty -echo; cat"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
        origin_session_id: None,
    };
    let handle = engine.create_task(&payload);
    let mut driver = tokio::spawn(run_task(
        handle.clone(),
        payload,
        config.clone(),
        event_log,
        snapshot_dir,
    ));

    timeout(Duration::from_secs(2), async {
        loop {
            if handle.status().await.status == ApiToolTaskStatus::Running {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("running");

    handle.cancel("cancel".to_string());
    timeout(Duration::from_secs(2), &mut driver)
        .await
        .expect("join")
        .expect("task join");

    let status = handle.status().await;
    assert_eq!(status.status, ApiToolTaskStatus::Cancelled);
    let events = handle.events_snapshot().await;
    assert!(events
        .iter()
        .any(|event| matches!(&event.kind, EventKind::ToolTaskCancelled { .. })));
}

#[cfg(not(windows))]
#[tokio::test]
async fn pty_task_can_be_signalled_via_ctrl_c() {
    let dir = tempdir().expect("tmp");
    let (engine, config, event_log, snapshot_dir) = build_engine(&dir);

    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"stty -echo; cat"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
        origin_session_id: None,
    };

    let handle = engine.create_task(&payload);
    let mut driver = tokio::spawn(run_task(
        handle.clone(),
        payload,
        config.clone(),
        event_log,
        snapshot_dir,
    ));

    timeout(Duration::from_secs(2), async {
        loop {
            if handle
                .signal(TaskSignalPayload {
                    signal: "INT".to_string(),
                })
                .await
                .is_ok()
            {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("signal ready");

    match timeout(Duration::from_millis(250), &mut driver).await {
        Ok(result) => {
            result.expect("task join");
        }
        Err(_) => {
            handle.cancel("cancel".to_string());
            timeout(Duration::from_secs(2), &mut driver)
                .await
                .expect("join")
                .expect("task join");
        }
    }

    let events = handle.events_snapshot().await;
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        EventKind::ToolTaskSignalled { signal, .. } if signal == "INT"
    )));
}

#[cfg(not(windows))]
#[tokio::test]
async fn pty_task_can_be_signalled_via_ctrl_backslash() {
    let dir = tempdir().expect("tmp");
    let (engine, config, event_log, snapshot_dir) = build_engine(&dir);

    let payload = TaskSpawnPayload {
        tool: "bash".to_string(),
        args: json!({"command":"stty -echo; trap '' QUIT; while true; do read -r _; done"}),
        title: None,
        execution_mode: Some(ApiToolTaskExecutionMode::Pty),
        origin_session_id: None,
    };

    let handle = engine.create_task(&payload);
    let mut driver = tokio::spawn(run_task(
        handle.clone(),
        payload,
        config.clone(),
        event_log,
        snapshot_dir,
    ));

    timeout(Duration::from_secs(2), async {
        loop {
            if handle
                .signal(TaskSignalPayload {
                    signal: "QUIT".to_string(),
                })
                .await
                .is_ok()
            {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("signal ready");

    timeout(Duration::from_secs(2), async {
        loop {
            let events = handle.events_snapshot().await;
            if events.iter().any(|event| {
                matches!(
                    &event.kind,
                    EventKind::ToolTaskSignalled { signal, .. } if signal == "QUIT"
                )
            }) {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("signalled");

    handle.cancel("cancel".to_string());
    timeout(Duration::from_secs(2), &mut driver)
        .await
        .expect("join")
        .expect("task join");

    let events = handle.events_snapshot().await;
    assert!(events.iter().any(|event| matches!(
        &event.kind,
        EventKind::ToolTaskSignalled { signal, .. } if signal == "QUIT"
    )));
}
