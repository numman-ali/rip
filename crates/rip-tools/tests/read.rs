mod common;

use std::fs;

use common::setup_registry;
use rip_tools::ToolInvocation;
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn read_respects_line_ranges() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("notes.txt"), "one\nTwo\nthree\n").expect("write");

    let registry = setup_registry(root);
    let handler = registry.get("read").expect("read tool");
    let output = handler(ToolInvocation {
        name: "read".to_string(),
        args: json!({"path": "notes.txt", "start_line": 2, "end_line": 3}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout.len(), 1);
    assert_eq!(output.stdout[0], "Two\nthree\n");
}

#[tokio::test]
async fn read_respects_max_bytes() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("short.txt"), "abcdef").expect("write");

    let registry = setup_registry(root);
    let handler = registry.get("read").expect("read tool");
    let output = handler(ToolInvocation {
        name: "read".to_string(),
        args: json!({"path": "short.txt", "max_bytes": 3}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout[0], "abc");
}

#[tokio::test]
async fn read_rejects_invalid_range() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("bad.txt"), "one\n").expect("write");

    let registry = setup_registry(root);
    let handler = registry.get("read").expect("read tool");
    let output = handler(ToolInvocation {
        name: "read".to_string(),
        args: json!({"path": "bad.txt", "start_line": 3, "end_line": 2}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn read_rejects_zero_end_line() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("bad.txt"), "one\n").expect("write");

    let registry = setup_registry(root);
    let handler = registry.get("read").expect("read tool");
    let output = handler(ToolInvocation {
        name: "read".to_string(),
        args: json!({"path": "bad.txt", "end_line": 0}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn read_rejects_zero_line() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("bad.txt"), "one\n").expect("write");

    let registry = setup_registry(root);
    let handler = registry.get("read").expect("read tool");
    let output = handler(ToolInvocation {
        name: "read".to_string(),
        args: json!({"path": "bad.txt", "start_line": 0}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn read_invalid_args() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();

    let registry = setup_registry(root);
    let handler = registry.get("read").expect("read tool");
    let output = handler(ToolInvocation {
        name: "read".to_string(),
        args: json!("nope"),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn read_rejects_parent_path() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();

    let registry = setup_registry(root);
    let handler = registry.get("read").expect("read tool");
    let output = handler(ToolInvocation {
        name: "read".to_string(),
        args: json!({"path": "../nope.txt"}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}

#[tokio::test]
async fn read_stops_at_end_line() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("notes.txt"), "one\ntwo\nthree\n").expect("write");

    let registry = setup_registry(root);
    let handler = registry.get("read").expect("read tool");
    let output = handler(ToolInvocation {
        name: "read".to_string(),
        args: json!({"path": "notes.txt", "end_line": 1}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.stdout[0], "one\n");
}

#[tokio::test]
async fn read_rejects_absolute_path() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let abs = std::env::current_dir().expect("cwd");

    let registry = setup_registry(root);
    let handler = registry.get("read").expect("read tool");
    let output = handler(ToolInvocation {
        name: "read".to_string(),
        args: json!({"path": abs.to_string_lossy()}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}

#[tokio::test]
async fn read_missing_file() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();

    let registry = setup_registry(root);
    let handler = registry.get("read").expect("read tool");
    let output = handler(ToolInvocation {
        name: "read".to_string(),
        args: json!({"path": "missing.txt"}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}

#[tokio::test]
async fn read_invalid_utf8_reports_failure() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("bad.bin"), [0xff]).expect("write");

    let registry = setup_registry(root);
    let handler = registry.get("read").expect("read tool");
    let output = handler(ToolInvocation {
        name: "read".to_string(),
        args: json!({"path": "bad.bin"}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}
