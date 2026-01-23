use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use rip_kernel::{Event, StreamKind};

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
}
