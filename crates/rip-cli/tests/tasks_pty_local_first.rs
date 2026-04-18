#![cfg(not(windows))]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rip_kernel::{Event, EventKind};
use serde_json::Value;
use tokio::process::Command;

fn unique_tmp_root(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{unique}", std::process::id()))
}

fn rip_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_rip") {
        return PathBuf::from(path);
    }

    let exe = std::env::current_exe().expect("current_exe");
    let debug_dir = exe
        .parent()
        .and_then(|path| path.parent())
        .expect("debug dir");
    let candidate = debug_dir.join("rip");
    assert!(
        candidate.exists(),
        "expected rip binary at {}",
        candidate.display()
    );
    candidate
}

fn strip_openresponses_env(cmd: &mut Command) {
    cmd.env_remove("RIP_OPENRESPONSES_ENDPOINT")
        .env_remove("RIP_OPENRESPONSES_API_KEY")
        .env_remove("RIP_OPENRESPONSES_MODEL")
        .env_remove("RIP_OPENRESPONSES_TOOL_CHOICE")
        .env_remove("RIP_OPENRESPONSES_FOLLOWUP_USER_MESSAGE")
        .env_remove("RIP_OPENRESPONSES_STATELESS_HISTORY")
        .env_remove("RIP_OPENRESPONSES_PARALLEL_TOOL_CALLS")
        .env_remove("OPENAI_API_KEY")
        .env_remove("OPENROUTER_API_KEY");
}

struct KillOnDrop {
    pid: Option<u32>,
}

impl KillOnDrop {
    fn new(pid: u32) -> Self {
        Self { pid: Some(pid) }
    }

    fn disarm(&mut self) {
        self.pid = None;
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let Some(pid) = self.pid else {
            return;
        };
        let _ = std::process::Command::new("kill")
            .args(["-KILL", &pid.to_string()])
            .status();
    }
}

async fn run_json(rip: &Path, data_dir: &Path, workspace_dir: &Path, args: &[&str]) -> Value {
    let mut cmd = Command::new(rip);
    cmd.args(args)
        .env("RIP_DATA_DIR", data_dir)
        .env("RIP_WORKSPACE_ROOT", workspace_dir)
        .env("RIP_TASKS_ALLOW_PTY", "1");
    strip_openresponses_env(&mut cmd);
    let out = cmd.output().await.expect("rip command");
    assert!(
        out.status.success(),
        "expected `{}` to succeed; stderr={}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|err| panic!("expected json stdout for `{}`: {err}", args.join(" ")))
}

async fn run_ok(rip: &Path, data_dir: &Path, workspace_dir: &Path, args: &[&str]) {
    let mut cmd = Command::new(rip);
    cmd.args(args)
        .env("RIP_DATA_DIR", data_dir)
        .env("RIP_WORKSPACE_ROOT", workspace_dir)
        .env("RIP_TASKS_ALLOW_PTY", "1");
    strip_openresponses_env(&mut cmd);
    let out = cmd.output().await.expect("rip command");
    assert!(
        out.status.success(),
        "expected `{}` to succeed; stderr={}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

async fn run_jsonl(rip: &Path, data_dir: &Path, workspace_dir: &Path, args: &[&str]) -> Vec<Event> {
    let mut cmd = Command::new(rip);
    cmd.args(args)
        .env("RIP_DATA_DIR", data_dir)
        .env("RIP_WORKSPACE_ROOT", workspace_dir)
        .env("RIP_TASKS_ALLOW_PTY", "1");
    strip_openresponses_env(&mut cmd);
    let out = cmd.output().await.expect("rip command");
    assert!(
        out.status.success(),
        "expected `{}` to succeed; stderr={}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Event>(line).unwrap_or_else(|err| {
                panic!("invalid frame json for `{}`: {err}: {line}", args.join(" "))
            })
        })
        .collect()
}

