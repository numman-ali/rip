use super::scan::{scan_sidecar_backwards, strip_line_terminator, ParseMode};
use super::*;

impl ContinuityStreamCache {
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
        if let Ok(Some(window)) = self.window_recent_messages_v1_from_message_id_messages_runs_v1(
            continuity_id,
            anchor_message_id,
            message_limit,
        ) {
            return Ok(Some(window));
        }

        self.window_recent_messages_v1_from_message_id_full_sidecar(
            continuity_id,
            anchor_message_id,
            message_limit,
        )
    }

    pub(super) fn window_recent_messages_v1_from_message_id_full_sidecar(
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

    pub(super) fn window_recent_messages_v1_from_message_id_messages_runs_v1(
        &self,
        continuity_id: &str,
        anchor_message_id: &str,
        message_limit: usize,
    ) -> io::Result<Option<ContinuityWindow>> {
        const INITIAL_BACKSCAN_BYTES: usize = 256 * 1024;
        const MAX_BACKSCAN_BYTES: usize = 64 * 1024 * 1024;
        const MAX_BACKSCAN_EVENTS: usize = 100_000;

        let sidecar_path = match self.ensure_messages_runs_sidecar_best_effort_v1(continuity_id)? {
            Some(path) => path,
            None => return Ok(None),
        };

        let sidecar_file = File::open(&sidecar_path)?;
        let (anchor_seq, anchor_offset) = match self.lookup_message_anchor_messages_runs_v1(
            continuity_id,
            &sidecar_path,
            anchor_message_id,
        )? {
            Some(v) => v,
            None => return Ok(None),
        };

        let head_seq = self
            .try_read_last_seq(continuity_id)
            .ok()
            .flatten()
            .or_else(|| {
                self.try_read_last_seq_messages_runs_v1(continuity_id)
                    .ok()
                    .flatten()
            })
            .unwrap_or(anchor_seq);

        let mut next_message_seq: Option<u64> = None;
        let mut boundary_pos: u64 = sidecar_file.metadata()?.len();
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
                        "continuity mr sidecar contains non-continuity event",
                    ));
                }

                if !saw_anchor {
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
                    boundary_pos = line_start;
                    break;
                }
            }
        }

        let from_seq = match next_message_seq {
            Some(seq) => seq.saturating_sub(1).max(anchor_seq),
            _ => head_seq.max(anchor_seq),
        };

        let mut backscan_bytes = INITIAL_BACKSCAN_BYTES.min(MAX_BACKSCAN_BYTES);
        loop {
            let mut file = File::open(&sidecar_path)?;
            let scan = scan_sidecar_backwards(
                &mut file,
                continuity_id,
                MAX_BACKSCAN_EVENTS,
                backscan_bytes,
                ParseMode::Event,
                Some(boundary_pos),
            )?;

            let mut selected_rev: Vec<Event> = Vec::new();
            let mut found_messages = 0usize;

            for event in &scan.events {
                if event.seq > from_seq {
                    continue;
                }
                selected_rev.push(event.clone());
                if matches!(event.kind, EventKind::ContinuityMessageAppended { .. }) {
                    found_messages = found_messages.saturating_add(1);
                    if found_messages >= message_limit {
                        break;
                    }
                }
            }

            if found_messages >= message_limit
                || scan.complete
                || backscan_bytes >= MAX_BACKSCAN_BYTES
            {
                selected_rev.reverse();
                return Ok(Some(ContinuityWindow {
                    events: selected_rev,
                    from_seq,
                    from_message_id: Some(anchor_message_id.to_string()),
                }));
            }

            backscan_bytes = (backscan_bytes * 2).min(MAX_BACKSCAN_BYTES);
        }
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

    pub(super) fn lookup_message_anchor_v1(
        &self,
        continuity_id: &str,
        sidecar_path: &Path,
        message_id: &str,
    ) -> io::Result<Option<(u64, u64)>> {
        let idx_path = message_index_path(&self.dir, continuity_id);

        match lookup_message_v1(&idx_path, message_id) {
            Ok(Some(found)) => Ok(Some(found)),
            Ok(None) => {
                let _ = rebuild_message_index_from_sidecar_v1(sidecar_path, &idx_path);
                Ok(lookup_message_v1(&idx_path, message_id)?)
            }
            Err(_) => {
                let _ = rebuild_message_index_from_sidecar_v1(sidecar_path, &idx_path);
                Ok(lookup_message_v1(&idx_path, message_id)?)
            }
        }
    }

    pub(super) fn lookup_message_anchor_messages_runs_v1(
        &self,
        continuity_id: &str,
        sidecar_path: &Path,
        message_id: &str,
    ) -> io::Result<Option<(u64, u64)>> {
        let idx_path = self.messages_runs_message_index_path_v1(continuity_id);

        match lookup_message_v1(&idx_path, message_id) {
            Ok(Some(found)) => Ok(Some(found)),
            Ok(None) => {
                let _ = rebuild_message_index_from_sidecar_v1(sidecar_path, &idx_path);
                Ok(lookup_message_v1(&idx_path, message_id)?)
            }
            Err(_) => {
                let _ = rebuild_message_index_from_sidecar_v1(sidecar_path, &idx_path);
                Ok(lookup_message_v1(&idx_path, message_id)?)
            }
        }
    }

    pub(super) fn boundary_pos_for_seq_v1(
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

    pub(super) fn window_recent_messages_v1_from_cut_v1(
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
