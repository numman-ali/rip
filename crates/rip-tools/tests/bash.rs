mod common;
mod shell_common;

use std::process::Command;

use common::setup_registry;
use rip_tools::ToolInvocation;
use serde_json::json;
use shell_common::{env_lock, EnvGuard};
use tempfile::tempdir;

#[tokio::test]
async fn bash_runs_command_if_available() {
    let _lock = env_lock().lock().await;
    if Command::new("bash")
        .arg("-c")
        .arg("echo test")
        .output()
        .is_err()
    {
        return;
    }

    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let bash = registry.get("bash").expect("bash tool");

    let output = bash(ToolInvocation {
        name: "bash".to_string(),
        args: json!({"command": "echo bash", "cwd": ".", "env": {"RIP_TEST": "1"}}),
        timeout_ms: None,
    })
    .await;

    let joined = output.stdout.join("\n");
    assert!(joined.contains("bash"));
}

#[tokio::test]
async fn bash_invalid_args() {
    let _lock = env_lock().lock().await;
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let bash = registry.get("bash").expect("bash tool");

    let output = bash(ToolInvocation {
        name: "bash".to_string(),
        args: json!("nope"),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn bash_rejects_invalid_cwd() {
    let _lock = env_lock().lock().await;
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let bash = registry.get("bash").expect("bash tool");

    let output = bash(ToolInvocation {
        name: "bash".to_string(),
        args: json!({"command": "echo ok", "cwd": "../"}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}

#[cfg(unix)]
#[tokio::test]
async fn bash_reports_missing_program() {
    let _lock = env_lock().lock().await;
    let _guard = EnvGuard::set("PATH", "");
    let _shell_guard = EnvGuard::set("SHELL", "/nope");

    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let bash = registry.get("bash").expect("bash tool");

    let output = bash(ToolInvocation {
        name: "bash".to_string(),
        args: json!({"command": "echo ok"}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}
