use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use rip_kernel::{Event, StreamKind};

pub struct EventLog {
    path: PathBuf,
    writer: Mutex<BufWriter<File>>,
}

impl EventLog {
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            path,
            writer: Mutex::new(BufWriter::new(file)),
        })
    }

    pub fn append(&self, event: &Event) -> io::Result<()> {
        let mut writer = self.writer.lock().expect("event log mutex");
        let line = serde_json::to_string(event)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        Ok(())
    }

    pub fn replay(&self) -> io::Result<Vec<Event>> {
        let file = File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let event: Event = serde_json::from_str(&line)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            events.push(event);
        }
        Ok(events)
    }

    pub fn replay_validated(&self) -> io::Result<Vec<Event>> {
        let events = self.replay()?;
        validate_event_order(&events)?;
        Ok(events)
    }

    pub fn replay_stream(
        &self,
        stream_kind: StreamKind,
        stream_id: &str,
    ) -> io::Result<Vec<Event>> {
        let events = self.replay_validated()?;
        Ok(events
            .into_iter()
            .filter(|event| event.stream_kind() == stream_kind && event.stream_id() == stream_id)
            .collect())
    }

    pub fn replay_session(&self, session_id: &str) -> io::Result<Vec<Event>> {
        self.replay_stream(StreamKind::Session, session_id)
    }
}

pub fn write_snapshot(
    dir: impl AsRef<Path>,
    session_id: &str,
    events: &[Event],
) -> io::Result<PathBuf> {
    let dir = dir.as_ref();
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{session_id}.json"));
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);
    let payload = serde_json::to_string_pretty(events)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    writer.write_all(payload.as_bytes())?;
    writer.flush()?;
    Ok(path)
}

pub fn read_snapshot(path: impl AsRef<Path>) -> io::Result<Vec<Event>> {
    let file = File::open(path)?;
    serde_json::from_reader(BufReader::new(file))
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

pub fn verify_snapshot(log: &EventLog, snapshot_path: impl AsRef<Path>) -> io::Result<()> {
    let snapshot_events = read_snapshot(&snapshot_path)?;
    if snapshot_events.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "snapshot is empty",
        ));
    }

    let stream_kind = snapshot_events[0].stream_kind();
    let stream_id = snapshot_events[0].stream_id().to_string();
    if snapshot_events
        .iter()
        .any(|event| event.stream_kind() != stream_kind || event.stream_id() != stream_id)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "snapshot contains multiple stream ids",
        ));
    }

    let replayed = log.replay_stream(stream_kind, &stream_id)?;
    compare_events(&replayed, &snapshot_events)
}

fn validate_event_order(events: &[Event]) -> io::Result<()> {
    let mut expected: HashMap<(StreamKind, &str), u64> = HashMap::new();
    for event in events {
        let entry = expected
            .entry((event.stream_kind(), event.stream_id()))
            .or_insert(0);
        if event.seq != *entry {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "sequence mismatch for stream {:?}/{}: expected {}, got {}",
                    event.stream_kind(),
                    event.stream_id(),
                    entry,
                    event.seq
                ),
            ));
        }
        *entry += 1;
    }
    Ok(())
}

