use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use rip_kernel::{Event, EventKind, StreamKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub(crate) const SEEK_INDEX_VERSION_V1: u32 = 1;
/// Byte offsets are recorded every N continuity events.
///
/// Keep this fairly small so window reads can start close to the desired seq without scanning
/// hundreds of KB just to reach the anchor.
pub(crate) const SEEK_INDEX_STRIDE_EVENTS_V1: u64 = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SeqSeekIndexEntryV1 {
    version: u32,
    stride: u64,
    seq: u64,
    offset: u64,
}

impl SeqSeekIndexEntryV1 {
    pub(crate) fn new(seq: u64, offset: u64) -> Self {
        Self {
            version: SEEK_INDEX_VERSION_V1,
            stride: SEEK_INDEX_STRIDE_EVENTS_V1,
            seq,
            offset,
        }
    }
}

pub(crate) fn seq_index_path(dir: &Path, continuity_id: &str) -> PathBuf {
    dir.join(format!("{continuity_id}.seek.v1.jsonl"))
}

pub(crate) fn message_index_path(dir: &Path, continuity_id: &str) -> PathBuf {
    dir.join(format!("{continuity_id}.messages.v1.bin"))
}

pub(crate) fn append_seq_index_entry_best_effort(path: &Path, entry: &SeqSeekIndexEntryV1) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }

    let Ok(file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let mut writer = BufWriter::new(file);
    let Ok(line) = serde_json::to_string(entry) else {
        return;
    };
    let _ = writer.write_all(line.as_bytes());
    let _ = writer.write_all(b"\n");
    let _ = writer.flush();
}

/// Returns `Ok(None)` when the index file doesn't exist.
///
/// Validation errors are returned as `Err` so callers can rebuild.
pub(crate) fn load_seq_index_v1(path: &Path) -> io::Result<Option<Vec<SeqSeekIndexEntryV1>>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };

    let reader = BufReader::new(file);
    let mut entries: Vec<SeqSeekIndexEntryV1> = Vec::new();
    let mut last_seq: Option<u64> = None;
    let mut last_offset: Option<u64> = None;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: SeqSeekIndexEntryV1 = serde_json::from_str(&line)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

        if entry.version != SEEK_INDEX_VERSION_V1 || entry.stride != SEEK_INDEX_STRIDE_EVENTS_V1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "seek index version mismatch",
            ));
        }

        if let Some(prev) = last_seq {
            if entry.seq < prev {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "seek index seq is not monotonic",
                ));
            }
        }
        if let Some(prev) = last_offset {
            if entry.offset < prev {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "seek index offset is not monotonic",
                ));
            }
        }

        last_seq = Some(entry.seq);
        last_offset = Some(entry.offset);
        entries.push(entry);
    }

    if entries.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "seek index is empty",
        ));
    }

    Ok(Some(entries))
}

pub(crate) fn best_offset_for_seq(entries: &[SeqSeekIndexEntryV1], target_seq: u64) -> u64 {
    // Entries are monotonic by seq. Find the last entry with seq <= target_seq.
    match entries.binary_search_by(|entry| entry.seq.cmp(&target_seq)) {
        Ok(idx) => entries[idx].offset,
        Err(0) => 0,
        Err(idx) => entries[idx.saturating_sub(1)].offset,
    }
}

pub(crate) fn validate_seq_index_against_sidecar(
    entries: &[SeqSeekIndexEntryV1],
    sidecar_path: &Path,
    continuity_id: &str,
) -> io::Result<()> {
    let mut file = File::open(sidecar_path)?;
    let sidecar_len = file.metadata()?.len();
    let Some(last) = entries.last() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "seek index empty",
        ));
    };
    if last.offset >= sidecar_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "seek index points past end of sidecar",
        ));
    }
    file.seek(SeekFrom::Start(last.offset))?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let n = reader.read_line(&mut line)?;
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "seek index offset points to EOF",
        ));
    }
    let header: SidecarSeekHeader = serde_json::from_str(&line)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    if header.stream_kind != StreamKind::Continuity
        || header.stream_id != continuity_id
        || header.session_id != continuity_id
        || header.seq != last.seq
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "seek index does not match sidecar contents",
        ));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct SidecarSeekHeader {
    seq: u64,
    session_id: String,
    stream_kind: StreamKind,
    stream_id: String,
}

