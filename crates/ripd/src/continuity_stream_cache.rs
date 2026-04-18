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

    pub(crate) fn append_best_effort(&self, event: &Event) {
        if event.stream_kind() != StreamKind::Continuity {
            return;
        }

        let continuity_id = event.stream_id();
        let path = self.path_for(continuity_id);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
            return;
        };
        let offset = match file.metadata() {
            Ok(meta) => meta.len(),
            Err(_) => return,
        };

        let mut writer = BufWriter::new(file);
        let Ok(line) = serde_json::to_string(event) else {
            return;
        };
        if writer.write_all(line.as_bytes()).is_err() {
            return;
        }
        if writer.write_all(b"\n").is_err() {
            return;
        }
        if writer.flush().is_err() {
            return;
        }

        // Best-effort indexes (rebuildable caches) to avoid full sidecar scans.
        if event.seq.is_multiple_of(SEEK_INDEX_STRIDE_EVENTS_V1) {
            let seek_path = seq_index_path(&self.dir, continuity_id);
            append_seq_index_entry_best_effort(
                &seek_path,
                &SeqSeekIndexEntryV1::new(event.seq, offset),
            );
        }
        if matches!(
            &event.kind,
            rip_kernel::EventKind::ContinuityMessageAppended { .. }
        ) {
            let msg_path = message_index_path(&self.dir, continuity_id);
            insert_message_best_effort_v1(&msg_path, &path, &event.id, event.seq, offset);
        }

        // Additional cache: messages+runs-only sidecar + indexes.
        self.append_messages_runs_best_effort_v1(event);

        // Additional cache: compaction checkpoints only (summary selection).
        self.append_compaction_checkpoints_best_effort_v1(event);
    }

    pub(crate) fn rebuild_best_effort(&self, continuity_id: &str, events: &[Event]) {
        let path = self.path_for(continuity_id);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let Ok(file) = File::create(&path) else {
            return;
        };
        let mut writer = BufWriter::new(file);
        let mut offset: u64 = 0;
        let mut index_builder = SidecarIndexBuilderV1::new();
        for event in events {
            if event.stream_kind() != StreamKind::Continuity || event.stream_id() != continuity_id {
                continue;
            }
            let Ok(line) = serde_json::to_string(event) else {
                continue;
            };
            index_builder.observe_event(event, offset);
            let _ = writer.write_all(line.as_bytes());
            let _ = writer.write_all(b"\n");
            offset = offset.saturating_add(line.len() as u64 + 1);
        }
        let _ = writer.flush();

        let _ = index_builder.write_best_effort(&self.dir, continuity_id);

        self.rebuild_messages_runs_best_effort_v1(continuity_id, events);
        self.rebuild_compaction_checkpoints_best_effort_v1(continuity_id, events);
    }

    fn append_messages_runs_best_effort_v1(&self, event: &Event) {
        if event.stream_kind() != StreamKind::Continuity {
            return;
        }
        if !matches!(
            &event.kind,
            EventKind::ContinuityMessageAppended { .. } | EventKind::ContinuityRunEnded { .. }
        ) {
            return;
        }

        let continuity_id = event.stream_id();
        let path = self.messages_runs_path_for_v1(continuity_id);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
            return;
        };
        let offset = match file.metadata() {
            Ok(meta) => meta.len(),
            Err(_) => return,
        };

        let mut writer = BufWriter::new(file);
        let Ok(line) = serde_json::to_string(event) else {
            return;
        };
        if writer.write_all(line.as_bytes()).is_err() {
            return;
        }
        if writer.write_all(b"\n").is_err() {
            return;
        }
        if writer.flush().is_err() {
            return;
        }

        // Best-effort indexes (rebuildable caches).
        let seek_path = self.messages_runs_seq_index_path_v1(continuity_id);
        // Record seek entries for every message event; offsets are still monotonic and allow
        // bounded window reads keyed by continuity seq values.
        if matches!(&event.kind, EventKind::ContinuityMessageAppended { .. }) {
            append_seq_index_entry_best_effort(
                &seek_path,
                &SeqSeekIndexEntryV1::new(event.seq, offset),
            );
        }
        if matches!(&event.kind, EventKind::ContinuityMessageAppended { .. }) {
            let msg_path = self.messages_runs_message_index_path_v1(continuity_id);
            insert_message_best_effort_v1(&msg_path, &path, &event.id, event.seq, offset);
            let ord_path = self.messages_runs_message_ordinal_index_path_v1(continuity_id);
            append_message_record_best_effort_v1(&ord_path, event.seq, &event.id);
        }
    }

    fn append_compaction_checkpoints_best_effort_v1(&self, event: &Event) {
        if event.stream_kind() != StreamKind::Continuity {
            return;
        }
        if !matches!(
            &event.kind,
            EventKind::ContinuityCompactionCheckpointCreated { .. }
        ) {
            return;
        }

        let continuity_id = event.stream_id();
        let path = self.compaction_checkpoints_path_for_v1(continuity_id);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
            return;
        };

        let mut writer = BufWriter::new(file);
        let Ok(line) = serde_json::to_string(event) else {
            return;
        };
        if writer.write_all(line.as_bytes()).is_err() {
            return;
        }
        if writer.write_all(b"\n").is_err() {
            return;
        }
        let _ = writer.flush();

        if let Some(entry) = CompactionCheckpointIndexEntryV1::from_event(event) {
            let idx_path = self.compaction_checkpoints_index_path_for_v1(continuity_id);
            append_compaction_checkpoint_index_entry_best_effort_v1(&idx_path, &entry);
        }
    }

    fn rebuild_messages_runs_best_effort_v1(&self, continuity_id: &str, events: &[Event]) {
        let path = self.messages_runs_path_for_v1(continuity_id);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let tmp_path = path.with_extension("jsonl.tmp");
        let tmp_seek = self
            .messages_runs_seq_index_path_v1(continuity_id)
            .with_extension("jsonl.tmp");

        let Ok(file) = File::create(&tmp_path) else {
            return;
        };
        let Ok(seek_file) = File::create(&tmp_seek) else {
            return;
        };

        let mut writer = BufWriter::new(file);
        let mut seek_writer = BufWriter::new(seek_file);
        let mut offset: u64 = 0;
        let mut wrote_any = false;

        for event in events {
            if event.stream_kind() != StreamKind::Continuity || event.stream_id() != continuity_id {
                continue;
            }
            if !matches!(
                &event.kind,
                EventKind::ContinuityMessageAppended { .. } | EventKind::ContinuityRunEnded { .. }
            ) {
                continue;
            }

            let Ok(line) = serde_json::to_string(event) else {
                continue;
            };

            // Seek index entries for messages (sparse seq is OK; offsets remain monotonic).
            if matches!(&event.kind, EventKind::ContinuityMessageAppended { .. }) {
                if let Ok(entry) =
                    serde_json::to_string(&SeqSeekIndexEntryV1::new(event.seq, offset))
                {
                    let _ = seek_writer.write_all(entry.as_bytes());
                    let _ = seek_writer.write_all(b"\n");
                }
            }

            let _ = writer.write_all(line.as_bytes());
            let _ = writer.write_all(b"\n");
            wrote_any = true;
            offset = offset.saturating_add(line.len() as u64 + 1);
        }

        let _ = writer.flush();
        let _ = seek_writer.flush();

        if !wrote_any {
            let _ = fs::remove_file(&tmp_path);
            let _ = fs::remove_file(&tmp_seek);
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(self.messages_runs_seq_index_path_v1(continuity_id));
            let _ = fs::remove_file(self.messages_runs_message_index_path_v1(continuity_id));
            let _ =
                fs::remove_file(self.messages_runs_message_ordinal_index_path_v1(continuity_id));
            return;
        }

        let _ = fs::rename(tmp_path, path);
        let _ = fs::rename(
            tmp_seek,
            self.messages_runs_seq_index_path_v1(continuity_id),
        );

        // Build the message_id -> (seq, offset) index from the sidecar (cache-only; no truth log).
        let msg_path = self.messages_runs_message_index_path_v1(continuity_id);
        let _ = rebuild_message_index_from_sidecar_v1(
            &self.messages_runs_path_for_v1(continuity_id),
            &msg_path,
        );

        // Build the ordinal -> (seq, id) index from the full event list (cache-only; no truth log).
        let ord_path = self.messages_runs_message_ordinal_index_path_v1(continuity_id);
        let _ = rebuild_message_ordinal_index_from_events_v1(&ord_path, continuity_id, events);
    }

    /// Returns `Ok(None)` when the ordinal index doesn't exist.
    pub(crate) fn message_count_messages_runs_v1(
        &self,
        continuity_id: &str,
    ) -> io::Result<Option<u64>> {
        let path = self.messages_runs_message_ordinal_index_path_v1(continuity_id);
        let Some(count) = message_count_v1(&path)? else {
            return Ok(None);
        };

        // Validate the last ordinal record against the messages+runs sidecar so partial/truncated
        // caches don't change cut-point selection semantics. Callers may fall back to the truth log.
        let last_message = self.try_read_last_message_appended_messages_runs_v1(continuity_id)?;
        let Some((last_seq, last_id)) = last_message else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "message ordinal index cannot be validated (missing messages+runs sidecar)",
            ));
        };

        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "message ordinal index empty but messages+runs sidecar contains messages",
            ));
        }
        let Some(record) = read_message_by_ordinal_v1(&path, count)? else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "message ordinal index last record is missing",
            ));
        };
        if record.seq != last_seq || record.id.to_string() != last_id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "message ordinal index out of sync with messages+runs sidecar",
            ));
        }

        Ok(Some(count))
    }

    /// Returns `Ok(None)` when the ordinal index doesn't exist or the ordinal is out of range.
    pub(crate) fn message_by_ordinal_messages_runs_v1(
        &self,
        continuity_id: &str,
        ordinal: u64,
    ) -> io::Result<Option<(u64, String)>> {
        let path = self.messages_runs_message_ordinal_index_path_v1(continuity_id);
        let Some(record) = read_message_by_ordinal_v1(&path, ordinal)? else {
            return Ok(None);
        };

        // Validate the record points at an actual message in the messages+runs sidecar.
        let message_id = record.id.to_string();
        let idx_path = self.messages_runs_message_index_path_v1(continuity_id);
        let mut matches_index = match lookup_message_v1(&idx_path, &message_id) {
            Ok(Some((seq, _))) => seq == record.seq,
            Ok(None) => false,
            Err(_) => false,
        };
        if !matches_index {
            if let Some(sidecar_path) =
                self.ensure_messages_runs_sidecar_best_effort_v1(continuity_id)?
            {
                let _ = rebuild_message_index_from_sidecar_v1(&sidecar_path, &idx_path);
            }
            matches_index = match lookup_message_v1(&idx_path, &message_id) {
                Ok(Some((seq, _))) => seq == record.seq,
                _ => false,
            };
        }
        if !matches_index {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "message ordinal index references missing message",
            ));
        }

        Ok(Some((record.seq, message_id)))
    }

    fn rebuild_compaction_checkpoints_best_effort_v1(&self, continuity_id: &str, events: &[Event]) {
        let path = self.compaction_checkpoints_path_for_v1(continuity_id);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let tmp_path = path.with_extension("jsonl.tmp");
        let Ok(file) = File::create(&tmp_path) else {
            return;
        };
        let mut writer = BufWriter::new(file);

        let mut wrote_any = false;
        for event in events {
            if event.stream_kind() != StreamKind::Continuity || event.stream_id() != continuity_id {
                continue;
            }
            if !matches!(
                &event.kind,
                EventKind::ContinuityCompactionCheckpointCreated { .. }
            ) {
                continue;
            }

            let Ok(line) = serde_json::to_string(event) else {
                continue;
            };
            let _ = writer.write_all(line.as_bytes());
            let _ = writer.write_all(b"\n");
            wrote_any = true;
        }

        let _ = writer.flush();
        if !wrote_any {
            let _ = fs::remove_file(&tmp_path);
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(self.compaction_checkpoints_index_path_for_v1(continuity_id));
            return;
        }

        let _ = fs::rename(tmp_path, path);
        let _ = rebuild_compaction_checkpoint_index_from_events_v1(
            &self.compaction_checkpoints_index_path_for_v1(continuity_id),
            continuity_id,
            events,
        );
    }

    /// Returns `Ok(None)` when the cache file doesn't exist.
    ///
    /// Any validation/parsing error is surfaced via `Err` so callers can fall back to the truth log.
    pub(crate) fn try_replay(&self, continuity_id: &str) -> io::Result<Option<Vec<Event>>> {
        let path = self.path_for(continuity_id);
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };

        let reader = BufReader::new(file);
        let mut events = Vec::new();
        let mut expected_seq: u64 = 0;

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
                    "continuity sidecar contains non-continuity event",
                ));
            }
            if event.seq != expected_seq {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "continuity sidecar seq mismatch: expected {expected_seq}, got {}",
                        event.seq
                    ),
                ));
            }

            expected_seq = expected_seq.saturating_add(1);
            events.push(event);
        }

        if events.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "continuity sidecar is empty",
            ));
        }

        Ok(Some(events))
    }

    /// Returns `Ok(None)` when the cache file doesn't exist.
    pub(crate) fn try_read_last_seq(&self, continuity_id: &str) -> io::Result<Option<u64>> {
        self.try_read_last_seq_for_sidecar_path(continuity_id, &self.path_for(continuity_id))
    }

    fn try_read_last_seq_messages_runs_v1(&self, continuity_id: &str) -> io::Result<Option<u64>> {
        self.try_read_last_seq_for_sidecar_path(
            continuity_id,
            &self.messages_runs_path_for_v1(continuity_id),
        )
    }

    fn try_read_last_message_appended_messages_runs_v1(
        &self,
        continuity_id: &str,
    ) -> io::Result<Option<(u64, String)>> {
        let sidecar_path = match self.ensure_messages_runs_sidecar_best_effort_v1(continuity_id)? {
            Some(path) => path,
            None => return Ok(None),
        };

        const INITIAL_BACKSCAN_BYTES: usize = 64 * 1024;
        const MAX_BACKSCAN_BYTES: usize = 4 * 1024 * 1024;
        const MAX_BACKSCAN_EVENTS: usize = 10_000;

        let mut backscan_bytes = INITIAL_BACKSCAN_BYTES;
        loop {
            let mut file = File::open(&sidecar_path)?;
            let parsed = scan_sidecar_backwards(
                &mut file,
                continuity_id,
                MAX_BACKSCAN_EVENTS,
                backscan_bytes,
                ParseMode::Header,
                None,
            )?;

            for header in parsed.headers {
                if header.event_type == "continuity_message_appended" {
                    return Ok(Some((header.seq, header.id)));
                }
            }

            if parsed.complete {
                return Ok(None);
            }
            if backscan_bytes >= MAX_BACKSCAN_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "messages+runs sidecar tail scan exceeded max bytes while locating last message",
                ));
            }
            backscan_bytes = (backscan_bytes * 2).min(MAX_BACKSCAN_BYTES);
        }
    }

    /// Returns `Ok(None)` when the cache file doesn't exist and cannot be built from the full
    /// continuity sidecar.
    pub(crate) fn latest_compaction_checkpoint_before_or_at_seq_v1(
        &self,
        continuity_id: &str,
        max_to_seq: u64,
    ) -> io::Result<Option<Event>> {
        const MAX_BACKSCAN_BYTES: usize = 8 * 1024 * 1024;
        const MAX_BACKSCAN_EVENTS: usize = 10_000;

        let sidecar_path =
            match self.ensure_compaction_checkpoints_sidecar_best_effort_v1(continuity_id)? {
                Some(path) => path,
                None => return Ok(None),
            };

        let mut file = File::open(&sidecar_path)?;
        let parsed = scan_sidecar_backwards(
            &mut file,
            continuity_id,
            MAX_BACKSCAN_EVENTS,
            MAX_BACKSCAN_BYTES,
            ParseMode::Event,
            None,
        )?;

        let mut best: Option<Event> = None;
        for event in parsed.events {
            let EventKind::ContinuityCompactionCheckpointCreated { to_seq, .. } = &event.kind
            else {
                continue;
            };
            if *to_seq > max_to_seq {
                continue;
            }

            best = match best.take() {
                None => Some(event),
                Some(current) => {
                    let current_to_seq = match &current.kind {
                        EventKind::ContinuityCompactionCheckpointCreated { to_seq, .. } => *to_seq,
                        _ => 0,
                    };
                    if *to_seq > current_to_seq
                        || (*to_seq == current_to_seq && event.seq > current.seq)
                    {
                        Some(event)
                    } else {
                        Some(current)
                    }
                }
            };
        }

        Ok(best)
    }

    /// Returns `Ok(None)` when the cache index doesn't exist and cannot be built from sidecars.
    pub(crate) fn hierarchical_compaction_checkpoints_before_or_at_seq_v1(
        &self,
        continuity_id: &str,
        max_to_seq: u64,
        max_levels: usize,
        summary_kind: Option<&str>,
    ) -> io::Result<Option<Vec<CompactionCheckpointIndexEntryV1>>> {
        if max_levels == 0 {
            return Ok(Some(Vec::new()));
        }

        let index_path =
            match self.ensure_compaction_checkpoints_index_best_effort_v1(continuity_id)? {
                Some(path) => path,
                None => return Ok(None),
            };

        let mut entries = match load_compaction_checkpoint_index_v1(&index_path) {
            Ok(Some(entries)) => entries,
            Ok(None) => return Ok(None),
            Err(_) => {
                let sidecar_path = match self
                    .ensure_compaction_checkpoints_sidecar_best_effort_v1(continuity_id)?
                {
                    Some(path) => path,
                    None => return Ok(None),
                };
                let _ = rebuild_compaction_checkpoint_index_from_sidecar_v1(
                    &sidecar_path,
                    &index_path,
                    continuity_id,
                );
                load_compaction_checkpoint_index_v1(&index_path)?.unwrap_or_default()
            }
        };

        entries.retain(|entry| entry.to_seq <= max_to_seq);
        if let Some(kind) = summary_kind {
            entries.retain(|entry| entry.summary_kind == kind);
        }
        if entries.is_empty() {
            return Ok(Some(Vec::new()));
        }

        let mut latest_by_to_seq: HashMap<u64, CompactionCheckpointIndexEntryV1> = HashMap::new();
        for entry in entries {
            match latest_by_to_seq.get(&entry.to_seq) {
                Some(existing) if existing.seq >= entry.seq => {}
                _ => {
                    latest_by_to_seq.insert(entry.to_seq, entry);
                }
            }
        }

        let mut unique: Vec<CompactionCheckpointIndexEntryV1> =
            latest_by_to_seq.into_values().collect();
        unique.sort_by(|a, b| a.to_seq.cmp(&b.to_seq).then(a.seq.cmp(&b.seq)));

        let Some(latest) = unique.last().cloned() else {
            return Ok(Some(Vec::new()));
        };

        let mut selected: Vec<CompactionCheckpointIndexEntryV1> = vec![latest.clone()];
        let mut current_to_seq = latest.to_seq;

        while selected.len() < max_levels {
            if current_to_seq <= 1 {
                break;
            }
            let threshold = current_to_seq / 2;
            if threshold == 0 {
                break;
            }

            let idx = match unique.binary_search_by(|entry| entry.to_seq.cmp(&threshold)) {
                Ok(idx) => idx,
                Err(0) => break,
                Err(idx) => idx.saturating_sub(1),
            };
            let candidate = unique.get(idx).cloned();
            let Some(candidate) = candidate else {
                break;
            };
            if candidate.to_seq >= current_to_seq {
                break;
            }
            selected.push(candidate.clone());
            current_to_seq = candidate.to_seq;
        }

        selected.sort_by(|a, b| a.to_seq.cmp(&b.to_seq));
        Ok(Some(selected))
    }

    fn try_read_last_seq_for_sidecar_path(
        &self,
        continuity_id: &str,
        sidecar_path: &Path,
    ) -> io::Result<Option<u64>> {
        let mut file = match File::open(sidecar_path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };

        // Start small and expand if the last JSONL line is larger than the initial tail window.
        let mut max_bytes: usize = REVERSE_SCAN_CHUNK_BYTES * 2;
        let max_cap: usize = 4 * 1024 * 1024;
        loop {
            let tail = scan_sidecar_backwards(
                &mut file,
                continuity_id,
                1,
                max_bytes,
                ParseMode::Header,
                None,
            )?;
            if let Some(header) = tail.headers.into_iter().next() {
                return Ok(Some(header.seq));
            }

            if tail.complete {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "continuity sidecar is empty",
                ));
            }

            if max_bytes >= max_cap {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "continuity sidecar tail scan exceeded max bytes",
                ));
            }
            max_bytes = (max_bytes * 2).min(max_cap);
        }
    }

    /// Reads a bounded tail of the continuity sidecar by scanning backwards from the end.
    ///
    /// Returns `Ok(None)` when the cache file doesn't exist.
    pub(crate) fn scan_tail(
        &self,
        continuity_id: &str,
        max_events: usize,
        max_bytes: usize,
    ) -> io::Result<Option<TailScan>> {
        let path = self.path_for(continuity_id);
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };

        let parsed = scan_sidecar_backwards(
            &mut file,
            continuity_id,
            max_events,
            max_bytes,
            ParseMode::Event,
            None,
        )?;
        let mut events = parsed.events;
        events.reverse();

        if events.len() >= 2 {
            let mut expected = events[0].seq;
            for event in &events[1..] {
                expected = expected.saturating_add(1);
                if event.seq != expected {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "continuity sidecar tail contains non-contiguous seq values",
                    ));
                }
            }
        }

        Ok(Some(TailScan {
            events,
            complete: parsed.complete,
        }))
    }

    /// Reads a bounded tail of the messages+runs-only continuity sidecar.
    ///
    /// Returns `Ok(None)` when the cache file doesn't exist and cannot be built from the full
    /// continuity sidecar.
    pub(crate) fn scan_tail_messages_runs_v1(
        &self,
        continuity_id: &str,
        max_events: usize,
        max_bytes: usize,
    ) -> io::Result<Option<TailScan>> {
        let sidecar_path = match self.ensure_messages_runs_sidecar_best_effort_v1(continuity_id)? {
            Some(path) => path,
            None => return Ok(None),
        };

        let mut file = File::open(&sidecar_path)?;
        let parsed = scan_sidecar_backwards(
            &mut file,
            continuity_id,
            max_events,
            max_bytes,
            ParseMode::Event,
            None,
        )?;
        let mut events = parsed.events;
        events.reverse();

        Ok(Some(TailScan {
            events,
            complete: parsed.complete,
        }))
    }

    /// Read a bounded continuity window sufficient for `recent_messages_v1`, anchored by an
    /// existing `continuity_message_appended` event id.
    ///
    /// Returns `Ok(None)` when the sidecar/index cache is missing or doesn't contain the anchor.
    pub(crate) fn window_recent_messages_v1_from_message_id(
        &self,
        continuity_id: &str,
        anchor_message_id: &str,
        message_limit: usize,
    ) -> io::Result<Option<ContinuityWindow>> {
        if let Ok(Some(window)) = self.window_recent_messages_v1_from_message_id_messages_runs_v1(
            continuity_id,
            anchor_message_id,
            message_limit,
        ) {
            return Ok(Some(window));
        }

        self.window_recent_messages_v1_from_message_id_full_sidecar(
            continuity_id,
            anchor_message_id,
            message_limit,
        )
    }

    fn window_recent_messages_v1_from_message_id_full_sidecar(
        &self,
        continuity_id: &str,
        anchor_message_id: &str,
        message_limit: usize,
    ) -> io::Result<Option<ContinuityWindow>> {
        let sidecar_path = self.path_for(continuity_id);
        let sidecar_file = match File::open(&sidecar_path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };

        let (anchor_seq, anchor_offset) =
            match self.lookup_message_anchor_v1(continuity_id, &sidecar_path, anchor_message_id)? {
                Some(v) => v,
                None => return Ok(None),
            };

        // Determine the cut point `from_seq` as: (seq before the next message) or head_seq.
        let head_seq = self.try_read_last_seq(continuity_id)?.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "continuity sidecar empty")
        })?;

        let mut next_message_seq: Option<u64> = None;
        {
            let mut file = sidecar_file;
            file.seek(SeekFrom::Start(anchor_offset))?;
            let mut reader = BufReader::new(file);
            let mut cur_offset = anchor_offset;
            let mut saw_anchor = false;

            loop {
                let mut buf = Vec::new();
                let n = reader.read_until(b'\n', &mut buf)?;
                if n == 0 {
                    break;
                }
                cur_offset = cur_offset.saturating_add(n as u64);
                let line = strip_line_terminator(&mut buf);
                if line.is_empty() {
                    continue;
                }

                let header: SidecarEventHeader = serde_json::from_slice(line)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                if header.stream_kind != StreamKind::Continuity
                    || header.stream_id != continuity_id
                    || header.session_id != continuity_id
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "continuity sidecar contains non-continuity event",
                    ));
                }

                if !saw_anchor {
                    // Validate that the anchor offset points to the intended message.
                    if header.seq != anchor_seq
                        || header.event_type != "continuity_message_appended"
                        || header.id != anchor_message_id
                    {
                        return Ok(None);
                    }
                    saw_anchor = true;
                    continue;
                }

                if header.seq <= anchor_seq {
                    continue;
                }
                if header.event_type == "continuity_message_appended" {
                    next_message_seq = Some(header.seq);
                    break;
                }
            }
        }

        let from_seq = match next_message_seq {
            Some(seq) => seq.saturating_sub(1).max(anchor_seq),
            _ => head_seq.max(anchor_seq),
        };

        let Some(mut window) =
            self.window_recent_messages_v1_from_seq(continuity_id, from_seq, message_limit)?
        else {
            return Ok(None);
        };
        window.from_message_id = Some(anchor_message_id.to_string());
        Ok(Some(window))
    }

    fn window_recent_messages_v1_from_message_id_messages_runs_v1(
        &self,
        continuity_id: &str,
        anchor_message_id: &str,
        message_limit: usize,
    ) -> io::Result<Option<ContinuityWindow>> {
        const INITIAL_BACKSCAN_BYTES: usize = 256 * 1024;
        const MAX_BACKSCAN_BYTES: usize = 64 * 1024 * 1024;
        const MAX_BACKSCAN_EVENTS: usize = 100_000;

        let sidecar_path = match self.ensure_messages_runs_sidecar_best_effort_v1(continuity_id)? {
            Some(path) => path,
            None => return Ok(None),
        };

        let sidecar_file = File::open(&sidecar_path)?;
        let (anchor_seq, anchor_offset) = match self.lookup_message_anchor_messages_runs_v1(
            continuity_id,
            &sidecar_path,
            anchor_message_id,
        )? {
            Some(v) => v,
            None => return Ok(None),
        };

        // Determine the cut point `from_seq` as: (seq before the next message) or head_seq.
        // Use the full sidecar's head seq when available so `from_seq` matches the truth stream.
        let head_seq = self
            .try_read_last_seq(continuity_id)
            .ok()
            .flatten()
            .or_else(|| {
                self.try_read_last_seq_messages_runs_v1(continuity_id)
                    .ok()
                    .flatten()
            })
            .unwrap_or(anchor_seq);

        let mut next_message_seq: Option<u64> = None;
        let mut boundary_pos: u64 = sidecar_file.metadata()?.len();
        {
            let mut file = sidecar_file;
            file.seek(SeekFrom::Start(anchor_offset))?;
            let mut reader = BufReader::new(file);
            let mut cur_offset = anchor_offset;
            let mut saw_anchor = false;

            loop {
                let mut buf = Vec::new();
                let n = reader.read_until(b'\n', &mut buf)?;
                if n == 0 {
                    break;
                }

                let line_start = cur_offset;
                cur_offset = cur_offset.saturating_add(n as u64);
                let line = strip_line_terminator(&mut buf);
                if line.is_empty() {
                    continue;
                }

                let header: SidecarEventHeader = serde_json::from_slice(line)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                if header.stream_kind != StreamKind::Continuity
                    || header.stream_id != continuity_id
                    || header.session_id != continuity_id
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "continuity mr sidecar contains non-continuity event",
                    ));
                }

                if !saw_anchor {
                    // Validate that the anchor offset points to the intended message.
                    if header.seq != anchor_seq
                        || header.event_type != "continuity_message_appended"
                        || header.id != anchor_message_id
                    {
                        return Ok(None);
                    }
                    saw_anchor = true;
                    continue;
                }

                if header.seq <= anchor_seq {
                    continue;
                }
                if header.event_type == "continuity_message_appended" {
                    next_message_seq = Some(header.seq);
                    boundary_pos = line_start;
                    break;
                }
            }
        }

        let from_seq = match next_message_seq {
            Some(seq) => seq.saturating_sub(1).max(anchor_seq),
            _ => head_seq.max(anchor_seq),
        };

        // Backward scan from the boundary, collecting only what we need (O(k) in message density).
        let mut backscan_bytes = INITIAL_BACKSCAN_BYTES.min(MAX_BACKSCAN_BYTES);
        loop {
            let mut file = File::open(&sidecar_path)?;
            let scan = scan_sidecar_backwards(
                &mut file,
                continuity_id,
                MAX_BACKSCAN_EVENTS,
                backscan_bytes,
                ParseMode::Event,
                Some(boundary_pos),
            )?;

            let mut selected_rev: Vec<Event> = Vec::new();
            let mut found_messages = 0usize;

            for event in &scan.events {
                if event.seq > from_seq {
                    continue;
                }
                selected_rev.push(event.clone());
                if matches!(event.kind, EventKind::ContinuityMessageAppended { .. }) {
                    found_messages = found_messages.saturating_add(1);
                    if found_messages >= message_limit {
                        break;
                    }
                }
            }

            if found_messages >= message_limit
                || scan.complete
                || backscan_bytes >= MAX_BACKSCAN_BYTES
            {
                selected_rev.reverse();
                return Ok(Some(ContinuityWindow {
                    events: selected_rev,
                    from_seq,
                    from_message_id: Some(anchor_message_id.to_string()),
                }));
            }

            backscan_bytes = (backscan_bytes * 2).min(MAX_BACKSCAN_BYTES);
        }
    }

    /// Read a bounded continuity window sufficient for `recent_messages_v1`, anchored by a
    /// continuity `from_seq` cut point.
    ///
    /// Returns `Ok(None)` when the sidecar cache is missing.
    pub(crate) fn window_recent_messages_v1_from_seq(
        &self,
        continuity_id: &str,
        from_seq: u64,
        message_limit: usize,
    ) -> io::Result<Option<ContinuityWindow>> {
        let sidecar_path = self.path_for(continuity_id);
        if !sidecar_path.exists() {
            return Ok(None);
        }

        let seq_index = self.ensure_seq_index_v1(continuity_id, &sidecar_path)?;
        let boundary_pos =
            self.boundary_pos_for_seq_v1(continuity_id, &sidecar_path, &seq_index, from_seq)?;

        self.window_recent_messages_v1_from_cut_v1(
            continuity_id,
            &sidecar_path,
            from_seq,
            boundary_pos,
            message_limit,
            None,
        )
        .map(Some)
    }

    fn lookup_message_anchor_v1(
        &self,
        continuity_id: &str,
        sidecar_path: &Path,
        message_id: &str,
    ) -> io::Result<Option<(u64, u64)>> {
        let idx_path = message_index_path(&self.dir, continuity_id);

        match lookup_message_v1(&idx_path, message_id) {
            Ok(Some(found)) => Ok(Some(found)),
            Ok(None) => {
                // Missing/stale index: rebuild from sidecar (cache-only; do not touch truth log).
                let _ = rebuild_message_index_from_sidecar_v1(sidecar_path, &idx_path);
                Ok(lookup_message_v1(&idx_path, message_id)?)
            }
            Err(_) => {
                // Corrupt index: rebuild best-effort and retry.
                let _ = rebuild_message_index_from_sidecar_v1(sidecar_path, &idx_path);
                Ok(lookup_message_v1(&idx_path, message_id)?)
            }
        }
    }

    fn lookup_message_anchor_messages_runs_v1(
        &self,
        continuity_id: &str,
        sidecar_path: &Path,
        message_id: &str,
    ) -> io::Result<Option<(u64, u64)>> {
        let idx_path = self.messages_runs_message_index_path_v1(continuity_id);

        match lookup_message_v1(&idx_path, message_id) {
            Ok(Some(found)) => Ok(Some(found)),
            Ok(None) => {
                // Missing/stale index: rebuild from sidecar (cache-only; do not touch truth log).
                let _ = rebuild_message_index_from_sidecar_v1(sidecar_path, &idx_path);
                Ok(lookup_message_v1(&idx_path, message_id)?)
            }
            Err(_) => {
                // Corrupt index: rebuild best-effort and retry.
                let _ = rebuild_message_index_from_sidecar_v1(sidecar_path, &idx_path);
                Ok(lookup_message_v1(&idx_path, message_id)?)
            }
        }
    }

    fn ensure_messages_runs_sidecar_best_effort_v1(
        &self,
        continuity_id: &str,
    ) -> io::Result<Option<PathBuf>> {
        let mr_path = self.messages_runs_path_for_v1(continuity_id);
        if mr_path.exists() {
            return Ok(Some(mr_path));
        }

        let full_path = self.path_for(continuity_id);
        if !full_path.exists() {
            return Ok(None);
        }

        // Build from the full sidecar (cache-only; do not touch the truth log).
        self.rebuild_messages_runs_from_full_sidecar_best_effort_v1(
            continuity_id,
            &full_path,
            &mr_path,
        )?;
        if mr_path.exists() {
            Ok(Some(mr_path))
        } else {
            Ok(None)
        }
    }

    fn ensure_compaction_checkpoints_sidecar_best_effort_v1(
        &self,
        continuity_id: &str,
    ) -> io::Result<Option<PathBuf>> {
        let path = self.compaction_checkpoints_path_for_v1(continuity_id);
        if path.exists() {
            return Ok(Some(path));
        }

        let full_path = self.path_for(continuity_id);
        if !full_path.exists() {
            return Ok(None);
        }

        self.rebuild_compaction_checkpoints_from_full_sidecar_best_effort_v1(
            continuity_id,
            &full_path,
            &path,
        )?;
        if path.exists() {
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    fn ensure_compaction_checkpoints_index_best_effort_v1(
        &self,
        continuity_id: &str,
    ) -> io::Result<Option<PathBuf>> {
        let index_path = self.compaction_checkpoints_index_path_for_v1(continuity_id);
        if index_path.exists() {
            return Ok(Some(index_path));
        }

        let sidecar_path =
            match self.ensure_compaction_checkpoints_sidecar_best_effort_v1(continuity_id)? {
                Some(path) => path,
                None => return Ok(None),
            };

        let _ = rebuild_compaction_checkpoint_index_from_sidecar_v1(
            &sidecar_path,
            &index_path,
            continuity_id,
        );
        if index_path.exists() {
            Ok(Some(index_path))
        } else {
            Ok(None)
        }
    }

    fn rebuild_compaction_checkpoints_from_full_sidecar_best_effort_v1(
        &self,
        continuity_id: &str,
        full_sidecar_path: &Path,
        comp_sidecar_path: &Path,
    ) -> io::Result<()> {
        let tmp_sidecar = comp_sidecar_path.with_extension("jsonl.tmp");
        if let Some(parent) = tmp_sidecar.parent() {
            fs::create_dir_all(parent)?;
        }

        let full = File::open(full_sidecar_path)?;
        let mut reader = BufReader::new(full);
        let tmp_file = File::create(&tmp_sidecar)?;
        let mut writer = BufWriter::new(tmp_file);

        let mut wrote_any = false;
        loop {
            let mut buf = Vec::new();
            let n = reader.read_until(b'\n', &mut buf)?;
            if n == 0 {
                break;
            }
            let line = strip_line_terminator(&mut buf);
            if line.is_empty() {
                continue;
            }
            let header: SidecarEventHeader = serde_json::from_slice(line)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            if header.stream_kind != StreamKind::Continuity
                || header.stream_id != continuity_id
                || header.session_id != continuity_id
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "continuity sidecar contains non-continuity event while building compaction sidecar",
                ));
            }
            if header.event_type != "continuity_compaction_checkpoint_created" {
                continue;
            }

            writer.write_all(line)?;
            writer.write_all(b"\n")?;
            wrote_any = true;
        }

        writer.flush()?;
        if !wrote_any {
            let _ = fs::remove_file(tmp_sidecar);
            return Ok(());
        }

        fs::rename(tmp_sidecar, comp_sidecar_path)?;
        Ok(())
    }

    fn rebuild_messages_runs_from_full_sidecar_best_effort_v1(
        &self,
        continuity_id: &str,
        full_sidecar_path: &Path,
        mr_sidecar_path: &Path,
    ) -> io::Result<()> {
        let tmp_sidecar = mr_sidecar_path.with_extension("jsonl.tmp");
        if let Some(parent) = tmp_sidecar.parent() {
            fs::create_dir_all(parent)?;
        }

        let full = File::open(full_sidecar_path)?;
        let mut reader = BufReader::new(full);
        let tmp_file = File::create(&tmp_sidecar)?;
        let mut writer = BufWriter::new(tmp_file);

        let mut wrote_any = false;
        loop {
            let mut buf = Vec::new();
            let n = reader.read_until(b'\n', &mut buf)?;
            if n == 0 {
                break;
            }
            let line = strip_line_terminator(&mut buf);
            if line.is_empty() {
                continue;
            }
            let header: SidecarEventHeader = serde_json::from_slice(line)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            if header.stream_kind != StreamKind::Continuity
                || header.stream_id != continuity_id
                || header.session_id != continuity_id
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "continuity sidecar contains non-continuity event while building mr sidecar",
                ));
            }
            if header.event_type != "continuity_message_appended"
                && header.event_type != "continuity_run_ended"
            {
                continue;
            }

            writer.write_all(line)?;
            writer.write_all(b"\n")?;
            wrote_any = true;
        }

        writer.flush()?;
        if !wrote_any {
            let _ = fs::remove_file(tmp_sidecar);
            return Ok(());
        }

        fs::rename(tmp_sidecar, mr_sidecar_path)?;

        // Rebuild indexes for the new sidecar (cache-only; do not touch the truth log).
        let seek_path = self.messages_runs_seq_index_path_v1(continuity_id);
        rebuild_messages_runs_seek_index_best_effort_v1(mr_sidecar_path, &seek_path)?;
        let msg_path = self.messages_runs_message_index_path_v1(continuity_id);
        rebuild_message_index_from_sidecar_v1(mr_sidecar_path, &msg_path)?;
        Ok(())
    }

    fn ensure_seq_index_v1(
        &self,
        continuity_id: &str,
        sidecar_path: &Path,
    ) -> io::Result<Vec<SeqSeekIndexEntryV1>> {
        let idx_path = seq_index_path(&self.dir, continuity_id);

        let entries = match load_seq_index_v1(&idx_path) {
            Ok(Some(entries)) => entries,
            Ok(None) => {
                rebuild_seq_index_from_sidecar_v1(sidecar_path, &idx_path)?;
                load_seq_index_v1(&idx_path)?.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "seek index missing")
                })?
            }
            Err(_) => {
                rebuild_seq_index_from_sidecar_v1(sidecar_path, &idx_path)?;
                load_seq_index_v1(&idx_path)?.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "seek index missing")
                })?
            }
        };

        validate_seq_index_against_sidecar(&entries, sidecar_path, continuity_id)?;
        Ok(entries)
    }

    fn boundary_pos_for_seq_v1(
        &self,
        continuity_id: &str,
        sidecar_path: &Path,
        seq_index: &[SeqSeekIndexEntryV1],
        from_seq: u64,
    ) -> io::Result<u64> {
        let start_offset = best_offset_for_seq(seq_index, from_seq);
        let mut file = File::open(sidecar_path)?;
        let sidecar_len = file.metadata()?.len();
        file.seek(SeekFrom::Start(start_offset))?;
        let mut reader = BufReader::new(file);
        let mut cur_offset = start_offset;

        loop {
            let mut buf = Vec::new();
            let n = reader.read_until(b'\n', &mut buf)?;
            if n == 0 {
                return Ok(sidecar_len);
            }
            let line_start = cur_offset;
            cur_offset = cur_offset.saturating_add(n as u64);
            let line = strip_line_terminator(&mut buf);
            if line.is_empty() {
                continue;
            }

            let header: SidecarEventHeader = serde_json::from_slice(line)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            if header.stream_kind != StreamKind::Continuity
                || header.stream_id != continuity_id
                || header.session_id != continuity_id
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "continuity sidecar contains non-continuity event",
                ));
            }
            if header.seq > from_seq {
                return Ok(line_start);
            }
        }
    }

    fn window_recent_messages_v1_from_cut_v1(
        &self,
        continuity_id: &str,
        sidecar_path: &Path,
        from_seq: u64,
        boundary_pos: u64,
        message_limit: usize,
        from_message_id: Option<String>,
    ) -> io::Result<ContinuityWindow> {
        const INITIAL_BACKSCAN_BYTES: usize = 256 * 1024;
        const MAX_BACKSCAN_BYTES: usize = 256 * 1024 * 1024;
        const MAX_BACKSCAN_EVENTS: usize = 200_000;

        let seq_index = self.ensure_seq_index_v1(continuity_id, sidecar_path)?;

        // Backward scan (headers-only) to find the earliest seq needed to include `message_limit`
        // messages at or below `from_seq`.
        let mut start_seq: u64 = 0;
        let mut backscan_bytes = INITIAL_BACKSCAN_BYTES.min(MAX_BACKSCAN_BYTES);
        loop {
            let mut file = File::open(sidecar_path)?;
            let scan = scan_sidecar_backwards(
                &mut file,
                continuity_id,
                MAX_BACKSCAN_EVENTS,
                backscan_bytes,
                ParseMode::Header,
                Some(boundary_pos),
            )?;

            let mut found = 0usize;
            for header in &scan.headers {
                if header.seq > from_seq {
                    continue;
                }
                if header.event_type == "continuity_message_appended" {
                    found = found.saturating_add(1);
                    if found >= message_limit {
                        start_seq = header.seq;
                        break;
                    }
                }
            }

            if found >= message_limit || scan.complete || backscan_bytes >= MAX_BACKSCAN_BYTES {
                break;
            }
            backscan_bytes = (backscan_bytes * 2).min(MAX_BACKSCAN_BYTES);
        }

        // Forward scan from a nearby seek point, capturing only message + run_ended events in-range.
        let start_offset = best_offset_for_seq(&seq_index, start_seq);
        let mut file = File::open(sidecar_path)?;
        file.seek(SeekFrom::Start(start_offset))?;
        let mut reader = BufReader::new(file);
        let mut cur_offset = start_offset;

        let mut events: Vec<Event> = Vec::new();
        loop {
            if cur_offset >= boundary_pos {
                break;
            }
            let mut buf = Vec::new();
            let n = reader.read_until(b'\n', &mut buf)?;
            if n == 0 {
                break;
            }
            let line_start = cur_offset;
            cur_offset = cur_offset.saturating_add(n as u64);
            if line_start >= boundary_pos {
                break;
            }
            let line = strip_line_terminator(&mut buf);
            if line.is_empty() {
                continue;
            }

            let header: SidecarEventHeader = serde_json::from_slice(line)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            if header.stream_kind != StreamKind::Continuity
                || header.stream_id != continuity_id
                || header.session_id != continuity_id
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "continuity sidecar contains non-continuity event",
                ));
            }

            if header.seq < start_seq {
                continue;
            }
            if header.seq > from_seq {
                break;
            }

            if header.event_type == "continuity_message_appended"
                || header.event_type == "continuity_run_ended"
            {
                let event: Event = serde_json::from_slice(line)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                events.push(event);
            }
        }

        Ok(ContinuityWindow {
            events,
            from_seq,
            from_message_id,
        })
    }
}

