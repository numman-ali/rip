#![cfg(not(windows))]

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

#[tokio::test]
async fn local_authority_allows_parallel_local_clients_without_seq_corruption() {
    let rip = rip_bin();

    let root = unique_tmp_root("rip-local-authority-multi");
    let data_dir = root.join("data");
    let workspace_dir = root.join("workspace");
    std::fs::create_dir_all(&workspace_dir).expect("workspace");

    let tool_prompt = r#"{"tool":"bash","args":{"command":"set -euo pipefail; mkdir .rip_test_lock; sleep 0.2; rmdir .rip_test_lock","cwd":"."}}"#;

    let run_one = async {
        Command::new(&rip)
            .args(["run", tool_prompt, "--view", "raw"])
            .env("RIP_DATA_DIR", &data_dir)
            .env("RIP_WORKSPACE_ROOT", &workspace_dir)
            .output()
            .await
            .expect("run one")
    };
    let run_two = async {
        Command::new(&rip)
            .args(["run", tool_prompt, "--view", "raw"])
            .env("RIP_DATA_DIR", &data_dir)
            .env("RIP_WORKSPACE_ROOT", &workspace_dir)
            .output()
            .await
            .expect("run two")
    };

    let (out_one, out_two) = tokio::join!(run_one, run_two);

    let events_one = parse_event_lines("run one", &out_one.stdout);
    let events_two = parse_event_lines("run two", &out_two.stdout);

    let exit_one = events_one.iter().find_map(|event| match &event.kind {
        EventKind::ToolEnded { exit_code, .. } => Some(*exit_code),
        _ => None,
    });
    let exit_two = events_two.iter().find_map(|event| match &event.kind {
        EventKind::ToolEnded { exit_code, .. } => Some(*exit_code),
        _ => None,
    });
    assert_eq!(exit_one, Some(0), "expected run one tool exit_code=0");
    assert_eq!(exit_two, Some(0), "expected run two tool exit_code=0");

    let log_path = data_dir.join("events.jsonl");
    let raw = std::fs::read_to_string(&log_path).expect("event log");
    let events: Vec<Event> = raw
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Event>(line).expect("event json"))
        .collect();
    validate_seq_contiguity(&events);

    // Best-effort cleanup: stop the local authority so we don't leave background processes.
    if let Ok(Some(meta)) = ripd::read_authority_meta(&data_dir) {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &meta.pid.to_string()])
            .status();
    }

    // Give the authority a moment to exit before deleting the store directory.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn local_authority_recovers_from_stale_lock_under_concurrency_without_mutation_reordering() {
    let rip = rip_bin();

    let root = unique_tmp_root("rip-local-authority-stale-lock");
    let data_dir = root.join("data");
    let workspace_dir = root.join("workspace");
    std::fs::create_dir_all(&workspace_dir).expect("workspace");

    let ensured = Command::new(&rip)
        .args(["threads", "ensure"])
        .env("RIP_DATA_DIR", &data_dir)
        .env("RIP_WORKSPACE_ROOT", &workspace_dir)
        .output()
        .await
        .expect("threads ensure");
    assert!(
        ensured.status.success(),
        "expected threads ensure exit=0; stderr={}",
        String::from_utf8_lossy(&ensured.stderr)
    );

    let meta = ripd::read_authority_meta(&data_dir)
        .expect("read meta")
        .unwrap_or_else(|| panic!("expected authority meta at {}", data_dir.display()));

    let _ = std::process::Command::new("kill")
        .args(["-KILL", &meta.pid.to_string()])
        .status();

    for _ in 0..50 {
        if matches!(ripd::pid_liveness(meta.pid), ripd::PidLiveness::Dead) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        matches!(ripd::pid_liveness(meta.pid), ripd::PidLiveness::Dead),
        "expected authority pid {} to be dead after kill -KILL",
        meta.pid
    );

    let lock_path = ripd::authority_lock_path(&data_dir);
    assert!(
        lock_path.exists(),
        "expected stale authority lock at {}",
        lock_path.display()
    );

    let prompt_a = r#"{"tool":"bash","args":{"command":"set -euo pipefail; echo A_start >> order.txt; sleep 0.2; echo A_end >> order.txt","cwd":"."}}"#;
    let prompt_b = r#"{"tool":"bash","args":{"command":"set -euo pipefail; echo B_start >> order.txt; sleep 0.2; echo B_end >> order.txt","cwd":"."}}"#;

    let run_a = async {
        Command::new(&rip)
            .args(["run", prompt_a, "--view", "raw"])
            .env("RIP_DATA_DIR", &data_dir)
            .env("RIP_WORKSPACE_ROOT", &workspace_dir)
            .output()
            .await
            .expect("run a")
    };
    let run_b = async {
        Command::new(&rip)
            .args(["run", prompt_b, "--view", "raw"])
            .env("RIP_DATA_DIR", &data_dir)
            .env("RIP_WORKSPACE_ROOT", &workspace_dir)
            .output()
            .await
            .expect("run b")
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

    let order_path = workspace_dir.join("order.txt");
    let order = std::fs::read_to_string(&order_path).expect("order.txt");
    let lines: Vec<&str> = order
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let a_then_b = ["A_start", "A_end", "B_start", "B_end"];
    let b_then_a = ["B_start", "B_end", "A_start", "A_end"];
    assert!(
        lines == a_then_b || lines == b_then_a,
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

    // Best-effort cleanup: stop the local authority so we don't leave background processes.
    if let Ok(Some(meta)) = ripd::read_authority_meta(&data_dir) {
        let _ = std::process::Command::new("kill")
            .args(["-KILL", &meta.pid.to_string()])
            .status();
    }

    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = std::fs::remove_dir_all(&root);
}
