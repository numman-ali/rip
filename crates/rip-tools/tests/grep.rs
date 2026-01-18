mod common;

use std::fs;

use common::setup_registry;
use rip_tools::ToolInvocation;
use serde_json::json;
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[tokio::test]
async fn grep_finds_matches() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("log.txt"), "alpha\nbeta\nalpha\n").expect("write");

    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");

    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({"pattern": "alpha", "path": ".", "regex": false}),
        timeout_ms: None,
    })
    .await;

    let joined = output.stdout.join("\n");
    assert!(joined.contains("log.txt:1:alpha"));
    assert!(joined.contains("log.txt:3:alpha"));
}

#[tokio::test]
async fn grep_respects_max_results() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("log.txt"), "foo\nfoo\nfoo\n").expect("write");

    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");

    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({
            "pattern": "foo",
            "path": ".",
            "regex": false,
            "max_results": 1
        }),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.stdout.len(), 1);
}

#[tokio::test]
async fn grep_regex_enabled() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("regex.txt"), "alpha\n").expect("write");

    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");
    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({"pattern": "al.*a", "path": ".", "regex": true}),
        timeout_ms: None,
    })
    .await;

    let joined = output.stdout.join("\n");
    assert!(joined.contains("regex.txt:1:alpha"));
}

#[tokio::test]
async fn grep_skips_binary() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("bin.dat"), b"foo\0bar").expect("write");

    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");
    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({"pattern": "foo", "path": ".", "regex": false}),
        timeout_ms: None,
    })
    .await;

    assert!(output.stdout.is_empty());
}

#[tokio::test]
async fn grep_respects_max_bytes() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("limit.txt"), "skip\nmatch\n").expect("write");

    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");
    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({"pattern": "match", "path": ".", "regex": false, "max_bytes": 4}),
        timeout_ms: None,
    })
    .await;

    assert!(output.stdout.is_empty());
}

#[tokio::test]
async fn grep_rejects_invalid_regex() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("bad.txt"), "hello").expect("write");

    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");
    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({"pattern": "[", "path": ".", "regex": true}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn grep_respects_include_exclude() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("a.txt"), "match").expect("write");
    fs::write(root.join("b.log"), "match").expect("write");

    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");
    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({
            "pattern": "match",
            "path": ".",
            "regex": false,
            "include": ["**/*.txt"],
            "exclude": ["**/*.log"]
        }),
        timeout_ms: None,
    })
    .await;

    let joined = output.stdout.join("\n");
    assert!(joined.contains("a.txt:1:match"));
    assert!(!joined.contains("b.log:1:match"));
}

#[tokio::test]
async fn grep_invalid_args() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");
    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!("nope"),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn grep_rejects_parent_path() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");
    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({"pattern": "a", "path": "../", "regex": false}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 1);
}

#[tokio::test]
async fn grep_rejects_invalid_include_glob() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");
    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({"pattern": "a", "path": ".", "regex": false, "include": ["["]}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn grep_rejects_invalid_exclude_glob() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");
    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({"pattern": "a", "path": ".", "regex": false, "exclude": ["["]}),
        timeout_ms: None,
    })
    .await;

    assert_eq!(output.exit_code, 2);
}

#[tokio::test]
async fn grep_invalid_utf8_reports_failure() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    fs::write(root.join("bad.bin"), [0xff]).expect("write");

    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");
    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({"pattern": "a", "path": ".", "regex": false}),
        timeout_ms: None,
    })
    .await;

    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn grep_reports_unreadable_file() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let locked = root.join("locked.txt");
    fs::write(&locked, "match").expect("write");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");

    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");
    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({"pattern": "match", "path": ".", "regex": false}),
        timeout_ms: None,
    })
    .await;

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o600)).expect("chmod");
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn grep_reports_unreadable_entries() {
    let dir = tempdir().expect("tmp");
    let root = dir.path();
    let locked = root.join("locked");
    fs::create_dir_all(&locked).expect("dir");
    fs::write(locked.join("file.txt"), "match").expect("write");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod");

    let registry = setup_registry(root);
    let grep = registry.get("grep").expect("grep tool");
    let output = grep(ToolInvocation {
        name: "grep".to_string(),
        args: json!({"pattern": "match", "path": ".", "regex": false}),
        timeout_ms: None,
    })
    .await;

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).expect("chmod");
    assert!(!output.stderr.is_empty());
}
