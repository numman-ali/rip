use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use uuid::Uuid;

const CONTEXT_BUNDLE_SCHEMA_V1: &str = "rip.context_bundle.v1";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ContextBundleV1 {
    schema: &'static str,
    compiler: ContextBundleCompilerV1,
    source: ContextBundleSourceV1,
    provenance: ContextBundleProvenanceV1,
    items: Vec<ContextBundleItemV1>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ContextBundleCompilerV1 {
    pub(crate) id: String,
    pub(crate) strategy: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ContextBundleSourceV1 {
    pub(crate) thread_id: String,
    pub(crate) from_seq: u64,
    pub(crate) from_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ContextBundleProvenanceV1 {
    pub(crate) run_session_id: String,
    pub(crate) actor_id: String,
    pub(crate) origin: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum ContextBundleItemV1 {
    Message {
        role: String,
        content: String,
        actor_id: Option<String>,
        origin: Option<String>,
        thread_seq: Option<u64>,
        thread_event_id: Option<String>,
    },
    SummaryRef {
        artifact_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
}

impl ContextBundleV1 {
    pub(crate) fn new(
        compiler: ContextBundleCompilerV1,
        source: ContextBundleSourceV1,
        provenance: ContextBundleProvenanceV1,
        items: Vec<ContextBundleItemV1>,
    ) -> Self {
        Self {
            schema: CONTEXT_BUNDLE_SCHEMA_V1,
            compiler,
            source,
            provenance,
            items,
        }
    }

    pub(crate) fn items(&self) -> &[ContextBundleItemV1] {
        &self.items
    }
}

pub(crate) fn write_bundle_v1(
    workspace_root: &Path,
    bundle: &ContextBundleV1,
) -> Result<String, String> {
    let bytes = serde_json::to_vec(bundle)
        .map_err(|err| format!("context bundle serialize failed: {err}"))?;
    let artifact_id = new_artifact_id();
    write_blob_atomic(workspace_root, &artifact_id, &bytes)?;
    Ok(artifact_id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_bundle_v1_creates_blob() {
        let dir = tempdir().expect("tmp");
        let workspace_root = dir.path();

        let bundle = ContextBundleV1::new(
            ContextBundleCompilerV1 {
                id: "rip.context_compiler.v1".to_string(),
                strategy: "recent_messages_v1".to_string(),
            },
            ContextBundleSourceV1 {
                thread_id: "t1".to_string(),
                from_seq: 3,
                from_message_id: Some("m1".to_string()),
            },
            ContextBundleProvenanceV1 {
                run_session_id: "s1".to_string(),
                actor_id: "alice".to_string(),
                origin: "cli".to_string(),
            },
            vec![ContextBundleItemV1::Message {
                role: "user".to_string(),
                content: "hello".to_string(),
                actor_id: Some("alice".to_string()),
                origin: Some("cli".to_string()),
                thread_seq: Some(3),
                thread_event_id: Some("m1".to_string()),
            }],
        );

        let id = write_bundle_v1(workspace_root, &bundle).expect("write");

        let blob_path = artifacts_blobs_dir(workspace_root).join(&id);
        let bytes = fs::read(&blob_path).expect("read");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            json.get("schema").and_then(|v| v.as_str()),
            Some(CONTEXT_BUNDLE_SCHEMA_V1)
        );
        assert_eq!(
            json.get("compiler")
                .and_then(|v| v.get("strategy"))
                .and_then(|v| v.as_str()),
            Some("recent_messages_v1")
        );
        assert_eq!(
            json.get("source")
                .and_then(|v| v.get("thread_id"))
                .and_then(|v| v.as_str()),
            Some("t1")
        );
        assert!(json.get("items").and_then(|v| v.as_array()).is_some());
    }
}
