use super::*;

#[derive(Debug)]
pub(super) struct SidecarBackwardScan {
    pub(super) events: Vec<Event>,
    pub(super) headers: Vec<SidecarEventHeader>,
    pub(super) complete: bool,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ParseMode {
    Event,
    Header,
}

pub(super) fn scan_sidecar_backwards(
    file: &mut File,
    continuity_id: &str,
    max_events: usize,
    max_bytes: usize,
    mode: ParseMode,
    end_pos: Option<u64>,
) -> io::Result<SidecarBackwardScan> {
    let file_len = file.metadata()?.len();
    let end_pos = end_pos.unwrap_or(file_len).min(file_len);
    if end_pos == 0 {
        return Ok(SidecarBackwardScan {
            events: Vec::new(),
            headers: Vec::new(),
            complete: true,
        });
    }

    let mut pos = end_pos;
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

pub(super) fn drain_sidecar_lines(
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

pub(super) fn strip_line_terminator(buf: &mut Vec<u8>) -> &[u8] {
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
