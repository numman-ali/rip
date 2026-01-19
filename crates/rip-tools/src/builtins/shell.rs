use std::collections::HashMap;
use std::io::ErrorKind;
use std::path::Path;

use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use uuid::Uuid;

use crate::{ToolInvocation, ToolOutput};

use super::{default_shell_program, parse_args, resolve_path, truncate_utf8, BuiltinToolConfig};

#[derive(Deserialize)]
struct ShellArgs {
    command: String,
    cwd: Option<String>,
    env: Option<HashMap<String, String>>,
    max_bytes: Option<usize>,
}

pub(super) async fn run_bash(invocation: ToolInvocation, config: BuiltinToolConfig) -> ToolOutput {
    let args: ShellArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    let max_bytes = args.max_bytes.unwrap_or(config.max_bytes);
    let cmd_result = run_command("bash", &["-c", &args.command], &args, &config, max_bytes).await;
    match cmd_result {
        Ok(output) => output,
        Err(err) => {
            if err.kind() == ErrorKind::NotFound {
                return run_shell_with_args(&args, &config, max_bytes).await;
            }
            ToolOutput::failure(vec![format!("bash failed: {err}")])
        }
    }
}

async fn run_shell_with_args(
    args: &ShellArgs,
    config: &BuiltinToolConfig,
    max_bytes: usize,
) -> ToolOutput {
    let (program, mut program_args) = default_shell_program();
    program_args.push(args.command.clone());

    let args_refs: Vec<&str> = program_args.iter().map(String::as_str).collect();
    match run_command(&program, &args_refs, args, config, max_bytes).await {
        Ok(output) => output,
        Err(err) => ToolOutput::failure(vec![format!("shell failed: {err}")]),
    }
}

async fn run_command(
    program: &str,
    program_args: &[&str],
    args: &ShellArgs,
    config: &BuiltinToolConfig,
    max_bytes: usize,
) -> Result<ToolOutput, std::io::Error> {
    let mut cmd = Command::new(program);
    cmd.args(program_args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    if let Some(cwd) = args.cwd.as_deref() {
        match resolve_path(&config.workspace_root, cwd) {
            Ok(path) => {
                cmd.current_dir(path);
            }
            Err(err) => return Ok(ToolOutput::failure(vec![err])),
        }
    }
    if let Some(envs) = &args.env {
        cmd.envs(envs);
    }

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdout_fut = capture_stream(stdout, config, max_bytes);
    let stderr_fut = capture_stream(stderr, config, max_bytes);
    let status_fut = child.wait();
    let (stdout_capture, stderr_capture, status) = tokio::join!(stdout_fut, stderr_fut, status_fut);
    let status = status?;

    let artifacts = json!({
        "stdout": stdout_capture.as_json(),
        "stderr": stderr_capture.as_json(),
    });

    Ok(ToolOutput {
        stdout: stdout_capture.preview_lines,
        stderr: stderr_capture.preview_lines,
        exit_code: status.code().unwrap_or(1),
        artifacts: Some(artifacts),
    })
}

struct StreamArtifactRef {
    id: String,
    path: String,
    bytes: u64,
    truncated: bool,
}

struct StreamCapture {
    preview_lines: Vec<String>,
    bytes_preview: usize,
    bytes_total: u64,
    truncated_preview: bool,
    artifact: Option<StreamArtifactRef>,
    error: Option<String>,
}

impl StreamCapture {
    fn failed(error: String) -> Self {
        Self {
            preview_lines: Vec::new(),
            bytes_preview: 0,
            bytes_total: 0,
            truncated_preview: false,
            artifact: None,
            error: Some(error),
        }
    }

    fn as_json(&self) -> serde_json::Value {
        let artifact = self.artifact.as_ref().map(|artifact| {
            json!({
                "id": artifact.id,
                "path": artifact.path,
                "bytes": artifact.bytes,
                "truncated": artifact.truncated,
            })
        });

        json!({
            "bytes_total": self.bytes_total,
            "bytes_preview": self.bytes_preview,
            "truncated": self.truncated_preview,
            "artifact": artifact,
            "error": self.error,
        })
    }
}

async fn capture_stream<R: AsyncRead + Unpin>(
    stream: Option<R>,
    config: &BuiltinToolConfig,
    max_preview_bytes: usize,
) -> StreamCapture {
    let Some(mut stream) = stream else {
        return StreamCapture {
            preview_lines: Vec::new(),
            bytes_preview: 0,
            bytes_total: 0,
            truncated_preview: false,
            artifact: None,
            error: None,
        };
    };

    let mut preview = Vec::new();
    let mut bytes_total: u64 = 0;
    let mut preview_full = false;

    let mut tmp_file: Option<tokio::fs::File> = None;
    let mut tmp_path: Option<std::path::PathBuf> = None;
    let mut hasher: Option<Sha256> = None;
    let mut stored_bytes: u64 = 0;

    let mut buf = vec![0u8; 8192];

    loop {
        let n = match stream.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(err) => return StreamCapture::failed(format!("stream read failed: {err}")),
        };

        bytes_total = bytes_total.saturating_add(n as u64);
        let chunk = &buf[..n];

        let preview_before = preview.len();
        if !preview_full {
            let remaining = max_preview_bytes.saturating_sub(preview.len());
            let take = remaining.min(chunk.len());
            preview.extend_from_slice(&chunk[..take]);
            if preview.len() >= max_preview_bytes {
                preview.truncate(max_preview_bytes);
                preview_full = true;
            }
        }

        if tmp_file.is_none() {
            if !preview_full {
                continue;
            }

            if config.artifact_max_bytes == 0 {
                continue;
            }

            let root = config.artifacts_root();
            let blobs_dir = root.join("blobs");
            let tmp_dir = root.join("tmp");
            if let Err(err) = tokio::fs::create_dir_all(&blobs_dir).await {
                return StreamCapture::failed(format!("artifact dir create failed: {err}"));
            }
            if let Err(err) = tokio::fs::create_dir_all(&tmp_dir).await {
                return StreamCapture::failed(format!("artifact dir create failed: {err}"));
            }

            let path = tmp_dir.join(format!("{}.tmp", Uuid::new_v4()));
            let mut file = match tokio::fs::File::create(&path).await {
                Ok(file) => file,
                Err(err) => return StreamCapture::failed(format!("artifact create failed: {err}")),
            };

            let mut sha = Sha256::new();
            let initial = if config.artifact_max_bytes == 0 {
                0
            } else {
                preview.len().min(config.artifact_max_bytes)
            };
            if initial > 0 {
                if let Err(err) = file.write_all(&preview[..initial]).await {
                    return StreamCapture::failed(format!("artifact write failed: {err}"));
                }
                sha.update(&preview[..initial]);
                stored_bytes = initial as u64;
            }

            tmp_file = Some(file);
            tmp_path = Some(path);
            hasher = Some(sha);

            let already_in_preview =
                (preview.len().saturating_sub(preview_before)).min(chunk.len());
            let remainder = &chunk[already_in_preview..];
            if !remainder.is_empty() {
                let _ = write_artifact_tail(
                    tmp_file.as_mut().expect("file"),
                    hasher.as_mut().expect("hasher"),
                    &mut stored_bytes,
                    config.artifact_max_bytes,
                    remainder,
                )
                .await;
            }
            continue;
        }

        let _ = write_artifact_tail(
            tmp_file.as_mut().expect("file"),
            hasher.as_mut().expect("hasher"),
            &mut stored_bytes,
            config.artifact_max_bytes,
            chunk,
        )
        .await;
    }

    let truncated_preview = bytes_total as usize > max_preview_bytes;

    let (preview_text, _utf8_truncated, used_bytes) = truncate_utf8(&preview, max_preview_bytes);
    let preview_lines = preview_text
        .lines()
        .map(|line| line.trim_end_matches('\r').to_string())
        .collect::<Vec<_>>();

    let artifact = if !truncated_preview {
        if let Some(path) = tmp_path {
            let _ = tokio::fs::remove_file(path).await;
        }
        None
    } else {
        finalize_artifact(
            config,
            tmp_file,
            tmp_path,
            hasher,
            stored_bytes,
            bytes_total,
        )
        .await
    };

    StreamCapture {
        preview_lines,
        bytes_preview: used_bytes,
        bytes_total,
        truncated_preview,
        artifact,
        error: None,
    }
}

