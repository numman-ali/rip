use super::scan::strip_line_terminator;
use super::*;

impl ContinuityStreamCache {
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

        self.append_messages_runs_best_effort_v1(event);
        self.append_compaction_checkpoints_best_effort_v1(event);
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

        self.rebuild_messages_runs_best_effort_v1(continuity_id, events);
        self.rebuild_compaction_checkpoints_best_effort_v1(continuity_id, events);
    }

    pub(super) fn append_messages_runs_best_effort_v1(&self, event: &Event) {
        if event.stream_kind() != StreamKind::Continuity {
            return;
        }
        if !matches!(
            &event.kind,
            EventKind::ContinuityMessageAppended { .. } | EventKind::ContinuityRunEnded { .. }
        ) {
            return;
        }

        let continuity_id = event.stream_id();
        let path = self.messages_runs_path_for_v1(continuity_id);
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

        let seek_path = self.messages_runs_seq_index_path_v1(continuity_id);
        if matches!(&event.kind, EventKind::ContinuityMessageAppended { .. }) {
            append_seq_index_entry_best_effort(
                &seek_path,
                &SeqSeekIndexEntryV1::new(event.seq, offset),
            );
        }
        if matches!(&event.kind, EventKind::ContinuityMessageAppended { .. }) {
            let msg_path = self.messages_runs_message_index_path_v1(continuity_id);
            insert_message_best_effort_v1(&msg_path, &path, &event.id, event.seq, offset);
            let ord_path = self.messages_runs_message_ordinal_index_path_v1(continuity_id);
            append_message_record_best_effort_v1(&ord_path, event.seq, &event.id);
        }
    }

    pub(super) fn append_compaction_checkpoints_best_effort_v1(&self, event: &Event) {
        if event.stream_kind() != StreamKind::Continuity {
            return;
        }
        if !matches!(
            &event.kind,
            EventKind::ContinuityCompactionCheckpointCreated { .. }
        ) {
            return;
        }

        let continuity_id = event.stream_id();
        let path = self.compaction_checkpoints_path_for_v1(continuity_id);
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
        if writer.write_all(line.as_bytes()).is_err() {
            return;
        }
        if writer.write_all(b"\n").is_err() {
            return;
        }
        let _ = writer.flush();

        if let Some(entry) = CompactionCheckpointIndexEntryV1::from_event(event) {
            let idx_path = self.compaction_checkpoints_index_path_for_v1(continuity_id);
            append_compaction_checkpoint_index_entry_best_effort_v1(&idx_path, &entry);
        }
    }

    pub(super) fn rebuild_messages_runs_best_effort_v1(
        &self,
        continuity_id: &str,
        events: &[Event],
    ) {
        let path = self.messages_runs_path_for_v1(continuity_id);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let tmp_path = path.with_extension("jsonl.tmp");
        let tmp_seek = self
            .messages_runs_seq_index_path_v1(continuity_id)
            .with_extension("jsonl.tmp");

        let Ok(file) = File::create(&tmp_path) else {
            return;
        };
        let Ok(seek_file) = File::create(&tmp_seek) else {
            return;
        };

        let mut writer = BufWriter::new(file);
        let mut seek_writer = BufWriter::new(seek_file);
        let mut offset: u64 = 0;
        let mut wrote_any = false;

        for event in events {
            if event.stream_kind() != StreamKind::Continuity || event.stream_id() != continuity_id {
                continue;
            }
            if !matches!(
                &event.kind,
                EventKind::ContinuityMessageAppended { .. } | EventKind::ContinuityRunEnded { .. }
            ) {
                continue;
            }

            let Ok(line) = serde_json::to_string(event) else {
                continue;
            };

            if matches!(&event.kind, EventKind::ContinuityMessageAppended { .. }) {
                if let Ok(entry) =
                    serde_json::to_string(&SeqSeekIndexEntryV1::new(event.seq, offset))
                {
                    let _ = seek_writer.write_all(entry.as_bytes());
                    let _ = seek_writer.write_all(b"\n");
                }
            }

            let _ = writer.write_all(line.as_bytes());
            let _ = writer.write_all(b"\n");
            wrote_any = true;
            offset = offset.saturating_add(line.len() as u64 + 1);
        }

        let _ = writer.flush();
        let _ = seek_writer.flush();

        if !wrote_any {
            let _ = fs::remove_file(&tmp_path);
            let _ = fs::remove_file(&tmp_seek);
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(self.messages_runs_seq_index_path_v1(continuity_id));
            let _ = fs::remove_file(self.messages_runs_message_index_path_v1(continuity_id));
            let _ =
                fs::remove_file(self.messages_runs_message_ordinal_index_path_v1(continuity_id));
            return;
        }

        let _ = fs::rename(tmp_path, path);
        let _ = fs::rename(
            tmp_seek,
            self.messages_runs_seq_index_path_v1(continuity_id),
        );

        let msg_path = self.messages_runs_message_index_path_v1(continuity_id);
        let _ = rebuild_message_index_from_sidecar_v1(
            &self.messages_runs_path_for_v1(continuity_id),
            &msg_path,
        );

        let ord_path = self.messages_runs_message_ordinal_index_path_v1(continuity_id);
        let _ = rebuild_message_ordinal_index_from_events_v1(&ord_path, continuity_id, events);
    }

    pub(super) fn rebuild_compaction_checkpoints_best_effort_v1(
        &self,
        continuity_id: &str,
        events: &[Event],
    ) {
        let path = self.compaction_checkpoints_path_for_v1(continuity_id);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let tmp_path = path.with_extension("jsonl.tmp");
        let Ok(file) = File::create(&tmp_path) else {
            return;
        };
        let mut writer = BufWriter::new(file);

        let mut wrote_any = false;
        for event in events {
            if event.stream_kind() != StreamKind::Continuity || event.stream_id() != continuity_id {
                continue;
            }
            if !matches!(
                &event.kind,
                EventKind::ContinuityCompactionCheckpointCreated { .. }
            ) {
                continue;
            }

            let Ok(line) = serde_json::to_string(event) else {
                continue;
            };
            let _ = writer.write_all(line.as_bytes());
            let _ = writer.write_all(b"\n");
            wrote_any = true;
        }

        let _ = writer.flush();
        if !wrote_any {
            let _ = fs::remove_file(&tmp_path);
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(self.compaction_checkpoints_index_path_for_v1(continuity_id));
            return;
        }

        let _ = fs::rename(tmp_path, path);
        let _ = rebuild_compaction_checkpoint_index_from_events_v1(
            &self.compaction_checkpoints_index_path_for_v1(continuity_id),
            continuity_id,
            events,
        );
    }

    pub(super) fn ensure_messages_runs_sidecar_best_effort_v1(
        &self,
        continuity_id: &str,
    ) -> io::Result<Option<PathBuf>> {
        let mr_path = self.messages_runs_path_for_v1(continuity_id);
        if mr_path.exists() {
            return Ok(Some(mr_path));
        }

        let full_path = self.path_for(continuity_id);
        if !full_path.exists() {
            return Ok(None);
        }

        self.rebuild_messages_runs_from_full_sidecar_best_effort_v1(
            continuity_id,
            &full_path,
            &mr_path,
        )?;
        if mr_path.exists() {
            Ok(Some(mr_path))
        } else {
            Ok(None)
        }
    }

    pub(super) fn ensure_compaction_checkpoints_sidecar_best_effort_v1(
        &self,
        continuity_id: &str,
    ) -> io::Result<Option<PathBuf>> {
        let path = self.compaction_checkpoints_path_for_v1(continuity_id);
        if path.exists() {
            return Ok(Some(path));
        }

        let full_path = self.path_for(continuity_id);
        if !full_path.exists() {
            return Ok(None);
        }

        self.rebuild_compaction_checkpoints_from_full_sidecar_best_effort_v1(
            continuity_id,
            &full_path,
            &path,
        )?;
        if path.exists() {
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    pub(super) fn ensure_compaction_checkpoints_index_best_effort_v1(
        &self,
        continuity_id: &str,
    ) -> io::Result<Option<PathBuf>> {
        let index_path = self.compaction_checkpoints_index_path_for_v1(continuity_id);
        if index_path.exists() {
            return Ok(Some(index_path));
        }

        let sidecar_path =
            match self.ensure_compaction_checkpoints_sidecar_best_effort_v1(continuity_id)? {
                Some(path) => path,
                None => return Ok(None),
            };

        let _ = rebuild_compaction_checkpoint_index_from_sidecar_v1(
            &sidecar_path,
            &index_path,
            continuity_id,
        );
        if index_path.exists() {
            Ok(Some(index_path))
        } else {
            Ok(None)
        }
    }

    pub(super) fn rebuild_compaction_checkpoints_from_full_sidecar_best_effort_v1(
        &self,
        continuity_id: &str,
        full_sidecar_path: &Path,
        comp_sidecar_path: &Path,
    ) -> io::Result<()> {
        let tmp_sidecar = comp_sidecar_path.with_extension("jsonl.tmp");
        if let Some(parent) = tmp_sidecar.parent() {
            fs::create_dir_all(parent)?;
        }

        let full = File::open(full_sidecar_path)?;
        let mut reader = BufReader::new(full);
        let tmp_file = File::create(&tmp_sidecar)?;
        let mut writer = BufWriter::new(tmp_file);

        let mut wrote_any = false;
        loop {
            let mut buf = Vec::new();
            let n = reader.read_until(b'\n', &mut buf)?;
            if n == 0 {
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
                    "continuity sidecar contains non-continuity event while building compaction sidecar",
                ));
            }
            if header.event_type != "continuity_compaction_checkpoint_created" {
                continue;
            }

            writer.write_all(line)?;
            writer.write_all(b"\n")?;
            wrote_any = true;
        }

        writer.flush()?;
        if !wrote_any {
            let _ = fs::remove_file(tmp_sidecar);
            return Ok(());
        }

        fs::rename(tmp_sidecar, comp_sidecar_path)?;
        Ok(())
    }

    pub(super) fn rebuild_messages_runs_from_full_sidecar_best_effort_v1(
        &self,
        continuity_id: &str,
        full_sidecar_path: &Path,
        mr_sidecar_path: &Path,
    ) -> io::Result<()> {
        let tmp_sidecar = mr_sidecar_path.with_extension("jsonl.tmp");
        if let Some(parent) = tmp_sidecar.parent() {
            fs::create_dir_all(parent)?;
        }

        let full = File::open(full_sidecar_path)?;
        let mut reader = BufReader::new(full);
        let tmp_file = File::create(&tmp_sidecar)?;
        let mut writer = BufWriter::new(tmp_file);

        let mut wrote_any = false;
        loop {
            let mut buf = Vec::new();
            let n = reader.read_until(b'\n', &mut buf)?;
            if n == 0 {
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
                    "continuity sidecar contains non-continuity event while building mr sidecar",
                ));
            }
            if header.event_type != "continuity_message_appended"
                && header.event_type != "continuity_run_ended"
            {
                continue;
            }

            writer.write_all(line)?;
            writer.write_all(b"\n")?;
            wrote_any = true;
        }

        writer.flush()?;
        if !wrote_any {
            let _ = fs::remove_file(tmp_sidecar);
            return Ok(());
        }

        fs::rename(tmp_sidecar, mr_sidecar_path)?;

        let seek_path = self.messages_runs_seq_index_path_v1(continuity_id);
        rebuild_messages_runs_seek_index_best_effort_v1(mr_sidecar_path, &seek_path)?;
        let msg_path = self.messages_runs_message_index_path_v1(continuity_id);
        rebuild_message_index_from_sidecar_v1(mr_sidecar_path, &msg_path)?;
        Ok(())
    }

    pub(super) fn ensure_seq_index_v1(
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
}

pub(super) fn rebuild_messages_runs_seek_index_best_effort_v1(
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
    let mut wrote_any = false;
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

        let header: SidecarEventHeader = serde_json::from_slice(&buf)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if header.stream_kind != StreamKind::Continuity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "mr sidecar contains non-continuity event while building seek index",
            ));
        }

        if header.event_type == "continuity_message_appended" {
            let entry = SeqSeekIndexEntryV1::new(header.seq, offset);
            let line = serde_json::to_string(&entry)
                .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
            writer.write_all(line.as_bytes())?;
            writer.write_all(b"\n")?;
            wrote_any = true;
        }

        offset = offset.saturating_add(n as u64);
    }

    writer.flush()?;
    if wrote_any {
        fs::rename(tmp, index_path)?;
    } else {
        let _ = fs::remove_file(tmp);
    }
    Ok(())
}
