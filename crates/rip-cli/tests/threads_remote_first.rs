#![cfg(not(windows))]

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::Client;
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

async fn ping_openapi(client: &Client, endpoint: &str) -> bool {
    let url = format!("{endpoint}/openapi.json");
    match client.get(url).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

async fn wait_for_authority(data_dir: &Path, expected_pid: u32) -> ripd::AuthorityMeta {
    let client = Client::builder()
        .timeout(Duration::from_millis(250))
        .build()
        .expect("client");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(Some(meta)) = ripd::read_authority_meta(data_dir) {
            if meta.pid == expected_pid && ping_openapi(&client, &meta.endpoint).await {
                return meta;
            }
        }

        if std::time::Instant::now() >= deadline {
            panic!("timed out waiting for authority endpoint for pid {expected_pid}");
        }

        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn run_json(
    rip: &Path,
    data_dir: &Path,
    workspace_dir: &Path,
    server: &str,
    args: &[&str],
) -> Value {
    let mut full_args = vec!["threads", "--server", server];
    full_args.extend_from_slice(args);

    let mut cmd = Command::new(rip);
    cmd.args(&full_args)
        .env("RIP_DATA_DIR", data_dir)
        .env("RIP_WORKSPACE_ROOT", workspace_dir);
    strip_openresponses_env(&mut cmd);
    let out = cmd.output().await.expect("rip command");
    assert!(
        out.status.success(),
        "expected `{}` to succeed; stderr={}",
        full_args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|err| panic!("expected json stdout for `{}`: {err}", full_args.join(" ")))
}

async fn run_jsonl(
    rip: &Path,
    data_dir: &Path,
    workspace_dir: &Path,
    server: &str,
    args: &[&str],
) -> Vec<Event> {
    let mut full_args = vec!["threads", "--server", server];
    full_args.extend_from_slice(args);

    let mut cmd = Command::new(rip);
    cmd.args(&full_args)
        .env("RIP_DATA_DIR", data_dir)
        .env("RIP_WORKSPACE_ROOT", workspace_dir);
    strip_openresponses_env(&mut cmd);
    let out = cmd.output().await.expect("rip command");
    assert!(
        out.status.success(),
        "expected `{}` to succeed; stderr={}",
        full_args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Event>(line).unwrap_or_else(|err| {
                panic!(
                    "invalid frame json for `{}`: {err}: {line}",
                    full_args.join(" ")
                )
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

#[tokio::test]
async fn rip_threads_remote_mode_covers_management_flows() {
    let rip = rip_bin();

    let root = unique_tmp_root("rip-threads-remote-first");
    let data_dir = root.join("data");
    let workspace_dir = root.join("workspace");
    std::fs::create_dir_all(&workspace_dir).expect("workspace");

    let mut serve_cmd = Command::new(&rip);
    serve_cmd
        .args(["serve"])
        .env("RIP_DATA_DIR", &data_dir)
        .env("RIP_WORKSPACE_ROOT", &workspace_dir)
        .env("RIP_SERVER_ADDR", "127.0.0.1:0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    strip_openresponses_env(&mut serve_cmd);
    let mut serve = serve_cmd.spawn().expect("spawn serve");
    let serve_pid = serve.id().expect("serve pid");
    let mut serve_kill = KillOnDrop::new(serve_pid);

    let meta = wait_for_authority(&data_dir, serve_pid).await;
    let server = meta.endpoint;

    let ensured = run_json(&rip, &data_dir, &workspace_dir, &server, &["ensure"]).await;
    let thread_id = ensured
        .get("thread_id")
        .and_then(|value| value.as_str())
        .unwrap_or_else(|| panic!("expected thread_id in ensure response: {ensured}"))
        .to_string();

    let listed = run_json(&rip, &data_dir, &workspace_dir, &server, &["list"]).await;
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
        &server,
        &["get", &thread_id],
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
        &server,
        &[
            "post-message",
            &thread_id,
            "--content",
            "hello from remote integration",
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

    let _cursor_status = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &server,
        &["provider-cursor-status", &thread_id],
    )
    .await;
    let _rotated = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &server,
        &[
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
    let _context_status = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &server,
        &["context-selection-status", &thread_id, "--limit", "1"],
    )
    .await;
    let _cut_points = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &server,
        &[
            "compaction-cut-points",
            &thread_id,
            "--stride-messages",
            "1",
            "--limit",
            "1",
        ],
    )
    .await;
    let _compaction_status = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &server,
        &["compaction-status", &thread_id, "--stride-messages", "1"],
    )
    .await;
    let _compaction_auto = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &server,
        &[
            "compaction-auto",
            &thread_id,
            "--stride-messages",
            "1",
            "--dry-run",
            "--actor-id",
            "user",
            "--origin",
            "integration-test",
        ],
    )
    .await;
    let _auto_schedule = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &server,
        &[
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
    let _checkpoint = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &server,
        &[
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
    let _branched = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &server,
        &[
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
    let _handoff = run_json(
        &rip,
        &data_dir,
        &workspace_dir,
        &server,
        &[
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

    let events = run_jsonl(
        &rip,
        &data_dir,
        &workspace_dir,
        &server,
        &["events", &thread_id, "--max-events", "6"],
    )
    .await;
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ContinuityCreated { .. })),
        "expected continuity_created frame in remote thread stream"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ContinuityMessageAppended { .. })),
        "expected continuity_message_appended frame in remote thread stream"
    );

    let _ = std::process::Command::new("kill")
        .args(["-TERM", &serve_pid.to_string()])
        .status();
    let _ = serve.wait().await;
    serve_kill.disarm();
    let _ = std::fs::remove_dir_all(&root);
}