fn rebuild_messages_runs_seek_index_best_effort_v1(
    sidecar_path: &Path,
    index_path: &Path,
) -> io::Result<()> {
    let file = File::open(sidecar_path)?;
    let mut reader = BufReader::new(file);
    let tmp = index_path.with_extension("jsonl.tmp");

    if let Some(parent) = tmp.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_file = File::create(&tmp)?;
    let mut writer = BufWriter::new(tmp_file);

    let mut offset: u64 = 0;
    let mut wrote_any = false;
    loop {
        let mut buf = Vec::new();
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }
        if buf == b"\n" || buf == b"\r\n" {
            offset = offset.saturating_add(n as u64);
            continue;
        }

        let header: SidecarEventHeader = serde_json::from_slice(&buf)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if header.stream_kind != StreamKind::Continuity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "mr sidecar contains non-continuity event while building seek index",
            ));
        }

        // The mr sidecar has sparse seq values; record entries for message events only (sufficient
        // for bounded window reads around messages).
        if header.event_type == "continuity_message_appended" {
            let entry = SeqSeekIndexEntryV1::new(header.seq, offset);
            let line = serde_json::to_string(&entry)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            writer.write_all(line.as_bytes())?;
            writer.write_all(b"\n")?;
            wrote_any = true;
        }

        offset = offset.saturating_add(n as u64);
    }

    writer.flush()?;
    if wrote_any {
        fs::rename(tmp, index_path)?;
    } else {
        let _ = fs::remove_file(tmp);
    }
    Ok(())
}

