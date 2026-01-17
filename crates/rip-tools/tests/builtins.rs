use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use rip_tools::{register_builtin_tools, BuiltinToolConfig, ToolInvocation, ToolRegistry};
use serde_json::json;
use tempfile::tempdir;
use tokio::sync::Mutex;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn setup_registry(root: &Path) -> ToolRegistry {
    let registry = ToolRegistry::default();
    let config = BuiltinToolConfig {
        workspace_root: root.to_path_buf(),
        max_bytes: 1024 * 1024,
        max_results: 100,
        max_depth: 16,
        follow_symlinks: false,
        include_hidden: false,
    };
    register_builtin_tools(&registry, config);
    registry
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct EnvGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: impl Into<OsString>) -> Self {
        let previous = std::env::var_os(key);
        std::env::set_var(key, value.into());
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

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

#[tokio::test]
async fn bash_runs_command_if_available() {
    let _lock = env_lock().lock().await;
    if std::process::Command::new("bash")
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
