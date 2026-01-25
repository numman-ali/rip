use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub(crate) const COMPACTION_SUMMARY_SCHEMA_V1: &str = "rip.compaction_summary.v1";
pub(crate) const COMPACTION_SUMMARY_KIND_CUMULATIVE_V1: &str = "cumulative_v1";

#[derive(Debug, Clone)]
pub(crate) struct NewCumulativeCompactionSummaryV1 {
    pub(crate) thread_id: String,
    pub(crate) to_seq: u64,
    pub(crate) to_message_id: Option<String>,
    pub(crate) actor_id: String,
    pub(crate) origin: String,
    pub(crate) produced_by: Option<(String, String)>,
    pub(crate) base_summary_artifact_id: Option<String>,
    pub(crate) basis_note: Option<String>,
    pub(crate) summary_markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompactionSummaryV1 {
    schema: String,
    kind: String,
    coverage: CompactionSummaryCoverageV1,
    provenance: CompactionSummaryProvenanceV1,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    basis: Option<CompactionSummaryBasisV1>,
    summary_markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompactionSummaryCoverageV1 {
    thread_id: String,
    from_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from_message_id: Option<String>,
    to_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompactionSummaryProvenanceV1 {
    actor_id: String,
    origin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    produced_by: Option<CompactionSummaryProducedByV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompactionSummaryProducedByV1 {
    #[serde(rename = "type")]
    produced_by_type: String,
    id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompactionSummaryBasisV1 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    base_summary_artifact_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    note: Option<String>,
}

impl CompactionSummaryV1 {
    pub(crate) fn new_cumulative_source_cut(req: NewCumulativeCompactionSummaryV1) -> Self {
        Self {
            schema: COMPACTION_SUMMARY_SCHEMA_V1.to_string(),
            kind: COMPACTION_SUMMARY_KIND_CUMULATIVE_V1.to_string(),
            coverage: CompactionSummaryCoverageV1 {
                thread_id: req.thread_id,
                from_seq: 0,
                from_message_id: None,
                to_seq: req.to_seq,
                to_message_id: req.to_message_id,
            },
            provenance: CompactionSummaryProvenanceV1 {
                actor_id: req.actor_id,
                origin: req.origin,
                produced_by: req.produced_by.map(|(produced_by_type, id)| {
                    CompactionSummaryProducedByV1 {
                        produced_by_type,
                        id,
                    }
                }),
            },
            basis: Some(CompactionSummaryBasisV1 {
                base_summary_artifact_id: req.base_summary_artifact_id,
                note: req.basis_note,
            })
            .filter(|basis| basis.base_summary_artifact_id.is_some() || basis.note.is_some()),
            summary_markdown: req.summary_markdown,
        }
    }

    pub(crate) fn schema(&self) -> &str {
        &self.schema
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) fn coverage_thread_id(&self) -> &str {
        &self.coverage.thread_id
    }

    pub(crate) fn coverage_to_seq(&self) -> u64 {
        self.coverage.to_seq
    }

    pub(crate) fn summary_markdown(&self) -> &str {
        &self.summary_markdown
    }
}

pub(crate) fn write_compaction_summary_v1(
    workspace_root: &Path,
    summary: &CompactionSummaryV1,
) -> Result<String, String> {
    let bytes = serde_json::to_vec(summary)
        .map_err(|err| format!("compaction summary serialize failed: {err}"))?;
    let artifact_id = new_artifact_id();
    write_blob_atomic(workspace_root, &artifact_id, &bytes)?;
    Ok(artifact_id)
}

pub(crate) fn read_compaction_summary_v1(
    workspace_root: &Path,
    artifact_id: &str,
) -> Result<CompactionSummaryV1, String> {
    let path = artifacts_blobs_dir(workspace_root).join(artifact_id);
    let bytes = fs::read(&path).map_err(|err| format!("artifact read failed: {err}"))?;
    let summary: CompactionSummaryV1 = serde_json::from_slice(&bytes)
        .map_err(|err| format!("compaction summary parse failed: {err}"))?;
    if summary.schema() != COMPACTION_SUMMARY_SCHEMA_V1 {
        return Err(format!(
            "unexpected compaction summary schema: expected {}, got {}",
            COMPACTION_SUMMARY_SCHEMA_V1,
            summary.schema()
        ));
    }
    Ok(summary)
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
    fn write_and_read_compaction_summary_v1_round_trips() {
        let dir = tempdir().expect("tmp");
        let workspace_root = dir.path();

        let summary =
            CompactionSummaryV1::new_cumulative_source_cut(NewCumulativeCompactionSummaryV1 {
                thread_id: "t1".to_string(),
                to_seq: 42,
                to_message_id: Some("m42".to_string()),
                actor_id: "alice".to_string(),
                origin: "cli".to_string(),
                produced_by: Some(("manual".to_string(), "cli".to_string())),
                base_summary_artifact_id: None,
                basis_note: None,
                summary_markdown: "hello".to_string(),
            });

        let id = write_compaction_summary_v1(workspace_root, &summary).expect("write");
        let loaded = read_compaction_summary_v1(workspace_root, &id).expect("read");
        assert_eq!(loaded.schema(), COMPACTION_SUMMARY_SCHEMA_V1);
        assert_eq!(loaded.kind(), COMPACTION_SUMMARY_KIND_CUMULATIVE_V1);
        assert_eq!(loaded.coverage_thread_id(), "t1");
        assert_eq!(loaded.coverage_to_seq(), 42);
        assert_eq!(loaded.summary_markdown(), "hello");
    }
}
