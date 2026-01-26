use std::path::{Path, PathBuf};

use rip_kernel::ToolTaskExecutionMode;
use serde_json::{json, Value};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use super::{TaskEngineConfig, TaskOutputStream};

#[derive(Debug, Clone)]
pub(super) struct TaskLog {
    pub(super) artifact_id: String,
    pub(super) path: String,
}

impl TaskLog {
    pub(super) fn new(workspace_root: &Path) -> Self {
        let artifact_id = new_artifact_id();
        let path = workspace_root
            .join(".rip")
            .join("artifacts")
            .join("blobs")
            .join(&artifact_id);
        Self {
            artifact_id,
            path: normalize_rel_path(workspace_root, &path),
        }
    }

    pub(super) fn as_ref_json(&self) -> Value {
        json!({
            "id": self.artifact_id,
            "path": self.path,
        })
    }
}

#[derive(Debug)]
pub(super) struct TaskLogs {
    pub(super) stdout: Option<TaskLog>,
    pub(super) stderr: Option<TaskLog>,
    pub(super) pty: Option<TaskLog>,
}

impl TaskLogs {
    pub(super) fn new(workspace_root: &Path, mode: ToolTaskExecutionMode) -> Self {
        match mode {
            ToolTaskExecutionMode::Pipes => Self {
                stdout: Some(TaskLog::new(workspace_root)),
                stderr: Some(TaskLog::new(workspace_root)),
                pty: None,
            },
            ToolTaskExecutionMode::Pty => Self {
                stdout: None,
                stderr: None,
                pty: Some(TaskLog::new(workspace_root)),
            },
        }
    }

    pub(super) fn refs_json(&self) -> Value {
        let mut logs = serde_json::Map::new();
        if let Some(log) = &self.stdout {
            logs.insert("stdout".to_string(), log.as_ref_json());
        }
        if let Some(log) = &self.stderr {
            logs.insert("stderr".to_string(), log.as_ref_json());
        }
        if let Some(log) = &self.pty {
            logs.insert("pty".to_string(), log.as_ref_json());
        }
        Value::Object(logs)
    }

