use super::scan::{scan_sidecar_backwards, ParseMode};
use super::*;

impl ContinuityStreamCache {
    /// Returns `Ok(None)` when the ordinal index doesn't exist.
    pub(crate) fn message_count_messages_runs_v1(
        &self,
        continuity_id: &str,
    ) -> io::Result<Option<u64>> {
        let path = self.messages_runs_message_ordinal_index_path_v1(continuity_id);
        let Some(count) = message_count_v1(&path)? else {
            return Ok(None);
        };

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

    pub(super) fn try_read_last_seq_messages_runs_v1(
        &self,
        continuity_id: &str,
    ) -> io::Result<Option<u64>> {
        self.try_read_last_seq_for_sidecar_path(
            continuity_id,
            &self.messages_runs_path_for_v1(continuity_id),
        )
    }

    pub(super) fn try_read_last_message_appended_messages_runs_v1(
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
            let to_seq = match &event.kind {
                EventKind::ContinuityCompactionCheckpointCreated { to_seq, .. } => *to_seq,
                _ => continue,
            };
            if to_seq > max_to_seq {
                continue;
            }

            best = match best.take() {
                None => Some(event),
                Some(current) => {
                    let current_to_seq = match &current.kind {
                        EventKind::ContinuityCompactionCheckpointCreated { to_seq, .. } => *to_seq,
                        _ => 0,
                    };
                    if to_seq > current_to_seq
                        || (to_seq == current_to_seq && event.seq > current.seq)
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

    pub(super) fn try_read_last_seq_for_sidecar_path(
        &self,
        continuity_id: &str,
        sidecar_path: &Path,
    ) -> io::Result<Option<u64>> {
        let mut file = match File::open(sidecar_path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };

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
}
