use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use rip_kernel::Event;

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
        matches!(events[0].kind, EventKind::SessionStarted);
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
}