    pub(super) fn log_for_output(&self, stream: TaskOutputStream) -> Option<&TaskLog> {
        match stream {
            TaskOutputStream::Stdout => self.stdout.as_ref(),
            TaskOutputStream::Stderr => self.stderr.as_ref(),
            TaskOutputStream::Pty => self.pty.as_ref(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TaskLogSummary {
    artifact_id: String,
    path: String,
    bytes_total: u64,
    bytes_stored: u64,
    truncated: bool,
    error: Option<String>,
}

impl TaskLogSummary {
    pub(super) fn failed(artifact_id: String, path: String, error: String) -> Self {
        Self {
            artifact_id,
            path,
            bytes_total: 0,
            bytes_stored: 0,
            truncated: false,
            error: Some(error),
        }
    }

    pub(super) fn as_json(&self) -> Value {
        json!({
            "id": self.artifact_id,
            "path": self.path,
            "bytes_total": self.bytes_total,
            "bytes_stored": self.bytes_stored,
            "truncated": self.truncated,
            "error": self.error,
        })
    }
}

pub(super) struct TaskLogWriter {
    artifact_id: String,
    rel_path: String,
    file: tokio::fs::File,
    max_bytes: u64,
    bytes_total: u64,
    bytes_stored: u64,
    truncated: bool,
}

impl TaskLogWriter {
    pub(super) async fn new(
        config: &TaskEngineConfig,
        artifact_id: &str,
        rel_path: &str,
        max_bytes: usize,
    ) -> Result<Self, String> {
        let path = config.artifacts_blobs_dir().join(artifact_id);
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .map_err(|err| format!("artifact create failed: {err}"))?;
        Ok(Self {
            artifact_id: artifact_id.to_string(),
            rel_path: rel_path.to_string(),
            file,
            max_bytes: max_bytes as u64,
            bytes_total: 0,
            bytes_stored: 0,
            truncated: false,
        })
    }

    pub(super) async fn append(&mut self, chunk: &[u8]) -> Result<Value, ()> {
        let offset = self.bytes_stored;
        self.bytes_total = self.bytes_total.saturating_add(chunk.len() as u64);

        let remaining = self.max_bytes.saturating_sub(self.bytes_stored) as usize;
        let take = remaining.min(chunk.len());
        if take > 0 {
            let mut written = 0;
            while written < take {
                match self.file.write(&chunk[written..take]).await {
                    Ok(0) => return Err(()),
                    Ok(n) => written += n,
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => return Err(()),
                }
            }
            self.bytes_stored = self.bytes_stored.saturating_add(take as u64);
        }
        if (take as u64) < chunk.len() as u64 {
            self.truncated = true;
        }

        Ok(json!({
            "id": self.artifact_id,
            "path": self.rel_path,
            "offset_bytes": offset,
            "bytes": take,
            "bytes_total": self.bytes_total,
            "bytes_stored": self.bytes_stored,
            "truncated": self.truncated,
        }))
    }

    pub(super) fn finish(self) -> TaskLogSummary {
        TaskLogSummary {
            artifact_id: self.artifact_id,
            path: self.rel_path,
            bytes_total: self.bytes_total,
            bytes_stored: self.bytes_stored,
            truncated: self.truncated,
            error: None,
        }
    }
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

pub(super) fn resolve_path(root: &Path, raw: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Err("absolute paths are not allowed".to_string());
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("path escapes workspace root".to_string());
    }
    Ok(root.join(path))
}

pub(super) fn normalize_rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub(super) fn new_artifact_id() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub(super) fn read_artifact_range(
    config: &TaskEngineConfig,
    id: &str,
    offset_bytes: u64,
    max_bytes: usize,
) -> Result<(String, usize, u64, bool), String> {
    if !is_lower_hex_64(id) {
        return Err("invalid artifact id".to_string());
    }

    let path = config.artifacts_blobs_dir().join(id);
    let meta =
        std::fs::metadata(&path).map_err(|err| format!("read artifact meta failed: {err}"))?;
    let total_bytes = meta.len();

    let mut file =
        std::fs::File::open(&path).map_err(|err| format!("read artifact failed: {err}"))?;
    if offset_bytes > 0 {
        use std::io::Seek;
        file.seek(std::io::SeekFrom::Start(offset_bytes))
            .map_err(|err| format!("read artifact failed: {err}"))?;
    }

    let mut buf = vec![0u8; max_bytes];
    use std::io::Read;
    let read_bytes = file
        .read(&mut buf)
        .map_err(|err| format!("read artifact failed: {err}"))?;
    buf.truncate(read_bytes);

    let (content, utf8_truncated, used_bytes) = truncate_utf8(&buf, max_bytes);
    let truncated = utf8_truncated || (offset_bytes + read_bytes as u64) < total_bytes;
    Ok((content, used_bytes, total_bytes, truncated))
}

pub(super) fn is_lower_hex_64(value: &str) -> bool {
    if value.len() != 64 {
        return false;
    }
    value
        .as_bytes()
        .iter()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub(super) fn base64_decode(value: &str) -> Result<Vec<u8>, String> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    if !value.len().is_multiple_of(4) {
        return Err("invalid base64 length".to_string());
    }

    fn decode_char(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity((bytes.len() / 4) * 3);
    let mut i = 0;
    while i < bytes.len() {
        let c0 = bytes[i];
        let c1 = bytes[i + 1];
        let c2 = bytes[i + 2];
        let c3 = bytes[i + 3];

        let v0 = decode_char(c0).ok_or_else(|| "invalid base64 character".to_string())? as u32;
        let v1 = decode_char(c1).ok_or_else(|| "invalid base64 character".to_string())? as u32;

        let pad2 = c2 == b'=';
        let pad3 = c3 == b'=';
        let v2 = if pad2 {
            0
        } else {
            decode_char(c2).ok_or_else(|| "invalid base64 character".to_string())? as u32
        };
        let v3 = if pad3 {
            0
        } else {
            decode_char(c3).ok_or_else(|| "invalid base64 character".to_string())? as u32
        };

        if pad2 && !pad3 {
            return Err("invalid base64 padding".to_string());
        }

        let n = (v0 << 18) | (v1 << 12) | (v2 << 6) | v3;
        out.push(((n >> 16) & 0xff) as u8);
        if !pad2 {
            out.push(((n >> 8) & 0xff) as u8);
        }
        if !pad3 {
            out.push((n & 0xff) as u8);
        }

        i += 4;
    }

    Ok(out)
}
