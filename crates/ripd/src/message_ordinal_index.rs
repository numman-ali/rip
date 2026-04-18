use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use uuid::Uuid;

const MAGIC_V1: &[u8; 8] = b"RIPMORD1";
const VERSION_V1: u32 = 1;
const HEADER_SIZE_V1: u64 = 32;
const RECORD_SIZE_V1: u64 = 24; // u64 seq + 16B uuid

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MessageOrdinalRecordV1 {
    pub(crate) seq: u64,
    pub(crate) id: Uuid,
}

pub(crate) fn message_ordinal_index_path_v1(dir: &Path, continuity_id: &str) -> std::path::PathBuf {
    dir.join(format!("{continuity_id}.mr.msgord.v1.bin"))
}

pub(crate) fn message_count_v1(path: &Path) -> io::Result<Option<u64>> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let len = file.metadata()?.len();
    if len < HEADER_SIZE_V1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message ordinal index truncated",
        ));
    }
    validate_header_v1(&mut file)?;
    let data = len - HEADER_SIZE_V1;
    if !data.is_multiple_of(RECORD_SIZE_V1) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message ordinal index record alignment mismatch",
        ));
    }
    Ok(Some(data / RECORD_SIZE_V1))
}

pub(crate) fn read_message_by_ordinal_v1(
    path: &Path,
    ordinal: u64,
) -> io::Result<Option<MessageOrdinalRecordV1>> {
    if ordinal == 0 {
        return Ok(None);
    }

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let len = file.metadata()?.len();
    if len < HEADER_SIZE_V1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message ordinal index truncated",
        ));
    }
    validate_header_v1(&mut file)?;

    let count = (len - HEADER_SIZE_V1) / RECORD_SIZE_V1;
    if ordinal > count {
        return Ok(None);
    }

    let offset = HEADER_SIZE_V1 + (ordinal - 1) * RECORD_SIZE_V1;
    file.seek(SeekFrom::Start(offset))?;
    let mut buf = [0u8; RECORD_SIZE_V1 as usize];
    file.read_exact(&mut buf)?;

    let seq = u64::from_le_bytes(buf[0..8].try_into().expect("slice"));
    let mut uuid_bytes = [0u8; 16];
    uuid_bytes.copy_from_slice(&buf[8..24]);
    let id = Uuid::from_bytes(uuid_bytes);
    Ok(Some(MessageOrdinalRecordV1 { seq, id }))
}

pub(crate) fn append_message_record_best_effort_v1(path: &Path, seq: u64, id: &str) {
    let Ok(uuid) = Uuid::parse_str(id) else {
        return;
    };

    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }

    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(path)
    else {
        return;
    };

    let len = match file.metadata() {
        Ok(meta) => meta.len(),
        Err(_) => return,
    };

    if len == 0 {
        if write_header_v1(&mut file).is_err() {
            return;
        }
    } else if len < HEADER_SIZE_V1 || validate_header_v1(&mut file).is_err() {
        return;
    }

    let mut record = [0u8; RECORD_SIZE_V1 as usize];
    record[0..8].copy_from_slice(&seq.to_le_bytes());
    record[8..24].copy_from_slice(uuid.as_bytes());
    let _ = file.write_all(&record);
    let _ = file.flush();
}

pub(crate) fn rebuild_message_ordinal_index_from_events_v1(
    path: &Path,
    continuity_id: &str,
    events: &[rip_kernel::Event],
) -> io::Result<()> {
    let tmp = path.with_extension("bin.tmp");
    if let Some(parent) = tmp.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(&tmp)?;
    write_header_v1(&mut file)?;

    for event in events {
        if event.stream_kind() != rip_kernel::StreamKind::Continuity
            || event.stream_id() != continuity_id
        {
            continue;
        }
        if !matches!(
            event.kind,
            rip_kernel::EventKind::ContinuityMessageAppended { .. }
        ) {
            continue;
        }
        let id = match Uuid::parse_str(&event.id) {
            Ok(id) => id,
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "message id is not a uuid while building ordinal index",
                ));
            }
        };
        let mut record = [0u8; RECORD_SIZE_V1 as usize];
        record[0..8].copy_from_slice(&event.seq.to_le_bytes());
        record[8..24].copy_from_slice(id.as_bytes());
        file.write_all(&record)?;
    }

    file.flush()?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn write_header_v1(file: &mut File) -> io::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    file.write_all(MAGIC_V1)?;
    file.write_all(&VERSION_V1.to_le_bytes())?;
    file.write_all(&(RECORD_SIZE_V1 as u32).to_le_bytes())?;
    file.write_all(&[0u8; (HEADER_SIZE_V1 as usize).saturating_sub(8 + 4 + 4)])?;
    Ok(())
}

