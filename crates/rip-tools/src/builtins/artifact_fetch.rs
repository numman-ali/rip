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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn config_for(root: &Path) -> BuiltinToolConfig {
        BuiltinToolConfig {
            workspace_root: root.to_path_buf(),
            artifact_max_bytes: 1024,
            max_bytes: 64,
            max_results: 10,
            max_depth: 4,
            follow_symlinks: false,
            include_hidden: false,
        }
    }

    #[test]
    fn artifact_fetch_rejects_invalid_id() {
        let dir = tempdir().expect("tmp");
        let config = config_for(dir.path());
        let output = run_artifact_fetch(
            ToolInvocation {
                name: "artifact_fetch".to_string(),
                args: serde_json::json!({"id":"not-hex"}),
                timeout_ms: None,
            },
            &config,
        );
        assert_eq!(output.exit_code, 2);
        assert!(output.stderr.join("\n").contains("id must be"));
    }

    #[test]
    fn artifact_fetch_reads_with_offset() {
        let dir = tempdir().expect("tmp");
        let config = config_for(dir.path());
        let blobs = artifacts_blobs_dir(&config);
        std::fs::create_dir_all(&blobs).expect("blobs");
        let id = "a".repeat(64);
        let path = blobs.join(&id);
        std::fs::write(&path, b"hello world").expect("write");

        let output = run_artifact_fetch(
            ToolInvocation {
                name: "artifact_fetch".to_string(),
                args: serde_json::json!({"id": id, "offset_bytes": 6, "max_bytes": 5}),
                timeout_ms: None,
            },
            &config,
        );
        assert_eq!(output.exit_code, 0);
        assert_eq!(output.stdout, vec!["world".to_string()]);
        let artifacts = output.artifacts.expect("artifacts");
        assert_eq!(
            artifacts.get("offset_bytes").and_then(|v| v.as_u64()),
            Some(6)
        );
        assert_eq!(
            artifacts.get("truncated").and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn artifact_fetch_reports_missing_blob() {
        let dir = tempdir().expect("tmp");
        let config = config_for(dir.path());
        let output = run_artifact_fetch(
            ToolInvocation {
                name: "artifact_fetch".to_string(),
                args: serde_json::json!({"id": "b".repeat(64)}),
                timeout_ms: None,
            },
            &config,
        );
        assert_eq!(output.exit_code, 1);
        assert!(output.stderr.join("\n").contains("artifact_fetch failed"));
    }
}
