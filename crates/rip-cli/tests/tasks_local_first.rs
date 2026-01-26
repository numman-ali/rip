#![cfg(not(windows))]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        let mut cmd = Command::new(rip);
        cmd.args(["tasks", "status", task_id])
            .env("RIP_DATA_DIR", data_dir)
            .env("RIP_WORKSPACE_ROOT", workspace_dir);
        strip_openresponses_env(&mut cmd);
        let out = cmd.output().await.expect("tasks status");
        assert!(
            out.status.success(),
            "expected tasks status exit=0; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );

        let payload: Value = serde_json::from_slice(&out.stdout).expect("tasks status json");
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
    needle: &str,
    timeout: Duration,
) -> Value {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let mut cmd = Command::new(rip);
        cmd.args(["tasks", "output", task_id])
            .env("RIP_DATA_DIR", data_dir)
            .env("RIP_WORKSPACE_ROOT", workspace_dir);
        strip_openresponses_env(&mut cmd);
        let out = cmd.output().await.expect("tasks output");
        assert!(
            out.status.success(),
            "expected tasks output exit=0; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );

        let payload: Value = serde_json::from_slice(&out.stdout).expect("tasks output json");
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

#[tokio::test]
async fn rip_tasks_defaults_to_local_authority_when_server_omitted() {
    let rip = rip_bin();

    let root = unique_tmp_root("rip-tasks-local-first");
    let data_dir = root.join("data");
    let workspace_dir = root.join("workspace");
    std::fs::create_dir_all(&workspace_dir).expect("workspace");

    let mut list_cmd = Command::new(&rip);
    list_cmd
        .args(["tasks", "list"])
        .env("RIP_DATA_DIR", &data_dir)
        .env("RIP_WORKSPACE_ROOT", &workspace_dir);
    strip_openresponses_env(&mut list_cmd);
    let listed = list_cmd.output().await.expect("tasks list");
    assert!(
        listed.status.success(),
        "expected tasks list exit=0; stderr={}",
        String::from_utf8_lossy(&listed.stderr)
    );
    let list_json: Value = serde_json::from_slice(&listed.stdout).expect("tasks list json");
    assert!(
        list_json.is_array(),
        "expected tasks list json array; got {list_json}"
    );

    let meta = ripd::read_authority_meta(&data_dir)
        .expect("read authority meta")
        .unwrap_or_else(|| panic!("expected authority meta at {}", data_dir.display()));
    let mut authority_kill = KillOnDrop::new(meta.pid);

    let task_args = r#"{"command":"set -euo pipefail; echo hello_from_task; sleep 10","cwd":"."}"#;
    let mut spawn_cmd = Command::new(&rip);
    spawn_cmd
        .args(["tasks", "spawn", "--tool", "bash", "--args", task_args])
        .env("RIP_DATA_DIR", &data_dir)
        .env("RIP_WORKSPACE_ROOT", &workspace_dir);
    strip_openresponses_env(&mut spawn_cmd);
    let spawned = spawn_cmd.output().await.expect("tasks spawn");
    assert!(
        spawned.status.success(),
        "expected tasks spawn exit=0; stderr={}",
        String::from_utf8_lossy(&spawned.stderr)
    );
    let spawned_json: Value = serde_json::from_slice(&spawned.stdout).expect("tasks spawn json");
    let task_id = spawned_json
        .get("task_id")
        .and_then(|value| value.as_str())
        .unwrap_or_else(|| panic!("expected task_id in spawn response; got {spawned_json}"))
        .to_string();

    let _running_status = wait_for_task_status(
        &rip,
        &data_dir,
        &workspace_dir,
        &task_id,
        &["running"],
        Duration::from_secs(3),
    )
    .await;

    let _output = wait_for_task_output_contains(
        &rip,
        &data_dir,
        &workspace_dir,
        &task_id,
        "hello_from_task",
        Duration::from_secs(3),
    )
    .await;

    let mut cancel_cmd = Command::new(&rip);
    cancel_cmd
        .args(["tasks", "cancel", &task_id, "--reason", "test"])
        .env("RIP_DATA_DIR", &data_dir)
        .env("RIP_WORKSPACE_ROOT", &workspace_dir);
    strip_openresponses_env(&mut cancel_cmd);
    let cancelled = cancel_cmd.output().await.expect("tasks cancel");
    assert!(
        cancelled.status.success(),
        "expected tasks cancel exit=0; stderr={}",
        String::from_utf8_lossy(&cancelled.stderr)
    );

    let _cancelled_status = wait_for_task_status(
        &rip,
        &data_dir,
        &workspace_dir,
        &task_id,
        &["cancelled"],
        Duration::from_secs(5),
    )
    .await;

    let _ = std::process::Command::new("kill")
        .args(["-TERM", &meta.pid.to_string()])
        .status();
    for _ in 0..50 {
        if matches!(ripd::pid_liveness(meta.pid), ripd::PidLiveness::Dead) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if !matches!(ripd::pid_liveness(meta.pid), ripd::PidLiveness::Dead) {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", &meta.pid.to_string()])
            .status();
        for _ in 0..50 {
            if matches!(ripd::pid_liveness(meta.pid), ripd::PidLiveness::Dead) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    if matches!(ripd::pid_liveness(meta.pid), ripd::PidLiveness::Dead) {
        authority_kill.disarm();
    }

    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = std::fs::remove_dir_all(&root);
}
