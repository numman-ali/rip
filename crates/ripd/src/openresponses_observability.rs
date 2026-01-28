use std::fs;
use std::path::{Path, PathBuf};

use rip_kernel::{Event, EventKind};
use serde_json::Value;
use uuid::Uuid;

const DEFAULT_MAX_BYTES: usize = 1_000_000;

#[derive(Clone, Copy, Debug)]
pub(crate) struct OpenResponsesRequestDumpConfig {
    pub(crate) enabled: bool,
    pub(crate) max_bytes: usize,
}

pub(crate) fn request_dump_config_from_env() -> OpenResponsesRequestDumpConfig {
    let enabled = std::env::var("RIP_OPENRESPONSES_DUMP_REQUEST")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);

    let max_bytes = std::env::var("RIP_OPENRESPONSES_DUMP_REQUEST_MAX_BYTES")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_BYTES);

    OpenResponsesRequestDumpConfig { enabled, max_bytes }
}

pub(crate) struct OpenResponsesRequestDumpInput<'a> {
    pub(crate) workspace_root: &'a Path,
    pub(crate) session_id: &'a str,
    pub(crate) timestamp_ms: u64,
    pub(crate) seq: u64,
    pub(crate) endpoint: &'a str,
    pub(crate) request_index: u64,
    pub(crate) kind: &'a str,
    pub(crate) body: &'a Value,
}

pub(crate) fn maybe_dump_openresponses_request(
    config: OpenResponsesRequestDumpConfig,
    input: OpenResponsesRequestDumpInput<'_>,
) -> Result<Option<Event>, String> {
    if !config.enabled {
        return Ok(None);
    }

    let bytes =
        serde_json::to_vec(input.body).map_err(|err| format!("request serialize failed: {err}"))?;
    let total_bytes = bytes.len() as u64;

    let mut stored = bytes;
    let mut truncated = false;
    if stored.len() > config.max_bytes {
        stored.truncate(config.max_bytes);
        truncated = true;
    }
    let body_bytes = stored.len() as u64;

    let body_artifact_id = new_artifact_id();
    write_blob_atomic(input.workspace_root, &body_artifact_id, &stored)?;

    let model = input
        .body
        .get("model")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());

    Ok(Some(Event {
        id: Uuid::new_v4().to_string(),
        session_id: input.session_id.to_string(),
        timestamp_ms: input.timestamp_ms,
        seq: input.seq,
        kind: EventKind::OpenResponsesRequest {
            endpoint: input.endpoint.to_string(),
            model,
            request_index: input.request_index,
            kind: input.kind.to_string(),
            body_artifact_id,
            body_bytes,
            total_bytes,
            truncated,
        },
    }))
}

fn artifacts_blobs_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(".rip").join("artifacts").join("blobs")
}

fn write_blob_atomic(workspace_root: &Path, artifact_id: &str, bytes: &[u8]) -> Result<(), String> {
    let dir = artifacts_blobs_dir(workspace_root);
    fs::create_dir_all(&dir).map_err(|err| format!("artifact dir create failed: {err}"))?;

    let path = dir.join(artifact_id);
    let tmp = dir.join(format!("{artifact_id}.tmp"));
    fs::write(&tmp, bytes).map_err(|err| format!("artifact write failed: {err}"))?;
    fs::rename(&tmp, &path).map_err(|err| format!("artifact finalize failed: {err}"))?;
    Ok(())
}

fn new_artifact_id() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
