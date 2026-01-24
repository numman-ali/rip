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
    use tempfile::tempdir;

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
}