#[derive(Debug)]
struct SidecarBackwardScan {
    events: Vec<Event>,
    headers: Vec<SidecarEventHeader>,
    complete: bool,
}

#[derive(Debug, Clone, Copy)]
enum ParseMode {
    Event,
    Header,
}

fn scan_sidecar_backwards(
    file: &mut File,
    continuity_id: &str,
    max_events: usize,
    max_bytes: usize,
    mode: ParseMode,
    end_pos: Option<u64>,
) -> io::Result<SidecarBackwardScan> {
    let file_len = file.metadata()?.len();
    let end_pos = end_pos.unwrap_or(file_len).min(file_len);
    if end_pos == 0 {
        return Ok(SidecarBackwardScan {
            events: Vec::new(),
            headers: Vec::new(),
            complete: true,
        });
    }

    let mut pos = end_pos;
    let mut scanned: usize = 0;
    let mut pending: Vec<u8> = Vec::new();
    let mut events_rev: Vec<Event> = Vec::new();
    let mut headers_rev: Vec<SidecarEventHeader> = Vec::new();

    while pos > 0 && scanned < max_bytes && (events_rev.len() + headers_rev.len()) < max_events {
        let remaining = max_bytes.saturating_sub(scanned);
        if remaining == 0 {
            break;
        }

        let step = (pos as usize).min(REVERSE_SCAN_CHUNK_BYTES).min(remaining);
        pos = pos.saturating_sub(step as u64);
        file.seek(SeekFrom::Start(pos))?;

        let mut chunk = vec![0u8; step];
        file.read_exact(&mut chunk)?;
        scanned = scanned.saturating_add(step);

        if pending.is_empty() {
            pending = chunk;
        } else {
            chunk.extend_from_slice(&pending);
            pending = chunk;
        }

        drain_sidecar_lines(
            &mut pending,
            continuity_id,
            max_events,
            mode,
            &mut events_rev,
            &mut headers_rev,
        )?;
    }

    let reached_start = pos == 0;
    if reached_start && (events_rev.len() + headers_rev.len()) < max_events && !pending.is_empty() {
        let line = strip_line_terminator(&mut pending);
        if !line.is_empty() {
            match mode {
                ParseMode::Event => {
                    let event: Event = serde_json::from_slice(line)
                        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                    if event.stream_kind() != StreamKind::Continuity
                        || event.stream_id() != continuity_id
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "continuity sidecar contains non-continuity event",
                        ));
                    }
                    events_rev.push(event);
                }
                ParseMode::Header => {
                    let header: SidecarEventHeader = serde_json::from_slice(line)
                        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                    if header.stream_kind != StreamKind::Continuity
                        || header.stream_id != continuity_id
                        || header.session_id != continuity_id
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "continuity sidecar contains non-continuity event",
                        ));
                    }
                    headers_rev.push(header);
                }
            }
        }
        pending.clear();
    }

    let truncated =
        pos > 0 || ((events_rev.len() + headers_rev.len()) == max_events && !pending.is_empty());

    Ok(SidecarBackwardScan {
        events: events_rev,
        headers: headers_rev,
        complete: reached_start && !truncated,
    })
}