pub(crate) fn rebuild_seq_index_from_sidecar_v1(
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
    let mut expected_seq: u64 = 0;
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

        let header: SidecarSeekHeader = serde_json::from_slice(&buf)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if header.seq != expected_seq {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "continuity sidecar seq mismatch while building seek index",
            ));
        }
        if header.stream_kind != StreamKind::Continuity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "continuity sidecar contains non-continuity event while building seek index",
            ));
        }

        if header.seq.is_multiple_of(SEEK_INDEX_STRIDE_EVENTS_V1) {
            let entry = SeqSeekIndexEntryV1::new(header.seq, offset);
            let line = serde_json::to_string(&entry)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            writer.write_all(line.as_bytes())?;
            writer.write_all(b"\n")?;
        }

        expected_seq = expected_seq.saturating_add(1);
        offset = offset.saturating_add(n as u64);
    }

    writer.flush()?;
    fs::rename(tmp, index_path)?;
    Ok(())
}

// ---- message_id -> (seq, offset) index (best-effort cache) ----

const MSG_INDEX_MAGIC_V1: &[u8; 8] = b"RIPMSGI1";
const MSG_INDEX_VERSION_V1: u32 = 1;
const MSG_INDEX_HEADER_SIZE: u64 = 32;
const MSG_INDEX_SLOT_SIZE: u64 = 40;

#[derive(Debug, Clone, Copy)]
struct MsgIndexHeader {
    capacity: u64,
    len: u64,
}

fn read_msg_index_header(file: &mut File) -> io::Result<MsgIndexHeader> {
    file.seek(SeekFrom::Start(0))?;
    let mut buf = [0u8; MSG_INDEX_HEADER_SIZE as usize];
    file.read_exact(&mut buf)?;
    if &buf[0..8] != MSG_INDEX_MAGIC_V1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message index magic mismatch",
        ));
    }
    let version = u32::from_le_bytes(buf[8..12].try_into().expect("version"));
    if version != MSG_INDEX_VERSION_V1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message index version mismatch",
        ));
    }
    let capacity = u64::from_le_bytes(buf[16..24].try_into().expect("capacity"));
    let len = u64::from_le_bytes(buf[24..32].try_into().expect("len"));
    if capacity == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message index capacity is zero",
        ));
    }
    Ok(MsgIndexHeader { capacity, len })
}

fn write_msg_index_header(file: &mut File, header: MsgIndexHeader) -> io::Result<()> {
    file.seek(SeekFrom::Start(0))?;
    let mut buf = [0u8; MSG_INDEX_HEADER_SIZE as usize];
    buf[0..8].copy_from_slice(MSG_INDEX_MAGIC_V1);
    buf[8..12].copy_from_slice(&MSG_INDEX_VERSION_V1.to_le_bytes());
    // [12..16] reserved
    buf[16..24].copy_from_slice(&header.capacity.to_le_bytes());
    buf[24..32].copy_from_slice(&header.len.to_le_bytes());
    file.write_all(&buf)?;
    Ok(())
}

fn msg_index_slot_offset(slot: u64) -> u64 {
    MSG_INDEX_HEADER_SIZE + slot.saturating_mul(MSG_INDEX_SLOT_SIZE)
}

