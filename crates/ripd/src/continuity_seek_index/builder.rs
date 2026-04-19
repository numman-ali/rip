use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;

use rip_kernel::{Event, EventKind, StreamKind};
use uuid::Uuid;

use super::{
    message_index_path, msg_index_insert_v1, next_power_of_two_u64, read_msg_index_header,
    seq_index_path, write_msg_index_header, MsgIndexHeader, SeqSeekIndexEntryV1,
    MSG_INDEX_HEADER_SIZE, MSG_INDEX_SLOT_SIZE, SEEK_INDEX_STRIDE_EVENTS_V1,
};

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