async fn wait_for_task_status(
    rip: &Path,
    data_dir: &Path,
    workspace_dir: &Path,
    task_id: &str,
    expected: &[&str],
    timeout: Duration,
) -> Value {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let payload = run_json(rip, data_dir, workspace_dir, &["tasks", "status", task_id]).await;
        let status = payload
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("<missing>");
        if expected.contains(&status) {
            return payload;
        }

        if std::time::Instant::now() >= deadline {
            panic!("timed out waiting for status {expected:?}; last={payload}");
        }

        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn wait_for_task_output_contains(
    rip: &Path,
    data_dir: &Path,
    workspace_dir: &Path,
    task_id: &str,
    stream: &str,
    needle: &str,
    timeout: Duration,
) -> Value {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let payload = run_json(
            rip,
            data_dir,
            workspace_dir,
            &["tasks", "output", task_id, "--stream", stream],
        )
        .await;
        let content = payload
            .get("content")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if content.contains(needle) {
            return payload;
        }

        if std::time::Instant::now() >= deadline {
            panic!("timed out waiting for output containing {needle:?}; last={payload}");
        }

        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn terminate_authority(meta: &ripd::AuthorityMeta) -> bool {
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &meta.pid.to_string()])
        .status();
    for _ in 0..50 {
        if matches!(ripd::pid_liveness(meta.pid), ripd::PidLiveness::Dead) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &meta.pid.to_string()])
        .status();
    for _ in 0..50 {
        if matches!(ripd::pid_liveness(meta.pid), ripd::PidLiveness::Dead) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

#[tokio::test]
async fn rip_tasks_pty_local_first_covers_interactive_controls() {
    let rip = rip_bin();

    let root = unique_tmp_root("rip-tasks-pty-local-first");
    let data_dir = root.join("data");
    let workspace_dir = root.join("workspace");
    std::fs::create_dir_all(&workspace_dir).expect("workspace");

    let task_args = r#"{
  "command":"trap 'exit 0' TERM; stty -echo; while IFS= read -r line; do printf 'echo:%s\n' \"$line\"; done",
  "rows":24,
  "cols":80
}"#;
    let spawned = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &[
            "tasks",
            "spawn",
            "--tool",
            "bash",
            "--execution-mode",
            "pty",
            "--args",
            task_args,
        ],
    )
    .await;
    let task_id = spawned
        .get("task_id")
        .and_then(|value| value.as_str())
        .unwrap_or_else(|| panic!("expected task_id in spawn response; got {spawned}"))
        .to_string();

    let meta = ripd::read_authority_meta(&data_dir)
        .expect("read authority meta")
        .unwrap_or_else(|| panic!("expected authority meta at {}", data_dir.display()));
    let mut authority_kill = KillOnDrop::new(meta.pid);

    let _running = wait_for_task_status(
        &rip,
        &data_dir,
        &workspace_dir,
        &task_id,
        &["running"],
        Duration::from_secs(3),
    )
    .await;

    run_ok(
        &rip,
        &data_dir,
        &workspace_dir,
        &["tasks", "stdin", &task_id, "--text", "hello_from_pty"],
    )
    .await;

    let output = wait_for_task_output_contains(
        &rip,
        &data_dir,
        &workspace_dir,
        &task_id,
        "pty",
        "echo:hello_from_pty",
        Duration::from_secs(3),
    )
    .await;
    assert_eq!(
        output.get("stream").and_then(|value| value.as_str()),
        Some("pty")
    );

    run_ok(
        &rip,
        &data_dir,
        &workspace_dir,
        &["tasks", "resize", &task_id, "--rows", "30", "--cols", "100"],
    )
    .await;

    run_ok(
        &rip,
        &data_dir,
        &workspace_dir,
        &["tasks", "signal", &task_id, "TERM"],
    )
    .await;

    let terminal = wait_for_task_status(
        &rip,
        &data_dir,
        &workspace_dir,
        &task_id,
        &["exited", "cancelled", "failed"],
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(
        terminal.get("task_id").and_then(|value| value.as_str()),
        Some(task_id.as_str())
    );

    let events = run_jsonl(
        &rip,
        &data_dir,
        &workspace_dir,
        &["tasks", "events", &task_id],
    )
    .await;
    assert!(events
        .iter()
        .any(|event| { matches!(&event.kind, EventKind::ToolTaskStdinWritten { .. }) }));
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            EventKind::ToolTaskResized { rows, cols, .. } if *rows == 30 && *cols == 100
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            EventKind::ToolTaskSignalled { signal, .. } if signal == "TERM"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            &event.kind,
            EventKind::ToolTaskStatus { status, .. }
                if matches!(
                    status,
                    rip_kernel::ToolTaskStatus::Exited
                        | rip_kernel::ToolTaskStatus::Cancelled
                        | rip_kernel::ToolTaskStatus::Failed
                )
        )
    }));

    if terminate_authority(&meta).await {
        authority_kill.disarm();
    }

    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = std::fs::remove_dir_all(&root);
}
