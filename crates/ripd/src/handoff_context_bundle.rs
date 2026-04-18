use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use uuid::Uuid;

const HANDOFF_CONTEXT_BUNDLE_SCHEMA_V1: &str = "rip.handoff_context_bundle.v1";

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HandoffContextBundleV1 {
    schema: &'static str,
    summary_markdown: String,
    refs: HandoffContextBundleRefsV1,
}

#[derive(Debug, Clone, Serialize)]
struct HandoffContextBundleRefsV1 {
    threads: Vec<HandoffThreadRefV1>,
    artifacts: Vec<HandoffArtifactRefV1>,
    files: Vec<HandoffFileRefV1>,
}

#[derive(Debug, Clone, Serialize)]
struct HandoffThreadRefV1 {
    thread_id: String,
    seq: u64,
    message_id: Option<String>,
    note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct HandoffArtifactRefV1 {
    artifact_id: String,
    note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct HandoffFileRefV1 {
    path: String,
    note: Option<String>,
}

impl HandoffContextBundleV1 {
    pub(crate) fn new_source_cut(
        summary_markdown: String,
        from_thread_id: String,
        from_seq: u64,
        from_message_id: Option<String>,
    ) -> Self {
        Self {
            schema: HANDOFF_CONTEXT_BUNDLE_SCHEMA_V1,
            summary_markdown,
            refs: HandoffContextBundleRefsV1 {
                threads: vec![HandoffThreadRefV1 {
                    thread_id: from_thread_id,
                    seq: from_seq,
                    message_id: from_message_id,
                    note: Some("source cut".to_string()),
                }],
                artifacts: Vec::new(),
                files: Vec::new(),
            },
        }
    }
}

pub(crate) fn write_bundle_v1(
    workspace_root: &Path,
    bundle: &HandoffContextBundleV1,
) -> Result<String, String> {
    let bytes = serde_json::to_vec(bundle)
        .map_err(|err| format!("handoff bundle serialize failed: {err}"))?;
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

        let id = write_bundle_v1(
            workspace_root,
            &HandoffContextBundleV1::new_source_cut(
                "summary".to_string(),
                "t1".to_string(),
                3,
                Some("m1".to_string()),
            ),
        )
        .expect("write");

        let blob_path = artifacts_blobs_dir(workspace_root).join(&id);
        let bytes = fs::read(&blob_path).expect("read");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(
            json.get("schema").and_then(|v| v.as_str()),
            Some(HANDOFF_CONTEXT_BUNDLE_SCHEMA_V1)
        );
        assert_eq!(
            json.get("summary_markdown").and_then(|v| v.as_str()),
            Some("summary")
        );
        assert!(json
            .get("refs")
            .and_then(|v| v.get("threads"))
            .and_then(|v| v.as_array())
            .is_some());
    }

    #[test]
    fn source_cut_and_write_failures_are_serialized_cleanly() {
        let bundle = HandoffContextBundleV1::new_source_cut(
            "handoff".to_string(),
            "thread-1".to_string(),
            9,
            None,
        );
        let json = serde_json::to_value(&bundle).expect("json");
        assert_eq!(
            json.get("schema").and_then(|value| value.as_str()),
            Some(HANDOFF_CONTEXT_BUNDLE_SCHEMA_V1)
        );
        assert_eq!(
            json.get("summary_markdown")
                .and_then(|value| value.as_str()),
            Some("handoff")
        );
        assert_eq!(
            json.get("refs")
                .and_then(|value| value.get("threads"))
                .and_then(|value| value.as_array())
                .and_then(|threads| threads.first())
                .and_then(|thread| thread.get("thread_id"))
                .and_then(|value| value.as_str()),
            Some("thread-1")
        );

        let blocked = tempdir().expect("tmp");
        fs::create_dir_all(blocked.path().join(".rip")).expect("rip dir");
        fs::write(blocked.path().join(".rip").join("artifacts"), b"blocked")
            .expect("write blocker");
        let err = write_bundle_v1(blocked.path(), &bundle).expect_err("dir failure");
        assert!(err.contains("artifact dir create failed"));
    }
}
