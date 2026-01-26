#![cfg(not(windows))]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::Client;
use rip_kernel::{Event, EventKind, StreamKind};
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

fn parse_event_lines(label: &str, stdout: &[u8]) -> Vec<Event> {
    let text = String::from_utf8_lossy(stdout);
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<Event>(line)
                .unwrap_or_else(|err| panic!("{label}: invalid frame json: {err}: {line}"))
        })
        .collect()
}

fn validate_seq_contiguity(events: &[Event]) {
    let mut expected: HashMap<(StreamKind, &str), u64> = HashMap::new();
    for event in events {
        let key = (event.stream_kind(), event.session_id.as_str());
        let next = expected.entry(key).or_insert(0);
        assert_eq!(
            event.seq,
            *next,
            "seq gap for stream {:?}/{}: expected {}, got {}",
            event.stream_kind(),
            event.session_id,
            next,
            event.seq
        );
        *next += 1;
    }
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

async fn ping_openapi(client: &Client, endpoint: &str) -> bool {
    let url = format!("{endpoint}/openapi.json");
    match client.get(url).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

async fn wait_for_authority_meta_with_pid(
    data_dir: &Path,
    expected_pid: u32,
) -> ripd::AuthorityMeta {
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
            let meta_path = ripd::authority_meta_path(data_dir);
            let lock_path = ripd::authority_lock_path(data_dir);
            panic!(
                "timed out waiting for authority meta for pid {expected_pid}. meta_path={} lock_path={}",
                meta_path.display(),
                lock_path.display()
            );
        }

        tokio::time::sleep(Duration::from_millis(20)).await;
    }
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

#[tokio::test]
async fn rip_serve_recovers_from_stale_lock_and_cleans_up_on_sigterm() {
    let rip = rip_bin();

    let root = unique_tmp_root("rip-serve-restart");
    let data_dir = root.join("data");
    let workspace_dir = root.join("workspace");
    std::fs::create_dir_all(&workspace_dir).expect("workspace");

    // Start a local authority explicitly via `rip serve` (no client auto-start/cleanup).
    let mut serve_one_cmd = Command::new(&rip);
    serve_one_cmd
        .args(["serve"])
        .env("RIP_DATA_DIR", &data_dir)
        .env("RIP_WORKSPACE_ROOT", &workspace_dir)
        .env("RIP_SERVER_ADDR", "127.0.0.1:0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    strip_openresponses_env(&mut serve_one_cmd);
    let mut serve_one = serve_one_cmd.spawn().expect("spawn serve one");
    let serve_one_pid = serve_one.id().expect("serve one pid");
    let mut serve_one_kill = KillOnDrop::new(serve_one_pid);

    let meta_one = wait_for_authority_meta_with_pid(&data_dir, serve_one_pid).await;

    let prompt_a = r#"{"tool":"bash","args":{"command":"set -euo pipefail; echo A_start >> order_before.txt; sleep 0.2; echo A_end >> order_before.txt","cwd":"."}}"#;
    let prompt_b = r#"{"tool":"bash","args":{"command":"set -euo pipefail; echo B_start >> order_before.txt; sleep 0.2; echo B_end >> order_before.txt","cwd":"."}}"#;

    let run_a = async {
        let mut cmd = Command::new(&rip);
        cmd.args([
            "run",
            prompt_a,
            "--server",
            &meta_one.endpoint,
            "--view",
            "raw",
        ])
        .env("RIP_DATA_DIR", &data_dir)
        .env("RIP_WORKSPACE_ROOT", &workspace_dir);
        strip_openresponses_env(&mut cmd);
        cmd.output().await.expect("run a")
    };
    let run_b = async {
        let mut cmd = Command::new(&rip);
        cmd.args([
            "run",
            prompt_b,
            "--server",
            &meta_one.endpoint,
            "--view",
            "raw",
        ])
        .env("RIP_DATA_DIR", &data_dir)
        .env("RIP_WORKSPACE_ROOT", &workspace_dir);
        strip_openresponses_env(&mut cmd);
        cmd.output().await.expect("run b")
    };

    let (out_a, out_b) = tokio::join!(run_a, run_b);

    let events_a = parse_event_lines("run a", &out_a.stdout);
    let events_b = parse_event_lines("run b", &out_b.stdout);

    let exit_a = events_a.iter().find_map(|event| match &event.kind {
        EventKind::ToolEnded { exit_code, .. } => Some(*exit_code),
        _ => None,
    });
    let exit_b = events_b.iter().find_map(|event| match &event.kind {
        EventKind::ToolEnded { exit_code, .. } => Some(*exit_code),
        _ => None,
    });
    assert_eq!(exit_a, Some(0), "expected run a tool exit_code=0");
    assert_eq!(exit_b, Some(0), "expected run b tool exit_code=0");

    // Crash the authority (SIGKILL) and verify the lock is left behind.
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &serve_one_pid.to_string()])
        .status();
    let _ = serve_one.wait().await;
    serve_one_kill.disarm();

    for _ in 0..50 {
        if matches!(ripd::pid_liveness(serve_one_pid), ripd::PidLiveness::Dead) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        matches!(ripd::pid_liveness(serve_one_pid), ripd::PidLiveness::Dead),
        "expected serve one pid {} to be dead after kill -KILL",
        serve_one_pid
    );

    let lock_path = ripd::authority_lock_path(&data_dir);
    let meta_path = ripd::authority_meta_path(&data_dir);
    assert!(
        lock_path.exists(),
        "expected stale authority lock at {}",
        lock_path.display()
    );
    assert!(
        meta_path.exists(),
        "expected stale authority meta at {}",
        meta_path.display()
    );

    // Restart `rip serve` directly. This must self-heal the stale `lock.json` on startup without a client.
    let mut serve_two_cmd = Command::new(&rip);
    serve_two_cmd
        .args(["serve"])
        .env("RIP_DATA_DIR", &data_dir)
        .env("RIP_WORKSPACE_ROOT", &workspace_dir)
        .env("RIP_SERVER_ADDR", "127.0.0.1:0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    strip_openresponses_env(&mut serve_two_cmd);
    let mut serve_two = serve_two_cmd.spawn().expect("spawn serve two");
    let serve_two_pid = serve_two.id().expect("serve two pid");
    let mut serve_two_kill = KillOnDrop::new(serve_two_pid);

    let meta_two = wait_for_authority_meta_with_pid(&data_dir, serve_two_pid).await;

    let prompt_c = r#"{"tool":"bash","args":{"command":"set -euo pipefail; echo C_start >> order_after.txt; sleep 0.2; echo C_end >> order_after.txt","cwd":"."}}"#;
    let prompt_d = r#"{"tool":"bash","args":{"command":"set -euo pipefail; echo D_start >> order_after.txt; sleep 0.2; echo D_end >> order_after.txt","cwd":"."}}"#;

    let run_c = async {
        let mut cmd = Command::new(&rip);
        cmd.args([
            "run",
            prompt_c,
            "--server",
            &meta_two.endpoint,
            "--view",
            "raw",
        ])
        .env("RIP_DATA_DIR", &data_dir)
        .env("RIP_WORKSPACE_ROOT", &workspace_dir);
        strip_openresponses_env(&mut cmd);
        cmd.output().await.expect("run c")
    };
    let run_d = async {
        let mut cmd = Command::new(&rip);
        cmd.args([
            "run",
            prompt_d,
            "--server",
            &meta_two.endpoint,
            "--view",
            "raw",
        ])
        .env("RIP_DATA_DIR", &data_dir)
        .env("RIP_WORKSPACE_ROOT", &workspace_dir);
        strip_openresponses_env(&mut cmd);
        cmd.output().await.expect("run d")
    };

    let (out_c, out_d) = tokio::join!(run_c, run_d);

    let events_c = parse_event_lines("run c", &out_c.stdout);
    let events_d = parse_event_lines("run d", &out_d.stdout);

    let exit_c = events_c.iter().find_map(|event| match &event.kind {
        EventKind::ToolEnded { exit_code, .. } => Some(*exit_code),
        _ => None,
    });
    let exit_d = events_d.iter().find_map(|event| match &event.kind {
        EventKind::ToolEnded { exit_code, .. } => Some(*exit_code),
        _ => None,
    });
    assert_eq!(exit_c, Some(0), "expected run c tool exit_code=0");
    assert_eq!(exit_d, Some(0), "expected run d tool exit_code=0");

    // Workspace mutations must not interleave across concurrent clients.
    let after_path = workspace_dir.join("order_after.txt");
    let after = std::fs::read_to_string(&after_path).expect("order_after.txt");
    let lines: Vec<&str> = after
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let c_then_d = ["C_start", "C_end", "D_start", "D_end"];
    let d_then_c = ["D_start", "D_end", "C_start", "C_end"];
    assert!(
        lines == c_then_d || lines == d_then_c,
        "expected non-interleaved workspace mutations; got lines={lines:?}"
    );

    let log_path = data_dir.join("events.jsonl");
    let raw = std::fs::read_to_string(&log_path).expect("event log");
    let events: Vec<Event> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Event>(line).expect("event json"))
        .collect();
    validate_seq_contiguity(&events);

    // SIGTERM should trigger a graceful shutdown and best-effort lock/meta cleanup.
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &serve_two_pid.to_string()])
        .status();
    let _ = serve_two.wait().await;
    serve_two_kill.disarm();

    for _ in 0..50 {
        if !ripd::authority_lock_path(&data_dir).exists()
            && !ripd::authority_meta_path(&data_dir).exists()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        !ripd::authority_lock_path(&data_dir).exists(),
        "expected authority lock to be removed on shutdown: {}",
        ripd::authority_lock_path(&data_dir).display()
    );
    assert!(
        !ripd::authority_meta_path(&data_dir).exists(),
        "expected authority meta to be removed on shutdown: {}",
        ripd::authority_meta_path(&data_dir).display()
    );

    let _ = std::fs::remove_dir_all(&root);
}