fn compare_events(left: &[Event], right: &[Event]) -> io::Result<()> {
    if left.len() != right.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "snapshot mismatch: expected {} events, got {}",
                right.len(),
                left.len()
            ),
        ));
    }

    for (idx, (left_event, right_event)) in left.iter().zip(right.iter()).enumerate() {
        let left_value = serde_json::to_value(left_event)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let right_value = serde_json::to_value(right_event)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if left_value != right_value {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("snapshot mismatch at index {idx}"),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rip_kernel::{EventKind, Runtime};
    use tempfile::tempdir;

    #[test]
    fn append_and_replay_events() {
        let dir = tempdir().expect("tmp");
        let log_path = dir.path().join("events.jsonl");
        let log = EventLog::new(&log_path).expect("log");

        let runtime = Runtime::new();
        let mut session = runtime.start_session("hello".to_string());
        while let Some(event) = session.next_event() {
            log.append(&event).expect("append");
        }

        let events = log.replay().expect("replay");
        assert_eq!(events.len(), 3);
        matches!(events[0].kind, EventKind::SessionStarted { .. });
    }

    #[test]
    fn event_log_creates_parent_dirs() {
        let dir = tempdir().expect("tmp");
        let log_path = dir.path().join("nested").join("events.jsonl");
        let log = EventLog::new(&log_path).expect("log");
        let runtime = Runtime::new();
        let mut session = runtime.start_session("hello".to_string());
        let event = session.next_event().expect("event");
        log.append(&event).expect("append");
        assert!(log_path.exists());
    }

    #[test]
    fn write_snapshot_creates_file() {
        let dir = tempdir().expect("tmp");
        let runtime = Runtime::new();
        let mut session = runtime.start_session("hello".to_string());
        let mut events = Vec::new();
        while let Some(event) = session.next_event() {
            events.push(event);
        }

        let path = write_snapshot(dir.path(), "session-1", &events).expect("snapshot");
        assert!(path.exists());
    }

    #[test]
    fn replay_invalid_line_returns_error() {
        let dir = tempdir().expect("tmp");
        let log_path = dir.path().join("events.jsonl");
        fs::write(&log_path, "not json\n").expect("write");
        let log = EventLog::new(&log_path).expect("log");
        let err = log.replay().expect_err("error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn replay_validated_detects_seq_gap() {
        let dir = tempdir().expect("tmp");
        let log_path = dir.path().join("events.jsonl");
        let log = EventLog::new(&log_path).expect("log");

        let event0 = Event {
            id: "e0".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        };
        let event2 = Event {
            id: "e2".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 1,
            seq: 2,
            kind: EventKind::SessionEnded {
                reason: "done".to_string(),
            },
        };

        log.append(&event0).expect("append");
        log.append(&event2).expect("append");

        let err = log.replay_validated().expect_err("error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn read_snapshot_roundtrips() {
        let dir = tempdir().expect("tmp");
        let runtime = Runtime::new();
        let mut session = runtime.start_session("hello".to_string());
        let mut events = Vec::new();
        while let Some(event) = session.next_event() {
            events.push(event);
        }
        let path = write_snapshot(dir.path(), "s1", &events).expect("snapshot");
        let loaded = read_snapshot(&path).expect("read");
        assert_eq!(loaded.len(), events.len());
    }

    #[test]
    fn replay_session_filters_by_id() {
        let dir = tempdir().expect("tmp");
        let log_path = dir.path().join("events.jsonl");
        let log = EventLog::new(&log_path).expect("log");

        let event_a = Event {
            id: "e1".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        };
        let event_b = Event {
            id: "e2".to_string(),
            session_id: "s2".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted {
                input: "yo".to_string(),
            },
        };
        log.append(&event_a).expect("append");
        log.append(&event_b).expect("append");

        let events = log.replay_session("s1").expect("replay");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_id, "s1");
    }

    #[test]
    fn validate_event_order_rejects_gap() {
        let event0 = Event {
            id: "e0".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        };
        let event2 = Event {
            id: "e2".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 1,
            seq: 2,
            kind: EventKind::SessionEnded {
                reason: "done".to_string(),
            },
        };
        let err = validate_event_order(&[event0, event2]).expect_err("error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn compare_events_rejects_length_mismatch() {
        let left = vec![Event {
            id: "e0".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        }];
        let err = compare_events(&left, &[]).expect_err("error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn verify_snapshot_matches_replay() {
        let dir = tempdir().expect("tmp");
        let log_path = dir.path().join("events.jsonl");
        let log = EventLog::new(&log_path).expect("log");

        let runtime = Runtime::new();
        let mut session = runtime.start_session("hello".to_string());
        let mut events = Vec::new();
        while let Some(event) = session.next_event() {
            log.append(&event).expect("append");
            events.push(event);
        }

        let snapshot_path = write_snapshot(dir.path(), session.id(), &events).expect("snapshot");
        verify_snapshot(&log, snapshot_path).expect("verify");
    }

    #[test]
    fn verify_snapshot_detects_mismatch() {
        let dir = tempdir().expect("tmp");
        let log_path = dir.path().join("events.jsonl");
        let log = EventLog::new(&log_path).expect("log");

        let runtime = Runtime::new();
        let mut session = runtime.start_session("hello".to_string());
        let mut events = Vec::new();
        while let Some(event) = session.next_event() {
            log.append(&event).expect("append");
            events.push(event);
        }

        let snapshot_path = write_snapshot(dir.path(), session.id(), &events).expect("snapshot");
        let mut snapshot_events = read_snapshot(&snapshot_path).expect("read");
        snapshot_events.pop();
        let payload = serde_json::to_string_pretty(&snapshot_events).expect("json");
        fs::write(&snapshot_path, payload).expect("write");

        let err = verify_snapshot(&log, snapshot_path).expect_err("error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn verify_snapshot_rejects_empty_snapshot() {
        let dir = tempdir().expect("tmp");
        let log_path = dir.path().join("events.jsonl");
        let log = EventLog::new(&log_path).expect("log");
        let snapshot_path = dir.path().join("empty.json");
        fs::write(&snapshot_path, "[]").expect("write");

        let err = verify_snapshot(&log, snapshot_path).expect_err("error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn verify_snapshot_detects_content_mismatch() {
        let dir = tempdir().expect("tmp");
        let log_path = dir.path().join("events.jsonl");
        let log = EventLog::new(&log_path).expect("log");

        let runtime = Runtime::new();
        let mut session = runtime.start_session("hello".to_string());
        let mut events = Vec::new();
        while let Some(event) = session.next_event() {
            log.append(&event).expect("append");
            events.push(event);
        }

        let snapshot_path = write_snapshot(dir.path(), session.id(), &events).expect("snapshot");
        let mut snapshot_events = read_snapshot(&snapshot_path).expect("read");
        snapshot_events[0].seq = snapshot_events[0].seq.saturating_add(1);
        let payload = serde_json::to_string_pretty(&snapshot_events).expect("json");
        fs::write(&snapshot_path, payload).expect("write");

        let err = verify_snapshot(&log, snapshot_path).expect_err("error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn snapshot_roundtrip_accepts_pathbuf() {
        let dir = tempdir().expect("tmp");
        let log_path = dir.path().join("events.jsonl");
        let log = EventLog::new(log_path.clone()).expect("log");

        let runtime = Runtime::new();
        let mut session = runtime.start_session("hello".to_string());
        let mut events = Vec::new();
        while let Some(event) = session.next_event() {
            log.append(&event).expect("append");
            events.push(event);
        }

        let snapshot_root = log_path.parent().unwrap().join("snapshots");
        let snapshot_path = write_snapshot(snapshot_root, session.id(), &events).expect("snapshot");
        let roundtrip = read_snapshot(snapshot_path.clone()).expect("read");
        assert_eq!(roundtrip.len(), events.len());
        verify_snapshot(&log, snapshot_path).expect("verify");
    }

    #[test]
    fn verify_snapshot_rejects_multiple_sessions() {
        let dir = tempdir().expect("tmp");
        let log_path = dir.path().join("events.jsonl");
        let log = EventLog::new(&log_path).expect("log");

        let first = Event {
            id: "e1".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        };
        let second = Event {
            id: "e2".to_string(),
            session_id: "s2".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted {
                input: "yo".to_string(),
            },
        };
        log.append(&first).expect("append");
        log.append(&second).expect("append");

        let snapshot_path = dir.path().join("snapshots.json");
        let payload = serde_json::to_string_pretty(&vec![first, second]).expect("json");
        fs::write(&snapshot_path, payload).expect("write");

        let err = verify_snapshot(&log, snapshot_path).expect_err("error");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