fn validate_header_v1(file: &mut File) -> io::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut buf = [0u8; HEADER_SIZE_V1 as usize];
    file.read_exact(&mut buf)?;
    if &buf[0..8] != MAGIC_V1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message ordinal index magic mismatch",
        ));
    }
    let version = u32::from_le_bytes(buf[8..12].try_into().expect("slice"));
    if version != VERSION_V1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message ordinal index version mismatch",
        ));
    }
    let record_size = u32::from_le_bytes(buf[12..16].try_into().expect("slice")) as u64;
    if record_size != RECORD_SIZE_V1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message ordinal index record size mismatch",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rip_kernel::EventKind;
    use tempfile::tempdir;

    fn continuity_message_event(continuity_id: &str, seq: u64, id: &str) -> rip_kernel::Event {
        rip_kernel::Event {
            id: id.to_string(),
            session_id: continuity_id.to_string(),
            timestamp_ms: 0,
            seq,
            kind: EventKind::ContinuityMessageAppended {
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
                content: format!("m{seq}"),
            },
        }
    }

    #[test]
    fn ordinal_index_surfaces_validation_and_append_failures() {
        let dir = tempdir().expect("tmp");
        let path = dir.path().join("idx.bin");
        let valid_id = Uuid::new_v4().to_string();

        assert_eq!(message_count_v1(&path).expect("missing count"), None);
        assert_eq!(
            read_message_by_ordinal_v1(&path, 1).expect("missing read"),
            None
        );
        assert_eq!(
            read_message_by_ordinal_v1(&path, 0).expect("ordinal zero"),
            None
        );

        append_message_record_best_effort_v1(&path, 1, "not-a-uuid");
        assert!(!path.exists(), "invalid uuids should not create indexes");

        let blocked_parent = dir.path().join("blocked-parent");
        fs::write(&blocked_parent, b"nope").expect("write parent file");
        append_message_record_best_effort_v1(&blocked_parent.join("idx.bin"), 1, &valid_id);
        assert!(!blocked_parent.join("idx.bin").exists());

        let dir_target = dir.path().join("dir-target");
        fs::create_dir_all(&dir_target).expect("dir target");
        append_message_record_best_effort_v1(&dir_target, 1, &valid_id);
        assert!(dir_target.is_dir(), "directory targets should be ignored");

        let truncated_path = dir.path().join("truncated.bin");
        fs::write(&truncated_path, b"short").expect("write truncated");
        let err = message_count_v1(&truncated_path).expect_err("truncated count");
        assert!(err.to_string().contains("truncated"));
        let err = read_message_by_ordinal_v1(&truncated_path, 1).expect_err("truncated read");
        assert!(err.to_string().contains("truncated"));

        let aligned_path = dir.path().join("aligned.bin");
        let mut file = File::create(&aligned_path).expect("create");
        write_header_v1(&mut file).expect("header");
        file.write_all(&[0u8; 1]).expect("extra byte");
        drop(file);
        let err = message_count_v1(&aligned_path).expect_err("alignment");
        assert!(err.to_string().contains("alignment mismatch"));

        append_message_record_best_effort_v1(&path, 10, &valid_id);
        let id2 = Uuid::new_v4().to_string();
        append_message_record_best_effort_v1(&path, 20, &id2);
        assert_eq!(message_count_v1(&path).expect("count"), Some(2));
        assert_eq!(
            read_message_by_ordinal_v1(&path, 3).expect("out of range"),
            None
        );

        let magic_path = dir.path().join("bad-magic.bin");
        let mut magic_file = File::create(&magic_path).expect("magic file");
        write_header_v1(&mut magic_file).expect("magic header");
        magic_file.seek(SeekFrom::Start(0)).expect("seek");
        magic_file.write_all(b"BADMAGIC").expect("bad magic");
        drop(magic_file);
        let err = message_count_v1(&magic_path).expect_err("magic mismatch");
        assert!(err.to_string().contains("magic mismatch"));

        let version_path = dir.path().join("bad-version.bin");
        let mut version_file = File::create(&version_path).expect("version file");
        write_header_v1(&mut version_file).expect("version header");
        version_file.seek(SeekFrom::Start(8)).expect("seek");
        version_file
            .write_all(&2u32.to_le_bytes())
            .expect("bad version");
        drop(version_file);
        let err = read_message_by_ordinal_v1(&version_path, 1).expect_err("version mismatch");
        assert!(err.to_string().contains("version mismatch"));

        let record_size_path = dir.path().join("bad-record-size.bin");
        let mut record_size_file = File::create(&record_size_path).expect("record size file");
        write_header_v1(&mut record_size_file).expect("record size header");
        record_size_file.seek(SeekFrom::Start(12)).expect("seek");
        record_size_file
            .write_all(&8u32.to_le_bytes())
            .expect("bad record size");
        drop(record_size_file);
        let err = read_message_by_ordinal_v1(&record_size_path, 1).expect_err("record size");
        assert!(err.to_string().contains("record size mismatch"));
    }

    #[test]
    fn ordinal_index_round_trips() {
        let dir = tempdir().expect("tmp");
        let path = dir.path().join("idx.bin");

        let id1 = Uuid::new_v4().to_string();
        let id2 = Uuid::new_v4().to_string();
        append_message_record_best_effort_v1(&path, 10, &id1);
        append_message_record_best_effort_v1(&path, 20, &id2);

        assert_eq!(message_count_v1(&path).expect("count"), Some(2));
        let first = read_message_by_ordinal_v1(&path, 1)
            .expect("read")
            .expect("some");
        assert_eq!(first.seq, 10);
        assert_eq!(first.id.to_string(), id1);
        let second = read_message_by_ordinal_v1(&path, 2)
            .expect("read")
            .expect("some");
        assert_eq!(second.seq, 20);
        assert_eq!(second.id.to_string(), id2);
        assert!(read_message_by_ordinal_v1(&path, 3)
            .expect("read")
            .is_none());
    }

    #[test]
    fn rebuild_message_ordinal_index_filters_streams_and_rejects_invalid_ids() {
        let dir = tempdir().expect("tmp");
        let path = dir.path().join("rebuilt.bin");
        let continuity_id = "c-ordinal";

        let invalid_events = vec![
            rip_kernel::Event {
                id: "session-0".to_string(),
                session_id: "run-1".to_string(),
                timestamp_ms: 0,
                seq: 0,
                kind: EventKind::SessionStarted {
                    input: "hi".to_string(),
                },
            },
            continuity_message_event(continuity_id, 1, "not-a-uuid"),
        ];
        let err =
            rebuild_message_ordinal_index_from_events_v1(&path, continuity_id, &invalid_events)
                .expect_err("invalid uuid");
        assert!(err
            .to_string()
            .contains("message id is not a uuid while building ordinal index"));

        let id1 = Uuid::new_v4().to_string();
        let id2 = Uuid::new_v4().to_string();
        let other_id = Uuid::new_v4().to_string();
        let events = vec![
            rip_kernel::Event {
                id: "session-1".to_string(),
                session_id: "run-2".to_string(),
                timestamp_ms: 0,
                seq: 0,
                kind: EventKind::SessionEnded {
                    reason: "done".to_string(),
                },
            },
            continuity_message_event(continuity_id, 0, &id1),
            rip_kernel::Event {
                id: "tool-2".to_string(),
                session_id: continuity_id.to_string(),
                timestamp_ms: 0,
                seq: 1,
                kind: EventKind::ContinuityToolSideEffects {
                    run_session_id: "run-1".to_string(),
                    tool_id: "tool-2".to_string(),
                    tool_name: "write".to_string(),
                    affected_paths: None,
                    checkpoint_id: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            },
            continuity_message_event("other", 0, &other_id),
            continuity_message_event(continuity_id, 2, &id2),
        ];
        rebuild_message_ordinal_index_from_events_v1(&path, continuity_id, &events)
            .expect("rebuild");

        assert_eq!(message_count_v1(&path).expect("count"), Some(2));
        let first = read_message_by_ordinal_v1(&path, 1)
            .expect("read")
            .expect("first");
        let second = read_message_by_ordinal_v1(&path, 2)
            .expect("read")
            .expect("second");
        assert_eq!(first.seq, 0);
        assert_eq!(first.id.to_string(), id1);
        assert_eq!(second.seq, 2);
        assert_eq!(second.id.to_string(), id2);
    }
}
