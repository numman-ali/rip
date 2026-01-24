use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use rip_kernel::{Event, StreamKind};
use serde::Deserialize;

const REVERSE_SCAN_CHUNK_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct TailScan {
    pub(crate) events: Vec<Event>,
    pub(crate) complete: bool,
}

#[derive(Debug, Deserialize)]
struct SidecarEventHeader {
    seq: u64,
    session_id: String,
    stream_kind: StreamKind,
    stream_id: String,
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
        let mut writer = BufWriter::new(file);
        let Ok(line) = serde_json::to_string(event) else {
            return;
        };
        let _ = writer.write_all(line.as_bytes());
        let _ = writer.write_all(b"\n");
        let _ = writer.flush();
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
        for event in events {
            if event.stream_kind() != StreamKind::Continuity || event.stream_id() != continuity_id {
                continue;
            }
            let Ok(line) = serde_json::to_string(event) else {
                continue;
            };
            let _ = writer.write_all(line.as_bytes());
            let _ = writer.write_all(b"\n");
        }
        let _ = writer.flush();
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
            let tail =
                scan_sidecar_backwards(&mut file, continuity_id, 1, max_bytes, ParseMode::Header)?;
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
) -> io::Result<SidecarBackwardScan> {
    let file_len = file.metadata()?.len();
    if file_len == 0 {
        return Ok(SidecarBackwardScan {
            events: Vec::new(),
            headers: Vec::new(),
            complete: true,
        });
    }

    let mut pos = file_len;
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