fn hash_uuid_v1(key: &[u8; 16]) -> u64 {
    // FNV-1a 64-bit.
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in key {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn read_slot_state(file: &mut File, offset: u64) -> io::Result<u8> {
    file.seek(SeekFrom::Start(offset))?;
    let mut b = [0u8; 1];
    file.read_exact(&mut b)?;
    Ok(b[0])
}

fn read_slot_key(file: &mut File, offset: u64) -> io::Result<[u8; 16]> {
    file.seek(SeekFrom::Start(offset + 1))?;
    let mut key = [0u8; 16];
    file.read_exact(&mut key)?;
    Ok(key)
}

fn read_slot_payload(file: &mut File, offset: u64) -> io::Result<(u64, u64)> {
    file.seek(SeekFrom::Start(offset + 17))?;
    let mut buf = [0u8; 16];
    file.read_exact(&mut buf)?;
    let seq = u64::from_le_bytes(buf[0..8].try_into().expect("seq"));
    let line_offset = u64::from_le_bytes(buf[8..16].try_into().expect("offset"));
    Ok((seq, line_offset))
}

fn write_slot(
    file: &mut File,
    offset: u64,
    key: &[u8; 16],
    seq: u64,
    line_offset: u64,
) -> io::Result<()> {
    let mut buf = [0u8; MSG_INDEX_SLOT_SIZE as usize];
    buf[0] = 1;
    buf[1..17].copy_from_slice(key);
    buf[17..25].copy_from_slice(&seq.to_le_bytes());
    buf[25..33].copy_from_slice(&line_offset.to_le_bytes());
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&buf)?;
    Ok(())
}

fn create_empty_msg_index(path: &Path, capacity: u64) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    let header = MsgIndexHeader { capacity, len: 0 };
    write_msg_index_header(&mut file, header)?;
    file.set_len(MSG_INDEX_HEADER_SIZE + capacity.saturating_mul(MSG_INDEX_SLOT_SIZE))?;
    file.flush()?;
    Ok(())
}

fn next_power_of_two_u64(mut v: u64) -> u64 {
    if v <= 1 {
        return 1;
    }
    v = v.saturating_sub(1);
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    v |= v >> 32;
    v.saturating_add(1)
}

fn message_index_should_grow(header: MsgIndexHeader) -> bool {
    // len/capacity >= 0.7
    header.len.saturating_mul(10) >= header.capacity.saturating_mul(7)
}

pub(crate) fn lookup_message_v1(path: &Path, message_id: &str) -> io::Result<Option<(u64, u64)>> {
    let Ok(uuid) = Uuid::parse_str(message_id) else {
        return Ok(None);
    };
    let key = uuid.into_bytes();
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };

    let header = read_msg_index_header(&mut file)?;
    let cap = header.capacity;
    let mask = cap.saturating_sub(1);
    if cap == 0 || (cap & mask) != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message index capacity must be a power of two",
        ));
    }

    let mut slot = hash_uuid_v1(&key) & mask;
    for _ in 0..cap {
        let off = msg_index_slot_offset(slot);
        let state = read_slot_state(&mut file, off)?;
        if state == 0 {
            return Ok(None);
        }
        if state == 1 {
            let stored = read_slot_key(&mut file, off)?;
            if stored == key {
                let (seq, line_offset) = read_slot_payload(&mut file, off)?;
                return Ok(Some((seq, line_offset)));
            }
        }
        slot = (slot + 1) & mask;
    }
    Ok(None)
}

