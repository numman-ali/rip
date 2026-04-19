use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use rip_kernel::{Event, EventKind, StreamKind};
use serde::Deserialize;

use crate::compaction_checkpoint_index::{
    append_entry_best_effort_v1 as append_compaction_checkpoint_index_entry_best_effort_v1,
    compaction_checkpoint_index_path_v1, load_index_v1 as load_compaction_checkpoint_index_v1,
    rebuild_index_from_compaction_sidecar_v1 as rebuild_compaction_checkpoint_index_from_sidecar_v1,
    rebuild_index_from_events_v1 as rebuild_compaction_checkpoint_index_from_events_v1,
    CompactionCheckpointIndexEntryV1,
};
use crate::continuity_seek_index::{
    append_seq_index_entry_best_effort, best_offset_for_seq, insert_message_best_effort_v1,
    load_seq_index_v1, lookup_message_v1, message_index_path,
    rebuild_message_index_from_sidecar_v1, rebuild_seq_index_from_sidecar_v1, seq_index_path,
    validate_seq_index_against_sidecar, SeqSeekIndexEntryV1, SidecarIndexBuilderV1,
    SEEK_INDEX_STRIDE_EVENTS_V1,
};
use crate::message_ordinal_index::{
    append_message_record_best_effort_v1, message_count_v1, message_ordinal_index_path_v1,
    read_message_by_ordinal_v1, rebuild_message_ordinal_index_from_events_v1,
};

mod append;
mod read;
mod scan;
mod window;

#[cfg(test)]
mod tests;

const REVERSE_SCAN_CHUNK_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct TailScan {
    pub(crate) events: Vec<Event>,
    pub(crate) complete: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ContinuityWindow {
    pub(crate) events: Vec<Event>,
    pub(crate) from_seq: u64,
    pub(crate) from_message_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SidecarEventHeader {
    id: String,
    seq: u64,
    session_id: String,
    stream_kind: StreamKind,
    stream_id: String,
    #[serde(rename = "type")]
    event_type: String,
}

/// Best-effort cache for fast continuity replays without scanning the global event log.
///
/// The global `events.jsonl` remains the source of truth; this cache is rebuildable.
pub(crate) struct ContinuityStreamCache {
    dir: PathBuf,
}

impl ContinuityStreamCache {
    pub(crate) fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            dir: data_dir.as_ref().join("continuity_streams"),
        }
    }

    fn path_for(&self, continuity_id: &str) -> PathBuf {
        self.dir.join(format!("{continuity_id}.jsonl"))
    }

    // Sidecar containing only continuity_message_appended + continuity_run_ended (cache-only).
    //
    // Purpose: make recent_messages_v1 window reads O(k) even when the truth continuity stream has
    // high density of non-message events (e.g., continuity_tool_side_effects).
    fn messages_runs_path_for_v1(&self, continuity_id: &str) -> PathBuf {
        self.dir.join(format!("{continuity_id}.mr.v1.jsonl"))
    }

    // Sidecar containing only continuity_compaction_checkpoint_created (cache-only).
    //
    // Purpose: make compaction checkpoint lookups O(k) without scanning the full continuity
    // stream. The truth continuity stream remains canonical; this file is rebuildable.
    fn compaction_checkpoints_path_for_v1(&self, continuity_id: &str) -> PathBuf {
        self.dir.join(format!("{continuity_id}.comp.v1.jsonl"))
    }

    fn compaction_checkpoints_index_path_for_v1(&self, continuity_id: &str) -> PathBuf {
        compaction_checkpoint_index_path_v1(&self.dir, continuity_id)
    }

    fn messages_runs_seq_index_path_v1(&self, continuity_id: &str) -> PathBuf {
        self.dir.join(format!("{continuity_id}.mr.seek.v1.jsonl"))
    }

    fn messages_runs_message_index_path_v1(&self, continuity_id: &str) -> PathBuf {
        self.dir.join(format!("{continuity_id}.mr.messages.v1.bin"))
    }

    fn messages_runs_message_ordinal_index_path_v1(&self, continuity_id: &str) -> PathBuf {
        message_ordinal_index_path_v1(&self.dir, continuity_id)
    }
}
