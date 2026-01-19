use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use serde::Deserialize;
use serde_json::json;

use crate::{ToolInvocation, ToolOutput};

use super::{parse_args, truncate_utf8, BuiltinToolConfig};

#[derive(Deserialize)]
struct ArtifactFetchArgs {
    id: String,
    offset_bytes: Option<u64>,
    max_bytes: Option<usize>,
}

pub(super) fn run_artifact_fetch(
    invocation: ToolInvocation,
    config: &BuiltinToolConfig,
) -> ToolOutput {
    let args: ArtifactFetchArgs = match parse_args(invocation.args) {
        Ok(args) => args,
        Err(err) => return err,
    };

    if !is_sha256_hex(&args.id) {
        return ToolOutput::invalid_args("id must be a 64-char lowercase hex sha256".to_string());
    }

    let offset = args.offset_bytes.unwrap_or(0);
    let max_bytes = args.max_bytes.unwrap_or(config.max_bytes);

    let path = artifacts_blobs_dir(config).join(&args.id);
    let meta = match std::fs::metadata(&path) {
        Ok(meta) => meta,
        Err(err) => return ToolOutput::failure(vec![format!("artifact_fetch failed: {err}")]),
    };
    let total_bytes = meta.len();

    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(err) => return ToolOutput::failure(vec![format!("artifact_fetch failed: {err}")]),
    };

    if offset > 0 {
        if let Err(err) = file.seek(SeekFrom::Start(offset)) {
            return ToolOutput::failure(vec![format!("artifact_fetch failed: {err}")]);
        }
    }

    let mut buf = vec![0u8; max_bytes];
    let read_bytes = match file.read(&mut buf) {
        Ok(n) => n,
        Err(err) => return ToolOutput::failure(vec![format!("artifact_fetch failed: {err}")]),
    };
    buf.truncate(read_bytes);

    let (content, utf8_truncated, used_bytes) = truncate_utf8(&buf, max_bytes);
    let truncated = utf8_truncated || (offset + read_bytes as u64) < total_bytes;

    ToolOutput {
        stdout: vec![content],
        stderr: Vec::new(),
        exit_code: 0,
        artifacts: Some(json!({
            "id": args.id,
            "path": path_rel(&config.workspace_root, &path),
            "offset_bytes": offset,
            "bytes": used_bytes,
            "total_bytes": total_bytes,
            "truncated": truncated,
        })),
    }
}

fn artifacts_blobs_dir(config: &BuiltinToolConfig) -> std::path::PathBuf {
    config.artifacts_root().join("blobs")
}

fn path_rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_sha256_hex(id: &str) -> bool {
    if id.len() != 64 {
        return false;
    }
    id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