async fn write_artifact_tail(
    file: &mut tokio::fs::File,
    hasher: &mut Sha256,
    stored_bytes: &mut u64,
    max_bytes: usize,
    chunk: &[u8],
) -> Result<(), ()> {
    if max_bytes == 0 {
        return Ok(());
    }
    let remaining = max_bytes.saturating_sub(*stored_bytes as usize);
    if remaining == 0 {
        return Ok(());
    }
    let take = remaining.min(chunk.len());
    if take == 0 {
        return Ok(());
    }
    if file.write_all(&chunk[..take]).await.is_err() {
        return Err(());
    }
    hasher.update(&chunk[..take]);
    *stored_bytes = stored_bytes.saturating_add(take as u64);
    Ok(())
}

async fn finalize_artifact(
    config: &BuiltinToolConfig,
    file: Option<tokio::fs::File>,
    tmp_path: Option<std::path::PathBuf>,
    hasher: Option<Sha256>,
    stored_bytes: u64,
    bytes_total: u64,
) -> Option<StreamArtifactRef> {
    let (Some(_file), Some(tmp_path), Some(hasher)) = (file, tmp_path, hasher) else {
        return None;
    };

    let digest = hasher.finalize();
    let id = hex::encode(digest);
    let root = config.artifacts_root();
    let blobs_dir = root.join("blobs");
    let final_path = blobs_dir.join(&id);
    let truncated = bytes_total > stored_bytes;

    let rename_result = match tokio::fs::metadata(&final_path).await {
        Ok(_) => {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            Ok(())
        }
        Err(_) => tokio::fs::rename(&tmp_path, &final_path).await,
    };
    if rename_result.is_err() {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        return None;
    }

    let rel = path_rel(&config.workspace_root, &final_path);
    Some(StreamArtifactRef {
        id,
        path: rel,
        bytes: stored_bytes,
        truncated,
    })
}

fn path_rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
