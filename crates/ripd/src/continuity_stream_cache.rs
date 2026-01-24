use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use rip_kernel::{Event, StreamKind};
use serde::Deserialize;

use crate::continuity_seek_index::{
    append_seq_index_entry_best_effort, best_offset_for_seq, insert_message_best_effort_v1,
    load_seq_index_v1, lookup_message_v1, message_index_path,
    rebuild_message_index_from_sidecar_v1, rebuild_seq_index_from_sidecar_v1, seq_index_path,
    validate_seq_index_against_sidecar, SeqSeekIndexEntryV1, SidecarIndexBuilderV1,
    SEEK_INDEX_STRIDE_EVENTS_V1,
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
        let path = self.path_for(continuity_id);
        let mut file = match File::open(&path) {
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
}
