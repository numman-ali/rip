mod common;
mod shell_common;

use std::fs;

use common::setup_registry;
use rip_tools::ToolInvocation;
use serde_json::json;
use shell_common::{env_lock, EnvGuard};
use tempfile::tempdir;

#[tokio::test]
async fn shell_runs_command() {
    let _lock = env_lock().lock().await;
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let shell = registry.get("shell").expect("shell tool");

    let output = shell(ToolInvocation {
        name: "shell".to_string(),
        args: json!({"command": "echo hello"}),
        timeout_ms: None,
    })
    .await;

    let joined = output.stdout.join("\n");
    assert!(joined.to_lowercase().contains("hello"));
}

#[tokio::test]
async fn shell_accepts_env() {
    let _lock = env_lock().lock().await;
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let shell = registry.get("shell").expect("shell tool");

    let output = shell(ToolInvocation {
        name: "shell".to_string(),
        args: json!({"command": "echo hello", "env": {"RIP_TEST": "1"}}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 0);
}

#[tokio::test]
async fn shell_rejects_invalid_cwd() {
    let _lock = env_lock().lock().await;
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let shell = registry.get("shell").expect("shell tool");

    let output = shell(ToolInvocation {
        name: "shell".to_string(),
        args: json!({"command": "echo hello", "cwd": "../"}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}

#[tokio::test]
async fn shell_invalid_args() {
    let _lock = env_lock().lock().await;
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let shell = registry.get("shell").expect("shell tool");

    let output = shell(ToolInvocation {
        name: "shell".to_string(),
        args: json!("nope"),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn shell_accepts_cwd() {
    let _lock = env_lock().lock().await;
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::create_dir_all(root.join("subdir")).expect("dir");
    let registry = setup_registry(root);
    let shell = registry.get("shell").expect("shell tool");

    let output = shell(ToolInvocation {
        name: "shell".to_string(),
        args: json!({"command": "echo hello", "cwd": "subdir"}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 0);
}

#[cfg(unix)]
#[tokio::test]
async fn shell_reports_missing_program() {
    let _lock = env_lock().lock().await;
    let _guard = EnvGuard::set("SHELL", "/nope");
    let _path_guard = EnvGuard::set("PATH", "");

    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let shell = registry.get("shell").expect("shell tool");

    let output = shell(ToolInvocation {
        name: "shell".to_string(),
        args: json!({"command": "echo hello"}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}
