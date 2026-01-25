use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use rip_kernel::{Event, EventKind, StreamKind};
use serde::{Deserialize, Serialize};

pub(crate) const COMPACTION_CHECKPOINT_INDEX_VERSION_V1: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CompactionCheckpointIndexEntryV1 {
    version: u32,
    pub(crate) seq: u64,
    pub(crate) to_seq: u64,
    pub(crate) checkpoint_id: String,
    pub(crate) cut_rule_id: String,
    pub(crate) summary_kind: String,
    pub(crate) summary_artifact_id: String,
}

impl CompactionCheckpointIndexEntryV1 {
    pub(crate) fn from_event(event: &Event) -> Option<Self> {
        if event.stream_kind() != StreamKind::Continuity {
            return None;
        }
        let EventKind::ContinuityCompactionCheckpointCreated {
            checkpoint_id,
            cut_rule_id,
            summary_kind,
            summary_artifact_id,
            to_seq,
            ..
        } = &event.kind
        else {
            return None;
        };

        Some(Self {
            version: COMPACTION_CHECKPOINT_INDEX_VERSION_V1,
            seq: event.seq,
            to_seq: *to_seq,
            checkpoint_id: checkpoint_id.clone(),
            cut_rule_id: cut_rule_id.clone(),
            summary_kind: summary_kind.clone(),
            summary_artifact_id: summary_artifact_id.clone(),
        })
    }
}

pub(crate) fn compaction_checkpoint_index_path_v1(dir: &Path, continuity_id: &str) -> PathBuf {
    dir.join(format!("{continuity_id}.comp.idx.v1.jsonl"))
}

pub(crate) fn append_entry_best_effort_v1(path: &Path, entry: &CompactionCheckpointIndexEntryV1) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }

    let Ok(file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let mut writer = BufWriter::new(file);
    let Ok(line) = serde_json::to_string(entry) else {
        return;
    };
    let _ = writer.write_all(line.as_bytes());
    let _ = writer.write_all(b"\n");
    let _ = writer.flush();
}

/// Returns `Ok(None)` when the index file doesn't exist.
///
/// Validation errors are returned as `Err` so callers can rebuild from sidecars.
pub(crate) fn load_index_v1(
    path: &Path,
) -> io::Result<Option<Vec<CompactionCheckpointIndexEntryV1>>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };

    let reader = BufReader::new(file);
    let mut entries: Vec<CompactionCheckpointIndexEntryV1> = Vec::new();
    let mut last_seq: Option<u64> = None;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: CompactionCheckpointIndexEntryV1 = serde_json::from_str(&line)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if entry.version != COMPACTION_CHECKPOINT_INDEX_VERSION_V1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "compaction checkpoint index version mismatch",
            ));
        }
        if let Some(prev) = last_seq {
            if entry.seq < prev {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "compaction checkpoint index seq is not monotonic",
                ));
            }
        }
        last_seq = Some(entry.seq);
        entries.push(entry);
    }

    if entries.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "compaction checkpoint index is empty",
        ));
    }

    Ok(Some(entries))
}

pub(crate) fn rebuild_index_from_events_v1(
    index_path: &Path,
    continuity_id: &str,
    events: &[Event],
) -> io::Result<()> {
    let tmp = index_path.with_extension("jsonl.tmp");
    if let Some(parent) = tmp.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = File::create(&tmp)?;
    let mut writer = BufWriter::new(file);

    let mut wrote_any = false;
    for event in events {
        if event.stream_kind() != StreamKind::Continuity || event.stream_id() != continuity_id {
            continue;
        }
        let Some(entry) = CompactionCheckpointIndexEntryV1::from_event(event) else {
            continue;
        };
        let line = serde_json::to_string(&entry)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
        wrote_any = true;
    }
    writer.flush()?;

    if !wrote_any {
        let _ = fs::remove_file(&tmp);
        let _ = fs::remove_file(index_path);
        return Ok(());
    }

    fs::rename(tmp, index_path)?;
    Ok(())
}

pub(crate) fn rebuild_index_from_compaction_sidecar_v1(
    sidecar_path: &Path,
    index_path: &Path,
    continuity_id: &str,
) -> io::Result<()> {
    let sidecar = File::open(sidecar_path)?;
    let reader = BufReader::new(sidecar);
    let mut events: Vec<Event> = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let event: Event = serde_json::from_str(&line)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if event.stream_kind() != StreamKind::Continuity || event.stream_id() != continuity_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "compaction checkpoint sidecar contains non-continuity event while building index",
            ));
        }
        if !matches!(
            event.kind,
            EventKind::ContinuityCompactionCheckpointCreated { .. }
        ) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "compaction checkpoint sidecar contains non-checkpoint event while building index",
            ));
        }
        events.push(event);
    }

    rebuild_index_from_events_v1(index_path, continuity_id, &events)
}
