use std::collections::HashMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use regex::RegexBuilder;
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::task::spawn_blocking;

use crate::{ToolInvocation, ToolOutput, ToolRegistry};

#[derive(Clone, Debug)]
pub struct BuiltinToolConfig {
    pub workspace_root: PathBuf,
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
            max_bytes: 512 * 1024,
            max_results: 1000,
            max_depth: 64,
            follow_symlinks: false,
            include_hidden: false,
        }
    }
}

pub fn register_builtin_tools(registry: &ToolRegistry, config: BuiltinToolConfig) {
    let read_config = config.clone();
    registry.register(
        "read",
        std::sync::Arc::new(move |invocation| {
            let cfg = read_config.clone();
            Box::pin(async move {
                spawn_blocking(move || run_read(invocation, &cfg))
                    .await
                    .unwrap_or_else(|_| ToolOutput::failure(vec!["read panicked".to_string()]))
            })
        }),
    );

    let write_config = config.clone();
    registry.register(
        "write",
        std::sync::Arc::new(move |invocation| {
            let cfg = write_config.clone();
            Box::pin(async move {
                spawn_blocking(move || run_write(invocation, &cfg))
                    .await
                    .unwrap_or_else(|_| ToolOutput::failure(vec!["write panicked".to_string()]))
            })
        }),
    );

    let ls_config = config.clone();
    registry.register(
        "ls",
        std::sync::Arc::new(move |invocation| {
            let cfg = ls_config.clone();
            Box::pin(async move {
                spawn_blocking(move || run_ls(invocation, &cfg))
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
                spawn_blocking(move || run_grep(invocation, &cfg))
                    .await
                    .unwrap_or_else(|_| ToolOutput::failure(vec!["grep panicked".to_string()]))
            })
        }),
    );

    let shell_config = config.clone();
    registry.register(
        "shell",
        std::sync::Arc::new(move |invocation| {
            let cfg = shell_config.clone();
            Box::pin(run_shell(invocation, cfg))
        }),
    );

    let bash_config = config;
    registry.register(
        "bash",
        std::sync::Arc::new(move |invocation| {
            let cfg = bash_config.clone();
            Box::pin(run_bash(invocation, cfg))
        }),
    );
}

#[derive(Deserialize)]
struct ReadArgs {
    path: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
    max_bytes: Option<usize>,
}

fn run_read(invocation: ToolInvocation, config: &BuiltinToolConfig) -> ToolOutput {
    let args: ReadArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    if let Some(start) = args.start_line {
        if start == 0 {
            return ToolOutput::invalid_args("line numbers are 1-based".to_string());
        }
    }
    if let Some(end) = args.end_line {
        if end == 0 {
            return ToolOutput::invalid_args("line numbers are 1-based".to_string());
        }
    }
    if let (Some(start), Some(end)) = (args.start_line, args.end_line) {
        if start > end {
            return ToolOutput::invalid_args("start_line must be <= end_line".to_string());
        }
    }

    let path = match resolve_path(&config.workspace_root, &args.path) {
        Ok(path) => path,
        Err(err) => return ToolOutput::failure(vec![err]),
    };

    let file = match File::open(&path) {
        Ok(file) => file,
        Err(err) => return ToolOutput::failure(vec![format!("read failed: {err}")]),
    };

    let max_bytes = args.max_bytes.unwrap_or(config.max_bytes);
    let mut reader = BufReader::new(file);
    let mut buffer = String::new();
    let mut output = Vec::new();
    let mut line_no = 0usize;
    let mut truncated = false;

    loop {
        buffer.clear();
        let _read = match reader.read_line(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(err) => return ToolOutput::failure(vec![format!("read failed: {err}")]),
        };
        line_no += 1;

        if let Some(start) = args.start_line {
            if line_no < start {
                continue;
            }
        }
        if let Some(end) = args.end_line {
            if line_no > end {
                break;
            }
        }

        output.extend_from_slice(buffer.as_bytes());
        if output.len() >= max_bytes {
            output.truncate(max_bytes);
            truncated = true;
            break;
        }
    }

    let (content, _, used_bytes) = truncate_utf8(&output, max_bytes);

    ToolOutput {
        stdout: vec![content],
        stderr: Vec::new(),
        exit_code: 0,
        artifacts: Some(json!({
            "path": normalize_rel_path(&config.workspace_root, &path),
            "bytes": used_bytes,
            "truncated": truncated,
            "start_line": args.start_line,
            "end_line": args.end_line
        })),
    }
}