pub(crate) fn insert_message_best_effort_v1(
    path: &Path,
    sidecar_path: &Path,
    message_id: &str,
    seq: u64,
    line_offset: u64,
) {
    let Ok(uuid) = Uuid::parse_str(message_id) else {
        return;
    };
    let key = uuid.into_bytes();

    if !path.exists() {
        // Small-but-reasonable default; rebuilt when load factor grows too high.
        let _ = create_empty_msg_index(path, 16_384);
    }

    let mut file = match OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(_) => return,
    };

    let header = match read_msg_index_header(&mut file) {
        Ok(header) => header,
        Err(_) => {
            // Corrupt index: rebuild from sidecar.
            let _ = rebuild_message_index_from_sidecar_v1(sidecar_path, path);
            return;
        }
    };

    if message_index_should_grow(header) {
        let _ = rebuild_message_index_from_sidecar_v1(sidecar_path, path);
        return;
    }

    let cap = header.capacity;
    let mask = cap.saturating_sub(1);
    if cap == 0 || (cap & mask) != 0 {
        let _ = rebuild_message_index_from_sidecar_v1(sidecar_path, path);
        return;
    }

    let mut slot = hash_uuid_v1(&key) & mask;
    for _ in 0..cap {
        let off = msg_index_slot_offset(slot);
        let state = match read_slot_state(&mut file, off) {
            Ok(state) => state,
            Err(_) => return,
        };
        if state == 0 {
            if write_slot(&mut file, off, &key, seq, line_offset).is_err() {
                return;
            }
            let new_header = MsgIndexHeader {
                capacity: header.capacity,
                len: header.len.saturating_add(1),
            };
            let _ = write_msg_index_header(&mut file, new_header);
            let _ = file.flush();
            return;
        }
        if state == 1 {
            if let Ok(stored) = read_slot_key(&mut file, off) {
                if stored == key {
                    let _ = write_slot(&mut file, off, &key, seq, line_offset);
                    let _ = file.flush();
                    return;
                }
            }
        }
        slot = (slot + 1) & mask;
    }

    // Table is unexpectedly full; rebuild.
    let _ = rebuild_message_index_from_sidecar_v1(sidecar_path, path);
}

pub(crate) fn rebuild_message_index_from_sidecar_v1(
    sidecar_path: &Path,
    index_path: &Path,
) -> io::Result<()> {
    let count = count_messages_in_sidecar(sidecar_path)?;
    let capacity = next_power_of_two_u64(count.saturating_mul(2).max(16));
    let tmp = index_path.with_extension("bin.tmp");
    if let Some(parent) = tmp.parent() {
        fs::create_dir_all(parent)?;
    }

    {
        let mut file = File::create(&tmp)?;
        let header = MsgIndexHeader { capacity, len: 0 };
        write_msg_index_header(&mut file, header)?;
        file.set_len(MSG_INDEX_HEADER_SIZE + capacity.saturating_mul(MSG_INDEX_SLOT_SIZE))?;
    }

    let mut file = OpenOptions::new().read(true).write(true).open(&tmp)?;
    let mut header = read_msg_index_header(&mut file)?;

    let sidecar = File::open(sidecar_path)?;
    let mut reader = BufReader::new(sidecar);
    let mut offset: u64 = 0;
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

        let parsed: SidecarMessageIndexLine = serde_json::from_slice(&buf)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if parsed.ty == "continuity_message_appended" {
            if let Ok(uuid) = Uuid::parse_str(&parsed.id) {
                let key = uuid.into_bytes();
                msg_index_insert_v1(&mut file, &mut header, &key, parsed.seq, offset)?;
            }
        }

        offset = offset.saturating_add(n as u64);
    }

    write_msg_index_header(&mut file, header)?;
    file.flush()?;
    fs::rename(tmp, index_path)?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct SidecarMessageIndexLine {
    id: String,
    seq: u64,
    #[serde(rename = "type")]
    ty: String,
}

fn count_messages_in_sidecar(sidecar_path: &Path) -> io::Result<u64> {
    let sidecar = File::open(sidecar_path)?;
    let reader = BufReader::new(sidecar);
    let mut count: u64 = 0;
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: SidecarMessageCountLine = serde_json::from_str(&line)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if parsed.ty == "continuity_message_appended" {
            count = count.saturating_add(1);
        }
    }
    Ok(count)
}

#[derive(Debug, Deserialize)]
struct SidecarMessageCountLine {
    #[serde(rename = "type")]
    ty: String,
}

