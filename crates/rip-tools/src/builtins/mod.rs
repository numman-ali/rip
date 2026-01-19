use std::env;
use std::path::{Component, Path, PathBuf};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::task::spawn_blocking;

use crate::{ToolOutput, ToolRegistry};

mod apply_patch;
mod artifact_fetch;
mod grep;
mod ls;
mod read;
mod shell;
mod write;

#[derive(Clone, Debug)]
pub struct BuiltinToolConfig {
    pub workspace_root: PathBuf,
    pub artifact_max_bytes: usize,
    pub max_bytes: usize,
    pub max_results: usize,
    pub max_depth: usize,
    pub follow_symlinks: bool,
    pub include_hidden: bool,
}

impl Default for BuiltinToolConfig {
    fn default() -> Self {
        let workspace_root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            workspace_root,
            artifact_max_bytes: 16 * 1024 * 1024,
            max_bytes: 512 * 1024,
            max_results: 1000,
            max_depth: 64,
            follow_symlinks: false,
            include_hidden: false,
        }
    }
}

impl BuiltinToolConfig {
    pub fn artifacts_root(&self) -> PathBuf {
        self.workspace_root.join(".rip").join("artifacts")
    }
}

pub fn register_builtin_tools(registry: &ToolRegistry, config: BuiltinToolConfig) {
    let read_config = config.clone();
    registry.register(
        "read",
        std::sync::Arc::new(move |invocation| {
            let cfg = read_config.clone();
            Box::pin(async move {
                spawn_blocking(move || read::run_read(invocation, &cfg))
                    .await
                    .unwrap_or_else(|_| ToolOutput::failure(vec!["read panicked".to_string()]))
            })
        }),
    );

    let artifact_config = config.clone();
    registry.register(
        "artifact_fetch",
        std::sync::Arc::new(move |invocation| {
            let cfg = artifact_config.clone();
            Box::pin(async move {
                spawn_blocking(move || artifact_fetch::run_artifact_fetch(invocation, &cfg))
                    .await
                    .unwrap_or_else(|_| {
                        ToolOutput::failure(vec!["artifact_fetch panicked".to_string()])
                    })
            })
        }),
    );

    let write_config = config.clone();
    registry.register(
        "write",
        std::sync::Arc::new(move |invocation| {
            let cfg = write_config.clone();
            Box::pin(async move {
                spawn_blocking(move || write::run_write(invocation, &cfg))
                    .await
                    .unwrap_or_else(|_| ToolOutput::failure(vec!["write panicked".to_string()]))
            })
        }),
    );

    let patch_config = config.clone();
    registry.register(
        "apply_patch",
        std::sync::Arc::new(move |invocation| {
            let cfg = patch_config.clone();
            Box::pin(async move {
                spawn_blocking(move || apply_patch::run_apply_patch(invocation, &cfg))
                    .await
                    .unwrap_or_else(|_| {
                        ToolOutput::failure(vec!["apply_patch panicked".to_string()])
                    })
            })
        }),
    );

    let ls_config = config.clone();
    registry.register(
        "ls",
        std::sync::Arc::new(move |invocation| {
            let cfg = ls_config.clone();
            Box::pin(async move {
                spawn_blocking(move || ls::run_ls(invocation, &cfg))
                    .await
                    .unwrap_or_else(|_| ToolOutput::failure(vec!["ls panicked".to_string()]))
            })
        }),
    );

    let grep_config = config.clone();
    registry.register(
        "grep",
        std::sync::Arc::new(move |invocation| {
            let cfg = grep_config.clone();
            Box::pin(async move {
                spawn_blocking(move || grep::run_grep(invocation, &cfg))
                    .await
                    .unwrap_or_else(|_| ToolOutput::failure(vec!["grep panicked".to_string()]))
            })
        }),
    );

    let bash_config = config;
    registry.register(
        "bash",
        std::sync::Arc::new(move |invocation| {
            let cfg = bash_config.clone();
            Box::pin(shell::run_bash(invocation, cfg))
        }),
    );
    registry.register_alias("shell", "bash");
}

#[cfg_attr(test, inline(never))]
pub(super) fn parse_args<T: DeserializeOwned>(args: Value) -> Result<T, ToolOutput> {
    serde_json::from_value(args)
        .map_err(|err| ToolOutput::invalid_args(format!("invalid args: {err}")))
}

pub(super) fn resolve_path(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Err("absolute paths are not allowed".to_string());
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("path escapes workspace root".to_string());
    }
    Ok(root.join(path))
}

pub(super) fn normalize_rel_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

pub(super) fn truncate_utf8(bytes: &[u8], max_bytes: usize) -> (String, bool, usize) {
    if bytes.len() <= max_bytes {
        return (
            String::from_utf8_lossy(bytes).into_owned(),
            false,
            bytes.len(),
        );
    }

    let mut end = max_bytes;
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }
    (
        String::from_utf8_lossy(&bytes[..end]).into_owned(),
        true,
        end,
    )
}