fn drain_sidecar_lines(
    pending: &mut Vec<u8>,
    continuity_id: &str,
    max_events: usize,
    mode: ParseMode,
    events_rev: &mut Vec<Event>,
    headers_rev: &mut Vec<SidecarEventHeader>,
) -> io::Result<()> {
    while (events_rev.len() + headers_rev.len()) < max_events {
        let Some(nl) = pending.iter().rposition(|b| *b == b'\n') else {
            break;
        };

        let mut line = pending.split_off(nl.saturating_add(1));
        // Drop the newline separator itself.
        let _ = pending.pop();

        let line = strip_line_terminator(&mut line);
        if line.is_empty() {
            continue;
        }

        match mode {
            ParseMode::Event => {
                let event: Event = serde_json::from_slice(line)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                if event.stream_kind() != StreamKind::Continuity
                    || event.stream_id() != continuity_id
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "continuity sidecar contains non-continuity event",
                    ));
                }
                events_rev.push(event);
            }
            ParseMode::Header => {
                let header: SidecarEventHeader = serde_json::from_slice(line)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                if header.stream_kind != StreamKind::Continuity
                    || header.stream_id != continuity_id
                    || header.session_id != continuity_id
                {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "continuity sidecar contains non-continuity event",
                    ));
                }
                headers_rev.push(header);
            }
        }
    }
    Ok(())
}

