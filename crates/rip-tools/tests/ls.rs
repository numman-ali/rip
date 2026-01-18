mod common;

use std::fs;

use common::setup_registry;
use rip_tools::ToolInvocation;
use serde_json::json;
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn ls_lists_entries() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::create_dir_all(root.join("a")).expect("dir");
    fs::write(root.join("a").join("file.txt"), "hi").expect("write");
    fs::write(root.join("root.txt"), "hi").expect("write");

    let registry = setup_registry(root);
    let ls = registry.get("ls").expect("ls tool");

    let output = ls(ToolInvocation {
        name: "ls".to_string(),
        args: json!({"path": ".", "recursive": false}),
        timeout_ms: None,
    })
    .await;

    let joined = output.stdout.join("\n");
    assert!(joined.contains("root.txt"));
    assert!(joined.contains("a"));

    let output = ls(ToolInvocation {
        name: "ls".to_string(),
        args: json!({"path": ".", "recursive": true}),
        timeout_ms: None,
    })
    .await;

    let joined = output.stdout.join("\n");
    assert!(joined.contains("a/file.txt"));
}

#[tokio::test]
async fn ls_respects_include_exclude() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("a.txt"), "hi").expect("write");
    fs::write(root.join("b.log"), "hi").expect("write");

    let registry = setup_registry(root);
    let ls = registry.get("ls").expect("ls tool");
    let output = ls(ToolInvocation {
        name: "ls".to_string(),
        args: json!({
            "path": ".",
            "recursive": false,
            "include": ["**/*.txt"],
            "exclude": ["**/*.log"]
        }),
        timeout_ms: None,
    })
    .await;

    let joined = output.stdout.join("\n");
    assert!(joined.contains("a.txt"));
    assert!(!joined.contains("b.log"));
}

#[tokio::test]
async fn ls_includes_hidden_when_requested() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join(".hidden"), "hi").expect("write");

    let registry = setup_registry(root);
    let ls = registry.get("ls").expect("ls tool");
    let output = ls(ToolInvocation {
        name: "ls".to_string(),
        args: json!({"path": ".", "include_hidden": true}),
        timeout_ms: None,
    })
    .await;

    let joined = output.stdout.join("\n");
    assert!(joined.contains(".hidden"));
}

#[tokio::test]
async fn ls_rejects_invalid_glob() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let ls = registry.get("ls").expect("ls tool");

    let output = ls(ToolInvocation {
        name: "ls".to_string(),
        args: json!({"path": ".", "include": ["["]}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn ls_invalid_args() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let ls = registry.get("ls").expect("ls tool");

    let output = ls(ToolInvocation {
        name: "ls".to_string(),
        args: json!("nope"),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn ls_rejects_parent_path() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let ls = registry.get("ls").expect("ls tool");

    let output = ls(ToolInvocation {
        name: "ls".to_string(),
        args: json!({"path": "../"}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}

#[tokio::test]
async fn ls_rejects_invalid_exclude_glob() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let ls = registry.get("ls").expect("ls tool");

    let output = ls(ToolInvocation {
        name: "ls".to_string(),
        args: json!({"path": ".", "exclude": ["["]}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[cfg(unix)]
#[tokio::test]
async fn ls_reports_unreadable_entries() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let locked = root.join("locked");
    fs::create_dir_all(&locked).expect("dir");
    fs::write(locked.join("file.txt"), "hi").expect("write");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");

    let registry = setup_registry(root);
    let ls = registry.get("ls").expect("ls tool");
    let output = ls(ToolInvocation {
        name: "ls".to_string(),
        args: json!({"path": ".", "recursive": true}),
        timeout_ms: None,
    })
    .await;

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).expect("chmod");
    assert!(!output.stderr.is_empty());
}