#[derive(Deserialize)]
struct WriteArgs {
    path: String,
    content: String,
    append: Option<bool>,
    create: Option<bool>,
    atomic: Option<bool>,
}

fn run_write(invocation: ToolInvocation, config: &BuiltinToolConfig) -> ToolOutput {
    let args: WriteArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    let path = match resolve_path(&config.workspace_root, &args.path) {
        Ok(path) => path,
        Err(err) => return ToolOutput::failure(vec![err]),
    };

    let create = args.create.unwrap_or(true);
    let append = args.append.unwrap_or(false);
    let atomic = args.atomic.unwrap_or(true);

    if let Some(parent) = path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            return ToolOutput::failure(vec![format!("write failed: {err}")]);
        }
    }

    let bytes_written = if append {
        let mut file = match OpenOptions::new().create(create).append(true).open(&path) {
            Ok(file) => file,
            Err(err) => return ToolOutput::failure(vec![format!("write failed: {err}")]),
        };
        if let Err(err) = file.write_all(args.content.as_bytes()) {
            return ToolOutput::failure(vec![format!("write failed: {err}")]);
        }
        args.content.len()
    } else if atomic {
        let tmp_path = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
        if let Err(err) = fs::write(&tmp_path, args.content.as_bytes()) {
            return ToolOutput::failure(vec![format!("write failed: {err}")]);
        }
        if path.exists() {
            if let Err(err) = fs::remove_file(&path) {
                return ToolOutput::failure(vec![format!("write failed: {err}")]);
            }
        }
        if let Err(err) = fs::rename(&tmp_path, &path) {
            return ToolOutput::failure(vec![format!("write failed: {err}")]);
        }
        args.content.len()
    } else {
        if let Err(err) = fs::write(&path, args.content.as_bytes()) {
            return ToolOutput::failure(vec![format!("write failed: {err}")]);
        }
        args.content.len()
    };

    ToolOutput {
        stdout: vec![format!("wrote {bytes_written} bytes")],
        stderr: Vec::new(),
        exit_code: 0,
        artifacts: Some(json!({
            "path": normalize_rel_path(&config.workspace_root, &path),
            "bytes_written": bytes_written
        })),
    }
}

#[derive(Deserialize)]
struct LsArgs {
    path: Option<String>,
    recursive: Option<bool>,
    max_depth: Option<usize>,
    include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    include_hidden: Option<bool>,
    follow_symlinks: Option<bool>,
}

fn run_ls(invocation: ToolInvocation, config: &BuiltinToolConfig) -> ToolOutput {
    let args: LsArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    let root = args.path.unwrap_or_else(|| ".".to_string());
    let root_path = match resolve_path(&config.workspace_root, &root) {
        Ok(path) => path,
        Err(err) => return ToolOutput::failure(vec![err]),
    };

    let include_hidden = args.include_hidden.unwrap_or(config.include_hidden);
    let follow_symlinks = args.follow_symlinks.unwrap_or(config.follow_symlinks);
    let recursive = args.recursive.unwrap_or(false);
    let max_depth = args.max_depth.unwrap_or(config.max_depth);

    let include_set = match build_globset(args.include.as_deref()) {
        Ok(set) => set,
        Err(err) => return ToolOutput::invalid_args(err),
    };
    let exclude_set = match build_globset(args.exclude.as_deref()) {
        Ok(set) => set,
        Err(err) => return ToolOutput::invalid_args(err),
    };

    let mut builder = WalkBuilder::new(&root_path);
    builder
        .hidden(!include_hidden)
        .follow_links(follow_symlinks);
    if recursive {
        builder.max_depth(Some(max_depth));
    } else {
        builder.max_depth(Some(1));
    }

    let mut stdout = Vec::new();
    let mut errors = Vec::new();

    for entry in builder.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                errors.push(err.to_string());
                continue;
            }
        };
        if entry.depth() == 0 {
            continue;
        }
        let path = entry.path();
        let rel = normalize_rel_path(&config.workspace_root, path);
        if !globsets_match(&include_set, &exclude_set, &rel) {
            continue;
        }
        stdout.push(rel);
    }

    ToolOutput {
        stdout,
        stderr: errors,
        exit_code: 0,
        artifacts: Some(json!({
            "root": normalize_rel_path(&config.workspace_root, &root_path)
        })),
    }
}