fn strip_line_terminator(buf: &mut Vec<u8>) -> &[u8] {
    while let Some(last) = buf.last() {
        match last {
            b'\n' => {
                buf.pop();
            }
            b'\r' => {
                buf.pop();
            }
            _ => break,
        }
    }
    buf.as_slice()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rip_kernel::EventKind;
    use tempfile::tempdir;

    fn continuity_event(continuity_id: &str, seq: u64, kind: EventKind) -> Event {
        Event {
            id: format!("e{seq}"),
            session_id: continuity_id.to_string(),
            timestamp_ms: 0,
            seq,
            kind,
        }
    }

    #[test]
    fn try_read_last_seq_reads_last_sidecar_line() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c1";

        cache.append_best_effort(&continuity_event(
            cid,
            0,
            EventKind::ContinuityCreated {
                workspace: "w".to_string(),
                title: None,
            },
        ));
        cache.append_best_effort(&continuity_event(
            cid,
            1,
            EventKind::ContinuityMessageAppended {
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
                content: "hello".to_string(),
            },
        ));
        cache.append_best_effort(&continuity_event(
            cid,
            2,
            EventKind::ContinuityMessageAppended {
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
                content: "world".to_string(),
            },
        ));

        let last = cache.try_read_last_seq(cid).expect("last seq");
        assert_eq!(last, Some(2));
    }

    #[test]
    fn scan_tail_reports_completeness_and_respects_max_events() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c2";

        for seq in 0..6 {
            cache.append_best_effort(&continuity_event(
                cid,
                seq,
                EventKind::ContinuityMessageAppended {
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                    content: format!("m{seq}"),
                },
            ));
        }

        let tail = cache
            .scan_tail(cid, 2, 64 * 1024)
            .expect("tail")
            .expect("present");
        assert_eq!(tail.events.len(), 2);
        assert_eq!(tail.events[0].seq, 4);
        assert_eq!(tail.events[1].seq, 5);
        assert!(!tail.complete, "expected truncated tail");

        let all = cache
            .scan_tail(cid, 64, 64 * 1024)
            .expect("tail")
            .expect("present");
        assert_eq!(all.events.len(), 6);
        assert_eq!(all.events[0].seq, 0);
        assert_eq!(all.events[5].seq, 5);
        assert!(all.complete, "expected full read");
    }

    fn continuity_event_with_id(continuity_id: &str, seq: u64, id: &str, kind: EventKind) -> Event {
        Event {
            id: id.to_string(),
            session_id: continuity_id.to_string(),
            timestamp_ms: 0,
            seq,
            kind,
        }
    }

    fn message_event(continuity_id: &str, seq: u64, id: &str) -> Event {
        continuity_event_with_id(
            continuity_id,
            seq,
            id,
            EventKind::ContinuityMessageAppended {
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
                content: format!("m{seq}"),
            },
        )
    }

    fn run_ended_event(continuity_id: &str, seq: u64) -> Event {
        continuity_event_with_id(
            continuity_id,
            seq,
            &format!("run-{seq}"),
            EventKind::ContinuityRunEnded {
                run_session_id: format!("run-{seq}"),
                message_id: format!("m{seq}"),
                reason: "done".to_string(),
                actor_id: None,
                origin: None,
            },
        )
    }

    fn checkpoint_event(
        continuity_id: &str,
        seq: u64,
        checkpoint_id: &str,
        to_seq: u64,
        summary_kind: &str,
    ) -> Event {
        continuity_event_with_id(
            continuity_id,
            seq,
            checkpoint_id,
            EventKind::ContinuityCompactionCheckpointCreated {
                checkpoint_id: checkpoint_id.to_string(),
                cut_rule_id: "stride_messages_v1".to_string(),
                summary_kind: summary_kind.to_string(),
                summary_artifact_id: format!("artifact-{seq}"),
                from_seq: seq.saturating_sub(1),
                from_message_id: Some(format!("m{}", seq.saturating_sub(1))),
                to_seq,
                to_message_id: Some(format!("m{to_seq}")),
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
    }

    #[test]
    fn rebuild_best_effort_supports_replay_message_counts_and_windows() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-window";
        let id1 = "00000000-0000-0000-0000-000000000001";
        let id2 = "00000000-0000-0000-0000-000000000002";
        let id3 = "00000000-0000-0000-0000-000000000003";

        let events = vec![
            continuity_event(
                cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: Some("thread".to_string()),
                },
            ),
            message_event(cid, 1, id1),
            continuity_event_with_id(
                cid,
                2,
                "tool-2",
                EventKind::ContinuityToolSideEffects {
                    run_session_id: "run-1".to_string(),
                    tool_id: "tool-2".to_string(),
                    tool_name: "write".to_string(),
                    affected_paths: Some(vec!["a.txt".to_string()]),
                    checkpoint_id: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            ),
            run_ended_event(cid, 3),
            message_event(cid, 4, id2),
            continuity_event_with_id(
                cid,
                5,
                "tool-5",
                EventKind::ContinuityToolSideEffects {
                    run_session_id: "run-2".to_string(),
                    tool_id: "tool-5".to_string(),
                    tool_name: "edit".to_string(),
                    affected_paths: None,
                    checkpoint_id: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            ),
            message_event(cid, 6, id3),
        ];

        cache.rebuild_best_effort(cid, &events);

        let replay = cache.try_replay(cid).expect("replay").expect("sidecar");
        assert_eq!(replay.len(), 7);
        assert_eq!(replay[6].seq, 6);

        assert_eq!(
            cache.message_count_messages_runs_v1(cid).expect("count"),
            Some(3)
        );
        assert_eq!(
            cache
                .message_by_ordinal_messages_runs_v1(cid, 2)
                .expect("ordinal")
                .expect("message"),
            (4, id2.to_string())
        );
        assert!(cache
            .message_by_ordinal_messages_runs_v1(cid, 4)
            .expect("ordinal")
            .is_none());

        let tail = cache
            .scan_tail_messages_runs_v1(cid, 10, 64 * 1024)
            .expect("tail")
            .expect("present");
        assert_eq!(
            tail.events
                .iter()
                .map(|event| event.seq)
                .collect::<Vec<_>>(),
            vec![1, 3, 4, 6]
        );
        assert!(tail.complete);

        let from_seq = cache
            .window_recent_messages_v1_from_seq(cid, 6, 2)
            .expect("window")
            .expect("present");
        assert_eq!(from_seq.from_seq, 6);
        assert_eq!(
            from_seq
                .events
                .iter()
                .map(|event| event.seq)
                .collect::<Vec<_>>(),
            vec![4, 6]
        );

        let from_message = cache
            .window_recent_messages_v1_from_message_id(cid, id2, 2)
            .expect("window")
            .expect("present");
        assert_eq!(from_message.from_seq, 5);
        assert_eq!(from_message.from_message_id.as_deref(), Some(id2));
        assert_eq!(
            from_message
                .events
                .iter()
                .map(|event| event.seq)
                .collect::<Vec<_>>(),
            vec![1, 3, 4]
        );
    }

    #[test]
    fn missing_messages_runs_sidecar_and_corrupt_indexes_rebuild_from_full_sidecar() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-mr";
        let id1 = "00000000-0000-0000-0000-000000000010";
        let id2 = "00000000-0000-0000-0000-000000000011";

        cache.rebuild_best_effort(
            cid,
            &[
                continuity_event(
                    cid,
                    0,
                    EventKind::ContinuityCreated {
                        workspace: "w".to_string(),
                        title: None,
                    },
                ),
                message_event(cid, 1, id1),
                run_ended_event(cid, 2),
                message_event(cid, 3, id2),
            ],
        );

        std::fs::remove_file(cache.messages_runs_path_for_v1(cid)).expect("remove mr");
        std::fs::remove_file(cache.messages_runs_seq_index_path_v1(cid)).expect("remove seek");
        std::fs::remove_file(cache.messages_runs_message_index_path_v1(cid)).expect("remove idx");

        assert_eq!(
            cache.message_count_messages_runs_v1(cid).expect("count"),
            Some(2)
        );
        assert!(cache.messages_runs_path_for_v1(cid).exists());
        assert!(cache.messages_runs_seq_index_path_v1(cid).exists());
        assert!(cache.messages_runs_message_index_path_v1(cid).exists());

        std::fs::write(cache.messages_runs_message_index_path_v1(cid), b"bad")
            .expect("corrupt idx");
        assert_eq!(
            cache
                .message_by_ordinal_messages_runs_v1(cid, 2)
                .expect("ordinal")
                .expect("message"),
            (3, id2.to_string())
        );

        std::fs::write(cache.messages_runs_seq_index_path_v1(cid), b"bad").expect("corrupt seek");
        let window = cache
            .window_recent_messages_v1_from_seq(cid, 3, 2)
            .expect("window")
            .expect("present");
        assert_eq!(
            window
                .events
                .iter()
                .map(|event| event.seq)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn checkpoint_queries_rebuild_sidecars_and_choose_latest_entries() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-comp";
        cache.rebuild_best_effort(
            cid,
            &[
                continuity_event(
                    cid,
                    0,
                    EventKind::ContinuityCreated {
                        workspace: "w".to_string(),
                        title: None,
                    },
                ),
                checkpoint_event(cid, 1, "ckpt-1", 1, "cumulative_v1"),
                message_event(cid, 2, "00000000-0000-0000-0000-000000000020"),
                checkpoint_event(cid, 3, "ckpt-2", 3, "cumulative_v1"),
                checkpoint_event(cid, 4, "ckpt-3", 3, "cumulative_v1"),
                checkpoint_event(cid, 5, "ckpt-4", 5, "other_v1"),
                checkpoint_event(cid, 6, "ckpt-5", 6, "cumulative_v1"),
            ],
        );

        std::fs::remove_file(cache.compaction_checkpoints_path_for_v1(cid))
            .expect("remove sidecar");
        std::fs::remove_file(cache.compaction_checkpoints_index_path_for_v1(cid))
            .expect("remove index");

        let latest = cache
            .latest_compaction_checkpoint_before_or_at_seq_v1(cid, 3)
            .expect("latest")
            .expect("checkpoint");
        let EventKind::ContinuityCompactionCheckpointCreated {
            checkpoint_id,
            to_seq,
            ..
        } = latest.kind
        else {
            panic!("expected checkpoint");
        };
        assert_eq!(checkpoint_id, "ckpt-3");
        assert_eq!(to_seq, 3);

        let hierarchy = cache
            .hierarchical_compaction_checkpoints_before_or_at_seq_v1(
                cid,
                6,
                3,
                Some("cumulative_v1"),
            )
            .expect("hierarchy")
            .expect("present");
        assert_eq!(
            hierarchy
                .iter()
                .map(|entry| entry.checkpoint_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ckpt-1", "ckpt-3", "ckpt-5"]
        );
        assert!(cache
            .hierarchical_compaction_checkpoints_before_or_at_seq_v1(cid, 6, 0, None)
            .expect("hierarchy")
            .expect("present")
            .is_empty());
    }

    #[test]
    fn message_validations_surface_missing_sidecars_and_drifted_ordinals() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-ordinal";
        let id1 = "00000000-0000-0000-0000-000000000040";
        let id2 = "00000000-0000-0000-0000-000000000041";
        let bogus_id = "00000000-0000-0000-0000-000000000042";

        cache.rebuild_best_effort(
            cid,
            &[
                continuity_event(
                    cid,
                    0,
                    EventKind::ContinuityCreated {
                        workspace: "w".to_string(),
                        title: None,
                    },
                ),
                message_event(cid, 1, id1),
                run_ended_event(cid, 2),
                message_event(cid, 3, id2),
            ],
        );

        let ord_path = cache.messages_runs_message_ordinal_index_path_v1(cid);
        append_message_record_best_effort_v1(&ord_path, 99, bogus_id);

        let err = cache
            .message_count_messages_runs_v1(cid)
            .expect_err("drifted ordinal index");
        assert!(err.to_string().contains("out of sync"));

        let err = cache
            .message_by_ordinal_messages_runs_v1(cid, 3)
            .expect_err("missing message");
        assert!(err.to_string().contains("references missing message"));

        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-missing-sidecar";
        cache.rebuild_best_effort(
            cid,
            &[
                continuity_event(
                    cid,
                    0,
                    EventKind::ContinuityCreated {
                        workspace: "w".to_string(),
                        title: None,
                    },
                ),
                message_event(cid, 1, "00000000-0000-0000-0000-000000000043"),
            ],
        );

        std::fs::remove_file(cache.messages_runs_path_for_v1(cid)).expect("remove mr");
        std::fs::remove_file(cache.path_for(cid)).expect("remove full");

        let err = cache
            .message_count_messages_runs_v1(cid)
            .expect_err("missing mr sidecar");
        assert!(err.to_string().contains("cannot be validated"));
    }

    #[test]
    fn window_lookup_paths_rebuild_indexes_and_fall_back_to_available_sidecars() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-full-window";
        let id1 = "00000000-0000-0000-0000-000000000050";
        let id2 = "00000000-0000-0000-0000-000000000051";
        let id3 = "00000000-0000-0000-0000-000000000052";

        cache.rebuild_best_effort(
            cid,
            &[
                continuity_event(
                    cid,
                    0,
                    EventKind::ContinuityCreated {
                        workspace: "w".to_string(),
                        title: Some("thread".to_string()),
                    },
                ),
                message_event(cid, 1, id1),
                run_ended_event(cid, 2),
                message_event(cid, 3, id2),
                continuity_event_with_id(
                    cid,
                    4,
                    "tool-4",
                    EventKind::ContinuityToolSideEffects {
                        run_session_id: "run-2".to_string(),
                        tool_id: "tool-4".to_string(),
                        tool_name: "edit".to_string(),
                        affected_paths: None,
                        checkpoint_id: None,
                        actor_id: "user".to_string(),
                        origin: "cli".to_string(),
                    },
                ),
                message_event(cid, 5, id3),
            ],
        );

        std::fs::remove_file(message_index_path(&cache.dir, cid)).expect("remove full message idx");
        let window = cache
            .window_recent_messages_v1_from_message_id_full_sidecar(cid, id2, 2)
            .expect("window")
            .expect("present");
        assert_eq!(window.from_seq, 4);
        assert_eq!(window.from_message_id.as_deref(), Some(id2));
        assert_eq!(
            window
                .events
                .iter()
                .map(|event| event.seq)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );

        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-mr-head";
        let id1 = "00000000-0000-0000-0000-000000000053";
        let id2 = "00000000-0000-0000-0000-000000000054";
        cache.rebuild_best_effort(
            cid,
            &[
                continuity_event(
                    cid,
                    0,
                    EventKind::ContinuityCreated {
                        workspace: "w".to_string(),
                        title: None,
                    },
                ),
                message_event(cid, 1, id1),
                run_ended_event(cid, 2),
                message_event(cid, 3, id2),
            ],
        );

        std::fs::remove_file(cache.path_for(cid)).expect("remove full sidecar");
        let window = cache
            .window_recent_messages_v1_from_message_id_messages_runs_v1(cid, id2, 2)
            .expect("window")
            .expect("present");
        assert_eq!(window.from_seq, 3);
        assert_eq!(window.from_message_id.as_deref(), Some(id2));
        assert_eq!(
            window
                .events
                .iter()
                .map(|event| event.seq)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn sidecar_builders_and_tail_readers_surface_invalid_inputs() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-invalid-build";

        let session_event = Event {
            id: "session-0".to_string(),
            session_id: "run-1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        };
        std::fs::create_dir_all(&cache.dir).expect("dir");
        std::fs::write(
            cache.path_for(cid),
            format!("{}\n", serde_json::to_string(&session_event).expect("json")),
        )
        .expect("write");

        let err = cache
            .rebuild_messages_runs_from_full_sidecar_best_effort_v1(
                cid,
                &cache.path_for(cid),
                &cache.messages_runs_path_for_v1(cid),
            )
            .expect_err("invalid mr rebuild");
        assert!(err.to_string().contains("while building mr sidecar"));

        let err = cache
            .rebuild_compaction_checkpoints_from_full_sidecar_best_effort_v1(
                cid,
                &cache.path_for(cid),
                &cache.compaction_checkpoints_path_for_v1(cid),
            )
            .expect_err("invalid comp rebuild");
        assert!(err
            .to_string()
            .contains("while building compaction sidecar"));

        let empty_sidecar = cache.path_for("c-empty");
        std::fs::write(&empty_sidecar, "").expect("write empty");
        let err = cache
            .try_read_last_seq_for_sidecar_path("c-empty", &empty_sidecar)
            .expect_err("empty sidecar");
        assert!(err.to_string().contains("is empty"));

        let mr_sidecar = cache.messages_runs_path_for_v1("c-mr-index");
        std::fs::write(
            &mr_sidecar,
            format!("{}\n", serde_json::to_string(&session_event).expect("json")),
        )
        .expect("write mr");
        let err = rebuild_messages_runs_seek_index_best_effort_v1(
            &mr_sidecar,
            &cache.messages_runs_seq_index_path_v1("c-mr-index"),
        )
        .expect_err("invalid mr seek");
        assert!(err.to_string().contains("non-continuity event"));
    }

    #[test]
    fn compaction_index_rebuilds_from_corruption_and_absence_is_explicit() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-comp-rebuild";
        cache.rebuild_best_effort(
            cid,
            &[
                continuity_event(
                    cid,
                    0,
                    EventKind::ContinuityCreated {
                        workspace: "w".to_string(),
                        title: None,
                    },
                ),
                checkpoint_event(cid, 1, "ckpt-1", 1, "cumulative_v1"),
                checkpoint_event(cid, 2, "ckpt-2", 2, "cumulative_v1"),
                checkpoint_event(cid, 3, "ckpt-3", 2, "cumulative_v1"),
            ],
        );

        std::fs::write(cache.compaction_checkpoints_index_path_for_v1(cid), b"bad")
            .expect("corrupt index");
        let entries = cache
            .hierarchical_compaction_checkpoints_before_or_at_seq_v1(
                cid,
                2,
                2,
                Some("cumulative_v1"),
            )
            .expect("hierarchy")
            .expect("present");
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.checkpoint_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ckpt-1", "ckpt-3"]
        );

        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-no-checkpoints";
        cache.rebuild_best_effort(
            cid,
            &[
                continuity_event(
                    cid,
                    0,
                    EventKind::ContinuityCreated {
                        workspace: "w".to_string(),
                        title: None,
                    },
                ),
                message_event(cid, 1, "00000000-0000-0000-0000-000000000060"),
            ],
        );

        assert!(cache
            .ensure_compaction_checkpoints_sidecar_best_effort_v1(cid)
            .expect("sidecar")
            .is_none());
        assert!(cache
            .ensure_compaction_checkpoints_index_best_effort_v1(cid)
            .expect("index")
            .is_none());
    }

    #[test]
    fn invalid_sidecars_surface_replay_and_tail_errors() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-bad";

        std::fs::create_dir_all(cache.dir.clone()).expect("dir");
        std::fs::write(cache.path_for(cid), "\n\n").expect("write");
        let err = cache.try_replay(cid).expect_err("empty sidecar");
        assert!(err.to_string().contains("is empty"));

        let session_event = Event {
            id: "s0".to_string(),
            session_id: "run-1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        };
        std::fs::write(
            cache.path_for(cid),
            format!("{}\n", serde_json::to_string(&session_event).expect("json")),
        )
        .expect("write");
        let err = cache.try_replay(cid).expect_err("non continuity");
        assert!(err.to_string().contains("non-continuity"));

        std::fs::write(
            cache.path_for(cid),
            format!(
                "{}\n{}\n",
                serde_json::to_string(&message_event(
                    cid,
                    0,
                    "00000000-0000-0000-0000-000000000030",
                ))
                .expect("json"),
                serde_json::to_string(&message_event(
                    cid,
                    2,
                    "00000000-0000-0000-0000-000000000031",
                ))
                .expect("json"),
            ),
        )
        .expect("write");
        let err = cache.scan_tail(cid, 10, 64 * 1024).expect_err("gap");
        assert!(err.to_string().contains("non-contiguous"));
    }

    #[test]
    fn rebuild_helpers_leave_no_derived_files_when_no_cacheable_events_exist() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-empty-derived";

        cache.rebuild_best_effort(
            cid,
            &[continuity_event(
                cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: None,
                },
            )],
        );

        assert!(
            cache.path_for(cid).exists(),
            "full sidecar should still exist"
        );
        assert!(!cache.messages_runs_path_for_v1(cid).exists());
        assert!(!cache.messages_runs_seq_index_path_v1(cid).exists());
        assert!(!cache.messages_runs_message_index_path_v1(cid).exists());
        assert!(!cache
            .messages_runs_message_ordinal_index_path_v1(cid)
            .exists());
        assert!(!cache.compaction_checkpoints_path_for_v1(cid).exists());
        assert!(!cache.compaction_checkpoints_index_path_for_v1(cid).exists());
        assert!(cache
            .ensure_messages_runs_sidecar_best_effort_v1(cid)
            .expect("ensure mr")
            .is_none());
        assert!(cache
            .ensure_compaction_checkpoints_sidecar_best_effort_v1(cid)
            .expect("ensure comp")
            .is_none());
        assert!(cache
            .ensure_compaction_checkpoints_index_best_effort_v1(cid)
            .expect("ensure comp idx")
            .is_none());
        assert!(cache
            .scan_tail_messages_runs_v1(cid, 8, 64 * 1024)
            .expect("scan mr tail")
            .is_none());

        fs::create_dir_all(&cache.dir).expect("cache dir");
        let runs_only_cid = "c-runs-only";
        let runs_only_sidecar = cache.messages_runs_path_for_v1(runs_only_cid);
        fs::write(
            &runs_only_sidecar,
            format!(
                "{}\n",
                serde_json::to_string(&run_ended_event(runs_only_cid, 1)).expect("json")
            ),
        )
        .expect("write mr sidecar");
        let runs_only_index = cache.messages_runs_seq_index_path_v1(runs_only_cid);
        rebuild_messages_runs_seek_index_best_effort_v1(&runs_only_sidecar, &runs_only_index)
            .expect("rebuild mr seek");
        assert!(
            !runs_only_index.exists(),
            "run-only sidecars should not emit message seeks"
        );
    }

    #[test]
    fn backward_scan_helpers_cover_partial_lines_and_non_continuity_inputs() {
        let dir = tempdir().expect("tmp");
        let path = dir.path().join("scan.jsonl");
        let continuity_id = "c-scan";

        fs::write(&path, "").expect("write empty");
        let mut empty = File::open(&path).expect("open empty");
        let empty_scan = scan_sidecar_backwards(
            &mut empty,
            continuity_id,
            8,
            64 * 1024,
            ParseMode::Header,
            Some(0),
        )
        .expect("empty scan");
        assert!(empty_scan.complete);
        assert!(empty_scan.headers.is_empty());

        let message_json = serde_json::to_string(&message_event(
            continuity_id,
            0,
            "00000000-0000-0000-0000-000000000070",
        ))
        .expect("json");
        fs::write(&path, &message_json).expect("write partial line");
        let mut partial = File::open(&path).expect("open partial");
        let header_scan = scan_sidecar_backwards(
            &mut partial,
            continuity_id,
            8,
            64 * 1024,
            ParseMode::Header,
            None,
        )
        .expect("header scan");
        assert_eq!(header_scan.headers.len(), 1);
        assert!(header_scan.complete);

        let mut partial = File::open(&path).expect("open partial");
        let event_scan = scan_sidecar_backwards(
            &mut partial,
            continuity_id,
            8,
            64 * 1024,
            ParseMode::Event,
            None,
        )
        .expect("event scan");
        assert_eq!(event_scan.events.len(), 1);
        assert!(event_scan.complete);

        let session_event = Event {
            id: "session-0".to_string(),
            session_id: "run-1".to_string(),
            timestamp_ms: 0,
            seq: 1,
            kind: EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        };
        let session_json = serde_json::to_string(&session_event).expect("json");
        fs::write(&path, &session_json).expect("write session");
        let mut invalid = File::open(&path).expect("open invalid");
        let err = scan_sidecar_backwards(
            &mut invalid,
            continuity_id,
            8,
            64 * 1024,
            ParseMode::Event,
            None,
        )
        .expect_err("event mismatch");
        assert!(err.to_string().contains("non-continuity"));

        let mut invalid = File::open(&path).expect("open invalid");
        let err = scan_sidecar_backwards(
            &mut invalid,
            continuity_id,
            8,
            64 * 1024,
            ParseMode::Header,
            None,
        )
        .expect_err("header mismatch");
        assert!(err.to_string().contains("non-continuity"));

        let mut pending = format!("{}\n{session_json}\n", message_json).into_bytes();
        let err = drain_sidecar_lines(
            &mut pending,
            continuity_id,
            8,
            ParseMode::Event,
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .expect_err("drain event");
        assert!(err.to_string().contains("non-continuity"));

        let mut pending = format!("{}\n{session_json}\n", message_json).into_bytes();
        let err = drain_sidecar_lines(
            &mut pending,
            continuity_id,
            8,
            ParseMode::Header,
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .expect_err("drain header");
        assert!(err.to_string().contains("non-continuity"));
    }

    #[test]
    fn full_sidecar_helpers_rebuild_missing_indexes_and_surface_edge_cases() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-full-helpers";
        let id1 = "00000000-0000-0000-0000-000000000071";
        let id2 = "00000000-0000-0000-0000-000000000072";

        cache.rebuild_best_effort(
            cid,
            &[
                continuity_event(
                    cid,
                    0,
                    EventKind::ContinuityCreated {
                        workspace: "w".to_string(),
                        title: None,
                    },
                ),
                message_event(cid, 1, id1),
                run_ended_event(cid, 2),
                message_event(cid, 3, id2),
            ],
        );

        let full_seek = seq_index_path(&cache.dir, cid);
        fs::remove_file(&full_seek).expect("remove full seek");
        let rebuilt = cache
            .window_recent_messages_v1_from_seq(cid, 3, 2)
            .expect("window")
            .expect("present");
        assert_eq!(
            rebuilt
                .events
                .iter()
                .map(|event| event.seq)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );

        fs::write(&full_seek, b"bad").expect("corrupt full seek");
        let rebuilt = cache
            .window_recent_messages_v1_from_seq(cid, 3, 2)
            .expect("window")
            .expect("present");
        assert_eq!(
            rebuilt
                .events
                .iter()
                .map(|event| event.seq)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );

        let full_sidecar = cache.path_for(cid);
        let seq_entries = cache
            .ensure_seq_index_v1(cid, &full_sidecar)
            .expect("ensure seq");
        let boundary = cache
            .boundary_pos_for_seq_v1(cid, &full_sidecar, &seq_entries, 3)
            .expect("boundary");
        assert_eq!(boundary, fs::metadata(&full_sidecar).expect("meta").len());

        assert!(cache
            .window_recent_messages_v1_from_message_id_full_sidecar(
                cid,
                "00000000-0000-0000-0000-000000000073",
                2,
            )
            .expect("missing anchor")
            .is_none());

        let replay_gap_cid = "c-replay-gap";
        fs::write(
            cache.path_for(replay_gap_cid),
            format!(
                "{}\n{}\n",
                serde_json::to_string(&message_event(
                    replay_gap_cid,
                    0,
                    "00000000-0000-0000-0000-000000000074",
                ))
                .expect("json"),
                serde_json::to_string(&message_event(
                    replay_gap_cid,
                    2,
                    "00000000-0000-0000-0000-000000000075",
                ))
                .expect("json"),
            ),
        )
        .expect("write replay gap");
        let err = cache.try_replay(replay_gap_cid).expect_err("replay gap");
        assert!(err.to_string().contains("seq mismatch"));

        fs::remove_file(cache.path_for(cid)).expect("remove full sidecar");
        assert!(cache
            .window_recent_messages_v1_from_seq(cid, 3, 2)
            .expect("missing full sidecar")
            .is_none());
    }

    #[test]
    fn anchor_window_helpers_reject_non_continuity_sidecars_in_full_and_mr_modes() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());

        let full_cid = "c-full-anchor-invalid";
        let full_id1 = "00000000-0000-0000-0000-000000000080";
        let full_id2 = "00000000-0000-0000-0000-000000000081";
        fs::create_dir_all(&cache.dir).expect("cache dir");
        fs::write(
            cache.path_for(full_cid),
            format!(
                "{}\n{}\n{}\n{}\n",
                serde_json::to_string(&continuity_event(
                    full_cid,
                    0,
                    EventKind::ContinuityCreated {
                        workspace: "w".to_string(),
                        title: None,
                    },
                ))
                .expect("json"),
                serde_json::to_string(&message_event(full_cid, 1, full_id1)).expect("json"),
                serde_json::to_string(&Event {
                    id: "session-1".to_string(),
                    session_id: "run-1".to_string(),
                    timestamp_ms: 0,
                    seq: 2,
                    kind: EventKind::SessionStarted {
                        input: "hi".to_string(),
                    },
                })
                .expect("json"),
                serde_json::to_string(&message_event(full_cid, 3, full_id2)).expect("json"),
            ),
        )
        .expect("write full sidecar");
        rebuild_message_index_from_sidecar_v1(
            &cache.path_for(full_cid),
            &message_index_path(&cache.dir, full_cid),
        )
        .expect("rebuild full index");
        let err = cache
            .window_recent_messages_v1_from_message_id_full_sidecar(full_cid, full_id1, 2)
            .expect_err("invalid full sidecar");
        assert!(err.to_string().contains("non-continuity"));

        let mr_cid = "c-mr-anchor-invalid";
        let mr_id1 = "00000000-0000-0000-0000-000000000082";
        let mr_id2 = "00000000-0000-0000-0000-000000000083";
        let mr_sidecar = cache.messages_runs_path_for_v1(mr_cid);
        fs::write(
            &mr_sidecar,
            format!(
                "{}\n{}\n{}\n",
                serde_json::to_string(&message_event(mr_cid, 1, mr_id1)).expect("json"),
                serde_json::to_string(&Event {
                    id: "session-2".to_string(),
                    session_id: "run-2".to_string(),
                    timestamp_ms: 0,
                    seq: 2,
                    kind: EventKind::SessionStarted {
                        input: "hello".to_string(),
                    },
                })
                .expect("json"),
                serde_json::to_string(&message_event(mr_cid, 3, mr_id2)).expect("json"),
            ),
        )
        .expect("write mr sidecar");
        rebuild_message_index_from_sidecar_v1(
            &mr_sidecar,
            &cache.messages_runs_message_index_path_v1(mr_cid),
        )
        .expect("rebuild mr index");
        let err = cache
            .window_recent_messages_v1_from_message_id_messages_runs_v1(mr_cid, mr_id1, 2)
            .expect_err("invalid mr sidecar");
        assert!(err.to_string().contains("non-continuity"));
    }

    #[test]
    fn message_count_messages_runs_reports_header_only_indexes_with_messages() {
        let dir = tempdir().expect("tmp");
        let cache = ContinuityStreamCache::new(dir.path());
        let cid = "c-header-only";

        cache.rebuild_best_effort(
            cid,
            &[
                continuity_event(
                    cid,
                    0,
                    EventKind::ContinuityCreated {
                        workspace: "w".to_string(),
                        title: None,
                    },
                ),
                message_event(cid, 1, "00000000-0000-0000-0000-000000000090"),
            ],
        );

        let mut header = [0u8; 32];
        header[0..8].copy_from_slice(b"RIPMORD1");
        header[8..12].copy_from_slice(&1u32.to_le_bytes());
        header[12..16].copy_from_slice(&(24u32).to_le_bytes());
        fs::write(
            cache.messages_runs_message_ordinal_index_path_v1(cid),
            header,
        )
        .expect("write header only");

        let err = cache
            .message_count_messages_runs_v1(cid)
            .expect_err("header only ordinal index");
        assert!(err
            .to_string()
            .contains("empty but messages+runs sidecar contains messages"));
    }
}