#[cfg(windows)]
pub(super) fn default_shell_program() -> (String, Vec<String>) {
    if let Some(program) = find_program("pwsh") {
        return (program, vec!["-Command".to_string()]);
    }
    if let Some(program) = find_program("powershell") {
        return (program, vec!["-Command".to_string()]);
    }
    let program = env::var("COMSPEC").unwrap_or_else(|_| "cmd".to_string());
    (program, vec!["/C".to_string()])
}

#[cfg(not(windows))]
pub(super) fn default_shell_program() -> (String, Vec<String>) {
    let program = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    (program, vec!["-c".to_string()])
}

#[cfg(windows)]
pub(super) fn find_program(name: &str) -> Option<String> {
    if name.contains(std::path::MAIN_SEPARATOR) {
        return Some(name.to_string());
    }
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.exists() {
            return Some(candidate.to_string_lossy().to_string());
        }
        if let Some(exts) = env::var_os("PATHEXT") {
            let exts = exts.to_string_lossy();
            for ext in exts.split(';') {
                if ext.is_empty() {
                    continue;
                }
                let candidate = dir.join(format!("{}{}", name, ext));
                if candidate.exists() {
                    return Some(candidate.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

pub(super) fn build_globset(patterns: Option<&[String]>) -> Result<Option<GlobSet>, String> {
    let patterns = match patterns {
        Some(patterns) if !patterns.is_empty() => patterns,
        _ => return Ok(None),
    };

    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = GlobBuilder::new(pattern)
            .literal_separator(true)
            .case_insensitive(cfg!(windows))
            .build()
            .map_err(|err| format!("invalid glob '{pattern}': {err}"))?;
        builder.add(glob);
    }
    builder
        .build()
        .map(Some)
        .map_err(|err| format!("invalid glob set: {err}"))
}

pub(super) fn globsets_match(
    include: &Option<GlobSet>,
    exclude: &Option<GlobSet>,
    path: &str,
) -> bool {
    if let Some(set) = include {
        if !set.is_match(path) {
            return false;
        }
    }
    if let Some(set) = exclude {
        if set.is_match(path) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolInvocation;
    use serde_json::json;
    use std::ffi::OsString;
    use std::fs;
    use std::path::Path;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

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
            let previous = env::var_os(key);
            env::set_var(key, value.into());
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => env::set_var(self.key, value),
                None => env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn default_config_has_limits() {
        let config = BuiltinToolConfig::default();
        assert!(config.max_bytes > 0);
        assert!(config.max_results > 0);
        assert!(config.max_depth > 0);
    }

    #[test]
    fn truncate_utf8_no_truncation() {
        let bytes = b"hello";
        let (text, truncated, used) = truncate_utf8(bytes, 10);
        assert_eq!(text, "hello");
        assert!(!truncated);
        assert_eq!(used, bytes.len());
    }

    #[test]
    fn truncate_utf8_handles_multibyte() {
        let bytes = "é".as_bytes();
        let (text, truncated, used) = truncate_utf8(bytes, 1);
        assert_eq!(text, "");
        assert!(truncated);
        assert_eq!(used, 0);
    }

    #[test]
    fn globsets_match_exclude_only() {
        let patterns = vec!["**/*.log".to_string()];
        let exclude = build_globset(Some(&patterns)).expect("globset");
        assert!(!globsets_match(&None, &exclude, "a.log"));
        assert!(globsets_match(&None, &exclude, "a.txt"));
    }

    #[test]
    fn write_fails_when_parent_is_file() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        fs::write(root.join("blocked"), "nope").expect("write");

        let config = BuiltinToolConfig {
            workspace_root: root.to_path_buf(),
            artifact_max_bytes: 1024 * 1024,
            max_bytes: 1024,
            max_results: 10,
            max_depth: 4,
            follow_symlinks: false,
            include_hidden: false,
        };

        let output = write::run_write(
            ToolInvocation {
                name: "write".to_string(),
                args: json!({
                    "path": "blocked/child.txt",
                    "content": "hi"
                }),
                timeout_ms: None,
            },
            &config,
        );

        assert_eq!(output.exit_code, 1);
        assert!(output.stderr.join("\n").contains("write failed"));
    }

    #[test]
    fn write_fails_when_target_is_directory() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        fs::create_dir_all(root.join("target")).expect("dir");

        let config = BuiltinToolConfig {
            workspace_root: root.to_path_buf(),
            artifact_max_bytes: 1024 * 1024,
            max_bytes: 1024,
            max_results: 10,
            max_depth: 4,
            follow_symlinks: false,
            include_hidden: false,
        };

        let output = write::run_write(
            ToolInvocation {
                name: "write".to_string(),
                args: json!({
                    "path": "target",
                    "content": "hi",
                    "atomic": true
                }),
                timeout_ms: None,
            },
            &config,
        );

        assert_eq!(output.exit_code, 1);
        assert!(output.stderr.join("\n").contains("write failed"));
    }

    #[test]
    fn bash_falls_back_to_shell_when_missing() {
        let _lock = env_lock().lock().expect("lock");
        if !Path::new("/bin/sh").exists() {
            return;
        }
        let _path_guard = EnvGuard::set("PATH", "");
        let _shell_guard = EnvGuard::set("SHELL", "/bin/sh");

        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let config = BuiltinToolConfig {
            workspace_root: root.to_path_buf(),
            artifact_max_bytes: 1024 * 1024,
            max_bytes: 1024,
            max_results: 10,
            max_depth: 4,
            follow_symlinks: false,
            include_hidden: false,
        };

        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let output = rt.block_on(shell::run_bash(
            ToolInvocation {
                name: "bash".to_string(),
                args: json!({
                    "command": "echo ok",
                    "cwd": ".",
                    "env": { "RIP_TEST": "1" }
                }),
                timeout_ms: None,
            },
            config,
        ));

        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.join("\n").contains("ok"));
    }

    #[test]
    fn bash_writes_artifact_when_output_truncated() {
        if cfg!(windows) {
            return;
        }
        if !Path::new("/bin/sh").exists() {
            return;
        }

        let dir = tempdir().expect("tmp");
        let root = dir.path();
        let config = BuiltinToolConfig {
            workspace_root: root.to_path_buf(),
            artifact_max_bytes: 8 * 1024,
            max_bytes: 64,
            max_results: 10,
            max_depth: 4,
            follow_symlinks: false,
            include_hidden: false,
        };

        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let output = rt.block_on(shell::run_bash(
            ToolInvocation {
                name: "bash".to_string(),
                args: json!({
                    "command": "i=0; while [ $i -lt 2048 ]; do printf x; i=$((i+1)); done",
                    "cwd": ".",
                }),
                timeout_ms: None,
            },
            config.clone(),
        ));

        assert_eq!(output.exit_code, 0);
        let artifacts = output.artifacts.expect("artifacts");
        let stdout_meta = artifacts.get("stdout").expect("stdout meta");
        assert_eq!(
            stdout_meta.get("truncated").and_then(|v| v.as_bool()),
            Some(true)
        );

        let artifact = stdout_meta
            .get("artifact")
            .and_then(|v| v.as_object())
            .expect("stdout artifact");
        let id = artifact.get("id").and_then(|v| v.as_str()).expect("id");
        assert_eq!(id.len(), 64);
        let path = artifact.get("path").and_then(|v| v.as_str()).expect("path");
        assert!(root.join(path).exists(), "artifact path {path} missing");

        let fetched = artifact_fetch::run_artifact_fetch(
            ToolInvocation {
                name: "artifact_fetch".to_string(),
                args: json!({ "id": id, "max_bytes": 128 }),
                timeout_ms: None,
            },
            &config,
        );
        assert_eq!(fetched.exit_code, 0);
        assert!(
            fetched.stdout.join("\n").contains('x'),
            "stdout: {:?}",
            fetched.stdout
        );
        let fetched_artifacts = fetched.artifacts.expect("fetch artifacts");
        assert_eq!(
            fetched_artifacts.get("id").and_then(|v| v.as_str()),
            Some(id)
        );
    }

    #[test]
    fn run_ls_lists_entries_direct() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        fs::write(root.join("a.txt"), "hi").expect("write");

        let config = BuiltinToolConfig {
            workspace_root: root.to_path_buf(),
            artifact_max_bytes: 1024 * 1024,
            max_bytes: 1024,
            max_results: 10,
            max_depth: 4,
            follow_symlinks: false,
            include_hidden: false,
        };

        let output = ls::run_ls(
            ToolInvocation {
                name: "ls".to_string(),
                args: json!({ "path": "." }),
                timeout_ms: None,
            },
            &config,
        );

        assert_eq!(output.exit_code, 0);
        assert!(output.stdout.iter().any(|line| line.ends_with("a.txt")));
    }

    #[test]
    fn run_grep_finds_matches_direct() {
        let dir = tempdir().expect("tmp");
        let root = dir.path();
        fs::write(root.join("notes.txt"), "hello\n").expect("write");

        let config = BuiltinToolConfig {
            workspace_root: root.to_path_buf(),
            artifact_max_bytes: 1024 * 1024,
            max_bytes: 1024,
            max_results: 10,
            max_depth: 4,
            follow_symlinks: false,
            include_hidden: false,
        };

        let output = grep::run_grep(
            ToolInvocation {
                name: "grep".to_string(),
                args: json!({ "pattern": "hello", "path": "." }),
                timeout_ms: None,
            },
            &config,
        );

        assert_eq!(output.exit_code, 0);
        assert!(
            output
                .stdout
                .iter()
                .any(|line| line.contains("notes.txt:1:hello")),
            "stdout: {:?}",
            output.stdout
        );
    }
}