#[derive(Deserialize)]
struct GrepArgs {
    pattern: String,
    path: Option<String>,
    regex: Option<bool>,
    case_sensitive: Option<bool>,
    include: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    max_results: Option<usize>,
    max_bytes: Option<usize>,
    max_depth: Option<usize>,
    include_hidden: Option<bool>,
    follow_symlinks: Option<bool>,
}

fn run_grep(invocation: ToolInvocation, config: &BuiltinToolConfig) -> ToolOutput {
    let args: GrepArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    let root = args.path.unwrap_or_else(|| ".".to_string());
    let root_path = match resolve_path(&config.workspace_root, &root) {
        Ok(path) => path,
        Err(err) => return ToolOutput::failure(vec![err]),
    };

    let regex_enabled = args.regex.unwrap_or(true);
    let case_sensitive = args.case_sensitive.unwrap_or(true);
    let max_results = args.max_results.unwrap_or(config.max_results);
    let max_bytes = args.max_bytes.unwrap_or(config.max_bytes);
    let max_depth = args.max_depth.unwrap_or(config.max_depth);
    let include_hidden = args.include_hidden.unwrap_or(config.include_hidden);
    let follow_symlinks = args.follow_symlinks.unwrap_or(config.follow_symlinks);

    let include_set = match build_globset(args.include.as_deref()) {
        Ok(set) => set,
        Err(err) => return ToolOutput::invalid_args(err),
    };
    let exclude_set = match build_globset(args.exclude.as_deref()) {
        Ok(set) => set,
        Err(err) => return ToolOutput::invalid_args(err),
    };

    let pattern = if regex_enabled {
        args.pattern
    } else {
        regex::escape(&args.pattern)
    };

    let regex = match RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .build()
    {
        Ok(regex) => regex,
        Err(err) => return ToolOutput::invalid_args(format!("invalid regex: {err}")),
    };

    let mut builder = WalkBuilder::new(&root_path);
    builder
        .hidden(!include_hidden)
        .follow_links(follow_symlinks);
    builder.max_depth(Some(max_depth));

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut matches = 0usize;

    'walk: for entry in builder.build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                stderr.push(err.to_string());
                continue;
            }
        };

        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            let path = entry.path();
            let rel = normalize_rel_path(&config.workspace_root, path);
            if !globsets_match(&include_set, &exclude_set, &rel) {
                continue;
            }

            let file = match File::open(path) {
                Ok(file) => file,
                Err(err) => {
                    stderr.push(format!("{rel}: {err}"));
                    continue;
                }
            };
            let mut reader = BufReader::new(file);
            let mut buffer = String::new();
            let mut bytes_read = 0usize;
            let mut line_no = 0usize;

            loop {
                buffer.clear();
                let read = match reader.read_line(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(err) => {
                        stderr.push(format!("{rel}: {err}"));
                        break;
                    }
                };
                line_no += 1;
                bytes_read += read;
                if bytes_read > max_bytes {
                    break;
                }

                if buffer.contains('\0') {
                    break;
                }

                let line = buffer.trim_end_matches(['\r', '\n']);
                if regex.is_match(line) {
                    stdout.push(format!("{rel}:{line_no}:{line}"));
                    matches += 1;
                    if matches >= max_results {
                        break 'walk;
                    }
                }
            }
        }
    }

    ToolOutput {
        stdout,
        stderr,
        exit_code: 0,
        artifacts: Some(json!({
            "root": normalize_rel_path(&config.workspace_root, &root_path),
            "matches": matches
        })),
    }
}

