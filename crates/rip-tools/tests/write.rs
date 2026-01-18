mod common;

use std::fs;

use common::setup_registry;
use rip_tools::ToolInvocation;
use serde_json::json;
use tempfile::tempdir;

#[tokio::test]
async fn write_overwrites_and_appends() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();

    let registry = setup_registry(root);
    let write = registry.get("write").expect("write tool");

    let output = write(ToolInvocation {
        name: "write".to_string(),
        args: json!({"path": "out.txt", "content": "hello"}),
        timeout_ms: None,
    })
    .await;
    assert_eq!(output.exit_code, 0);

    let output = write(ToolInvocation {
        name: "write".to_string(),
        args: json!({"path": "out.txt", "content": " world", "append": true}),
        timeout_ms: None,
    })
    .await;
    assert_eq!(output.exit_code, 0);

    let content = fs::read_to_string(root.join("out.txt")).expect("read");
    assert_eq!(content, "hello world");
}

#[tokio::test]
async fn write_non_atomic() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let write = registry.get("write").expect("write tool");

    let output = write(ToolInvocation {
        name: "write".to_string(),
        args: json!({"path": "plain.txt", "content": "hi", "atomic": false}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 0);
    let content = fs::read_to_string(root.join("plain.txt")).expect("read");
    assert_eq!(content, "hi");
}

#[tokio::test]
async fn write_invalid_args() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();

    let registry = setup_registry(root);
    let write = registry.get("write").expect("write tool");
    let output = write(ToolInvocation {
        name: "write".to_string(),
        args: json!("nope"),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn write_append_requires_existing_file_when_create_false() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();

    let registry = setup_registry(root);
    let write = registry.get("write").expect("write tool");
    let output = write(ToolInvocation {
        name: "write".to_string(),
        args: json!({"path": "missing.txt", "content": "hi", "append": true, "create": false}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}

#[tokio::test]
async fn write_atomic_overwrites_existing_file() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();

    let registry = setup_registry(root);
    let write = registry.get("write").expect("write tool");
    write(ToolInvocation {
        name: "write".to_string(),
        args: json!({"path": "atomic.txt", "content": "first"}),
        timeout_ms: None,
    })
    .await;

    let output = write(ToolInvocation {
        name: "write".to_string(),
        args: json!({"path": "atomic.txt", "content": "second"}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 0);
    let content = fs::read_to_string(root.join("atomic.txt")).expect("read");
    assert_eq!(content, "second");
}

#[tokio::test]
async fn write_non_atomic_directory_fails() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::create_dir_all(root.join("dir")).expect("dir");

    let registry = setup_registry(root);
    let write = registry.get("write").expect("write tool");
    let output = write(ToolInvocation {
        name: "write".to_string(),
        args: json!({"path": "dir", "content": "hi", "atomic": false}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}

#[tokio::test]
async fn write_rejects_absolute_path() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let abs = std::env::current_dir().expect("cwd");

    let registry = setup_registry(root);
    let write = registry.get("write").expect("write tool");
    let output = write(ToolInvocation {
        name: "write".to_string(),
        args: json!({"path": abs.to_string_lossy(), "content": "nope"}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}

#[tokio::test]
async fn write_rejects_parent_path() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();

    let registry = setup_registry(root);
    let write = registry.get("write").expect("write tool");
    let output = write(ToolInvocation {
        name: "write".to_string(),
        args: json!({"path": "../nope.txt", "content": "nope"}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}
