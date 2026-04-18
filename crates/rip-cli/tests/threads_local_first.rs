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

async fn run_json(rip: &Path, data_dir: &Path, workspace_dir: &Path, args: &[&str]) -> Value {
    let mut cmd = Command::new(rip);
    cmd.args(args)
        .env("RIP_DATA_DIR", data_dir)
        .env("RIP_WORKSPACE_ROOT", workspace_dir);
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

async fn run_jsonl(rip: &Path, data_dir: &Path, workspace_dir: &Path, args: &[&str]) -> Vec<Event> {
    let mut cmd = Command::new(rip);
    cmd.args(args)
        .env("RIP_DATA_DIR", data_dir)
        .env("RIP_WORKSPACE_ROOT", workspace_dir);
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

async fn wait_for_session_end(data_dir: &Path, session_id: &str, timeout: Duration) {
    let deadline = std::time::Instant::now() + timeout;
    let log_path = data_dir.join("events.jsonl");
    loop {
        if let Ok(log) = std::fs::read_to_string(&log_path) {
            let ended = log
                .lines()
                .filter(|line| !line.trim().is_empty())
                .filter_map(|line| serde_json::from_str::<Event>(line).ok())
                .any(|event| {
                    event.session_id == session_id
                        && matches!(event.kind, EventKind::SessionEnded { .. })
                });
            if ended {
                return;
            }
        }

        if std::time::Instant::now() >= deadline {
            panic!("timed out waiting for session {session_id} to end");
        }

        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn terminate_authority(data_dir: &Path) {
    let Ok(Some(meta)) = ripd::read_authority_meta(data_dir) else {
        return;
    };
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &meta.pid.to_string()])
        .status();
    for _ in 0..50 {
        if matches!(ripd::pid_liveness(meta.pid), ripd::PidLiveness::Dead) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &meta.pid.to_string()])
        .status();
}

#[tokio::test]
async fn rip_threads_local_first_covers_management_flows() {
    let rip = rip_bin();

    let root = unique_tmp_root("rip-threads-local-first");
    let data_dir = root.join("data");
    let workspace_dir = root.join("workspace");
    std::fs::create_dir_all(&workspace_dir).expect("workspace");

    let ensured = run_json(&rip, &data_dir, &workspace_dir, &["threads", "ensure"]).await;
    let thread_id = ensured
        .get("thread_id")
        .and_then(|value| value.as_str())
        .unwrap_or_else(|| panic!("expected thread_id in ensure response: {ensured}"))
        .to_string();

    let listed = run_json(&rip, &data_dir, &workspace_dir, &["threads", "list"]).await;
    assert!(
        listed.as_array().into_iter().flatten().any(|entry| entry
            .get("thread_id")
            .and_then(|value| value.as_str())
            == Some(thread_id.as_str())),
        "expected threads list to include {thread_id}: {listed}"
    );

    let got = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &["threads", "get", &thread_id],
    )
    .await;
    assert_eq!(
        got.get("thread_id").and_then(|value| value.as_str()),
        Some(thread_id.as_str())
    );

    let posted = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &[
            "threads",
            "post-message",
            &thread_id,
            "--content",
            "hello from integration",
            "--actor-id",
            "user",
            "--origin",
            "integration-test",
        ],
    )
    .await;
    let message_id = posted
        .get("message_id")
        .and_then(|value| value.as_str())
        .unwrap_or_else(|| panic!("expected message_id in post response: {posted}"))
        .to_string();
    let session_id = posted
        .get("session_id")
        .and_then(|value| value.as_str())
        .unwrap_or_else(|| panic!("expected session_id in post response: {posted}"))
        .to_string();

    wait_for_session_end(&data_dir, &session_id, Duration::from_secs(5)).await;

    let cursor_status = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &["threads", "provider-cursor-status", &thread_id],
    )
    .await;
    assert_eq!(
        cursor_status
            .get("thread_id")
            .and_then(|value| value.as_str()),
        Some(thread_id.as_str())
    );

    let rotated = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &[
            "threads",
            "provider-cursor-rotate",
            &thread_id,
            "--reason",
            "coverage-test",
            "--actor-id",
            "user",
            "--origin",
            "integration-test",
        ],
    )
    .await;
    assert_eq!(
        rotated.get("thread_id").and_then(|value| value.as_str()),
        Some(thread_id.as_str())
    );

    let context_status = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &[
            "threads",
            "context-selection-status",
            &thread_id,
            "--limit",
            "1",
        ],
    )
    .await;
    assert_eq!(
        context_status
            .get("thread_id")
            .and_then(|value| value.as_str()),
        Some(thread_id.as_str())
    );

    let cut_points = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &[
            "threads",
            "compaction-cut-points",
            &thread_id,
            "--stride-messages",
            "1",
            "--limit",
            "1",
        ],
    )
    .await;
    assert_eq!(
        cut_points.get("thread_id").and_then(|value| value.as_str()),
        Some(thread_id.as_str())
    );

    let compaction_status = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &[
            "threads",
            "compaction-status",
            &thread_id,
            "--stride-messages",
            "1",
        ],
    )
    .await;
    assert_eq!(
        compaction_status
            .get("thread_id")
            .and_then(|value| value.as_str()),
        Some(thread_id.as_str())
    );

    let auto_schedule = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &[
            "threads",
            "compaction-auto-schedule",
            &thread_id,
            "--stride-messages",
            "1",
            "--dry-run",
            "--no-execute",
            "--actor-id",
            "user",
            "--origin",
            "integration-test",
        ],
    )
    .await;
    assert_eq!(
        auto_schedule
            .get("thread_id")
            .and_then(|value| value.as_str()),
        Some(thread_id.as_str())
    );

    let checkpoint = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &[
            "threads",
            "compaction-checkpoint",
            &thread_id,
            "--summary-markdown",
            "checkpoint summary",
            "--to-message-id",
            &message_id,
            "--actor-id",
            "user",
            "--origin",
            "integration-test",
        ],
    )
    .await;
    assert_eq!(
        checkpoint.get("thread_id").and_then(|value| value.as_str()),
        Some(thread_id.as_str())
    );

    let branched = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &[
            "threads",
            "branch",
            &thread_id,
            "--title",
            "branch child",
            "--from-message-id",
            &message_id,
            "--actor-id",
            "user",
            "--origin",
            "integration-test",
        ],
    )
    .await;
    assert_eq!(
        branched
            .get("parent_thread_id")
            .and_then(|value| value.as_str()),
        Some(thread_id.as_str())
    );

    let handoff = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &[
            "threads",
            "handoff",
            &thread_id,
            "--title",
            "handoff child",
            "--summary-markdown",
            "handoff summary",
            "--from-message-id",
            &message_id,
            "--actor-id",
            "user",
            "--origin",
            "integration-test",
        ],
    )
    .await;
    assert_eq!(
        handoff
            .get("from_thread_id")
            .and_then(|value| value.as_str()),
        Some(thread_id.as_str())
    );

    let events = run_jsonl(
        &rip,
        &data_dir,
        &workspace_dir,
        &["threads", "events", &thread_id, "--max-events", "6"],
    )
    .await;
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ContinuityCreated { .. })),
        "expected continuity_created frame in thread stream"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ContinuityMessageAppended { .. })),
        "expected continuity_message_appended frame in thread stream"
    );

    terminate_authority(&data_dir).await;
    let _ = std::fs::remove_dir_all(&root);
}