#[derive(Deserialize)]
struct ShellArgs {
    command: String,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    max_bytes: Option<usize>,
}

async fn run_shell(invocation: ToolInvocation, config: BuiltinToolConfig) -> ToolOutput {
    let args: ShellArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    let max_bytes = args.max_bytes.unwrap_or(config.max_bytes);
    let (program, mut program_args) = default_shell_program();

    program_args.push(args.command);

    let mut cmd = Command::new(program);
    cmd.args(program_args);
    if let Some(cwd) = args.cwd {
        match resolve_path(&config.workspace_root, &cwd) {
            Ok(path) => {
                cmd.current_dir(path);
            }
            Err(err) => return ToolOutput::failure(vec![err]),
        }
    }
    if let Some(envs) = args.env {
        cmd.envs(envs);
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = split_output(output.stdout, max_bytes);
            let stderr = split_output(output.stderr, max_bytes);
            ToolOutput {
                stdout,
                stderr,
                exit_code: output.status.code().unwrap_or(1),
                artifacts: None,
            }
        }
        Err(err) => ToolOutput::failure(vec![format!("shell failed: {err}")]),
    }
}

async fn run_bash(invocation: ToolInvocation, config: BuiltinToolConfig) -> ToolOutput {
    let args: ShellArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    let max_bytes = args.max_bytes.unwrap_or(config.max_bytes);
    let mut cmd = Command::new("bash");
    cmd.arg("-c").arg(args.command);
    if let Some(cwd) = args.cwd {
        match resolve_path(&config.workspace_root, &cwd) {
            Ok(path) => {
                cmd.current_dir(path);
            }
            Err(err) => return ToolOutput::failure(vec![err]),
        }
    }
    if let Some(envs) = args.env {
        cmd.envs(envs);
    }

    match cmd.output() {
        Ok(output) => {
            let stdout = split_output(output.stdout, max_bytes);
            let stderr = split_output(output.stderr, max_bytes);
            ToolOutput {
                stdout,
                stderr,
                exit_code: output.status.code().unwrap_or(1),
                artifacts: None,
            }
        }
        Err(err) => ToolOutput::failure(vec![format!("bash failed: {err}")]),
    }
}

fn parse_args<T: DeserializeOwned>(args: Value) -> Result<T, ToolOutput> {
    serde_json::from_value(args)
        .map_err(|err| ToolOutput::invalid_args(format!("invalid args: {err}")))
}

fn resolve_path(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Err("absolute paths are not allowed".to_string());
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("path escapes workspace root".to_string());
    }
    Ok(root.join(path))
}

fn normalize_rel_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

fn truncate_utf8(bytes: &[u8], max_bytes: usize) -> (String, bool, usize) {
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

fn split_output(bytes: Vec<u8>, max_bytes: usize) -> Vec<String> {
    let (text, _truncated, _) = truncate_utf8(&bytes, max_bytes);
    text.lines()
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect()
}

#[cfg(windows)]
fn default_shell_program() -> (String, Vec<String>) {
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
fn default_shell_program() -> (String, Vec<String>) {
    let program = env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
    (program, vec!["-c".to_string()])
}

#[cfg(windows)]
fn find_program(name: &str) -> Option<String> {
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

fn build_globset(patterns: Option<&[String]>) -> Result<Option<GlobSet>, String> {
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

fn globsets_match(include: &Option<GlobSet>, exclude: &Option<GlobSet>, path: &str) -> bool {
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
    fn split_output_trims_cr() {
        let output = split_output(b"one\r\ntwo\r\n".to_vec(), 1024);
        assert_eq!(output, vec!["one".to_string(), "two".to_string()]);
    }

    #[test]
    fn globsets_match_exclude_only() {
        let patterns = vec!["**/*.log".to_string()];
        let exclude = build_globset(Some(&patterns)).expect("globset");
        assert!(!globsets_match(&None, &exclude, "a.log"));
        assert!(globsets_match(&None, &exclude, "a.txt"));
    }
}