fn msg_index_insert_v1(
    file: &mut File,
    header: &mut MsgIndexHeader,
    key: &[u8; 16],
    seq: u64,
    line_offset: u64,
) -> io::Result<()> {
    let cap = header.capacity;
    let mask = cap.saturating_sub(1);
    if cap == 0 || (cap & mask) != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message index capacity must be a power of two",
        ));
    }

    let mut slot = hash_uuid_v1(key) & mask;
    for _ in 0..cap {
        let off = msg_index_slot_offset(slot);
        let state = read_slot_state(file, off)?;
        if state == 0 {
            write_slot(file, off, key, seq, line_offset)?;
            header.len = header.len.saturating_add(1);
            return Ok(());
        }
        if state == 1 {
            let stored = read_slot_key(file, off)?;
            if stored == *key {
                write_slot(file, off, key, seq, line_offset)?;
                return Ok(());
            }
        }
        slot = (slot + 1) & mask;
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "message index is full",
    ))
}

// ---- helper for rebuild-from-events during sidecar rebuild ----

pub(crate) struct SidecarIndexBuilderV1 {
    seq_entries: Vec<SeqSeekIndexEntryV1>,
    msg_entries: Vec<(Uuid, u64, u64)>,
}

impl SidecarIndexBuilderV1 {
    pub(crate) fn new() -> Self {
        Self {
            seq_entries: Vec::new(),
            msg_entries: Vec::new(),
        }
    }

    pub(crate) fn observe_event(&mut self, event: &Event, line_offset: u64) {
        if event.stream_kind() != StreamKind::Continuity {
            return;
        }
        if event.seq.is_multiple_of(SEEK_INDEX_STRIDE_EVENTS_V1) {
            self.seq_entries
                .push(SeqSeekIndexEntryV1::new(event.seq, line_offset));
        }
        if matches!(event.kind, EventKind::ContinuityMessageAppended { .. }) {
            if let Ok(uuid) = Uuid::parse_str(&event.id) {
                self.msg_entries.push((uuid, event.seq, line_offset));
            }
        }
    }

    pub(crate) fn write_best_effort(
        &self,
        sidecar_dir: &Path,
        continuity_id: &str,
    ) -> io::Result<()> {
        if self.seq_entries.is_empty() {
            return Ok(());
        }

        // Seq seek index (JSONL).
        let seek_path = seq_index_path(sidecar_dir, continuity_id);
        let tmp_seek = seek_path.with_extension("jsonl.tmp");
        if let Some(parent) = tmp_seek.parent() {
            fs::create_dir_all(parent)?;
        }
        {
            let file = File::create(&tmp_seek)?;
            let mut writer = BufWriter::new(file);
            for entry in &self.seq_entries {
                let line = serde_json::to_string(entry)
                    .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
                writer.write_all(line.as_bytes())?;
                writer.write_all(b"\n")?;
            }
            writer.flush()?;
        }
        fs::rename(tmp_seek, seek_path)?;

        // Message index (binary). Rebuild from observed message events.
        let msg_path = message_index_path(sidecar_dir, continuity_id);
        let tmp_msg = msg_path.with_extension("bin.tmp");
        let capacity =
            next_power_of_two_u64(self.msg_entries.len().saturating_mul(2).max(16) as u64);
        if let Some(parent) = tmp_msg.parent() {
            fs::create_dir_all(parent)?;
        }
        {
            let mut file = File::create(&tmp_msg)?;
            let header = MsgIndexHeader { capacity, len: 0 };
            write_msg_index_header(&mut file, header)?;
            file.set_len(MSG_INDEX_HEADER_SIZE + capacity.saturating_mul(MSG_INDEX_SLOT_SIZE))?;
        }
        let mut file = OpenOptions::new().read(true).write(true).open(&tmp_msg)?;
        let mut header = read_msg_index_header(&mut file)?;
        for (uuid, seq, off) in &self.msg_entries {
            let key = uuid.into_bytes();
            // Ignore collisions/full errors; this is a cache and can be rebuilt from sidecar later.
            let _ = msg_index_insert_v1(&mut file, &mut header, &key, *seq, *off);
        }
        let _ = write_msg_index_header(&mut file, header);
        let _ = file.flush();
        let _ = fs::rename(tmp_msg, msg_path);

        Ok(())
    }
}
