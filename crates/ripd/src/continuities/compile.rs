use super::*;

impl ContinuityStore {
    pub(crate) fn load_context_compile_input_recent_messages_v1(
        &self,
        continuity_id: &str,
        anchor_message_id: &str,
    ) -> Result<ContextCompileInput, String> {
        const INITIAL_TAIL_BYTES: usize = 256 * 1024;
        const MAX_TAIL_BYTES: usize = 8 * 1024 * 1024;
        const MAX_TAIL_EVENTS: usize = 100_000;

        let mut tail_bytes = INITIAL_TAIL_BYTES;
        while tail_bytes <= MAX_TAIL_BYTES {
            match self.stream_cache.scan_tail_messages_runs_v1(
                continuity_id,
                MAX_TAIL_EVENTS,
                tail_bytes,
            ) {
                Ok(Some(tail)) => {
                    if !tail.events.is_empty() {
                        let head_seq = self
                            .stream_cache
                            .try_read_last_seq(continuity_id)
                            .ok()
                            .flatten()
                            .or_else(|| tail.events.last().map(|event| event.seq))
                            .unwrap_or_default();

                        let mut message_events: Vec<(u64, String)> = Vec::new();
                        for event in &tail.events {
                            if matches!(event.kind, EventKind::ContinuityMessageAppended { .. }) {
                                message_events.push((event.seq, event.id.clone()));
                            }
                        }

                        if let Some((message_seq, from_seq)) =
                            resolve_cutpoint_from_tail(&message_events, head_seq, anchor_message_id)
                        {
                            let message_count = message_events
                                .iter()
                                .filter(|(seq, _)| *seq <= from_seq)
                                .count();

                            if tail.complete || message_count >= RECENT_MESSAGES_V1_LIMIT {
                                return Ok(ContextCompileInput {
                                    continuity_events: tail.events,
                                    from_seq: from_seq.max(message_seq),
                                    from_message_id: Some(anchor_message_id.to_string()),
                                });
                            }
                        } else if tail.complete {
                            return Err(format!(
                                "continuity message not found: {anchor_message_id}"
                            ));
                        }
                    } else if tail.complete {
                        return Err("continuity sidecar is empty".to_string());
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }

            if tail_bytes >= MAX_TAIL_BYTES {
                break;
            }
            tail_bytes = (tail_bytes * 2).min(MAX_TAIL_BYTES);
        }

        if let Ok(Some(window)) = self.stream_cache.window_recent_messages_v1_from_message_id(
            continuity_id,
            anchor_message_id,
            RECENT_MESSAGES_V1_LIMIT,
        ) {
            return Ok(ContextCompileInput {
                continuity_events: window.events,
                from_seq: window.from_seq,
                from_message_id: window.from_message_id,
            });
        }

        let continuity_events = self
            .replay_events(continuity_id)
            .map_err(|err| format!("continuity replay failed: {err}"))?;
        if continuity_events.is_empty() {
            return Err("continuity stream does not exist".to_string());
        }

        let (from_seq, from_message_id) =
            resolve_context_compile_cutpoint_full(&continuity_events, anchor_message_id)?;
        Ok(ContextCompileInput {
            continuity_events,
            from_seq,
            from_message_id,
        })
    }

    pub(crate) fn latest_compaction_checkpoint_for_compile_v1(
        &self,
        continuity_id: &str,
        from_seq: u64,
    ) -> Result<Option<CompactionCheckpointForCompile>, String> {
        if let Ok(Some(event)) = self
            .stream_cache
            .latest_compaction_checkpoint_before_or_at_seq_v1(continuity_id, from_seq)
        {
            if let EventKind::ContinuityCompactionCheckpointCreated {
                checkpoint_id,
                summary_kind,
                summary_artifact_id,
                to_seq,
                ..
            } = event.kind
            {
                return Ok(Some(CompactionCheckpointForCompile {
                    checkpoint_id,
                    summary_kind,
                    summary_artifact_id,
                    to_seq,
                }));
            }
        }

        let events = self
            .replay_events(continuity_id)
            .map_err(|err| format!("continuity replay failed: {err}"))?;

        let mut best: Option<CompactionCheckpointForCompile> = None;
        for event in &events {
            let EventKind::ContinuityCompactionCheckpointCreated {
                checkpoint_id,
                summary_kind,
                summary_artifact_id,
                to_seq,
                ..
            } = &event.kind
            else {
                continue;
            };
            if *to_seq > from_seq {
                continue;
            }

            let replace = match best.as_ref() {
                None => true,
                Some(current) => *to_seq > current.to_seq,
            };
            if replace {
                best = Some(CompactionCheckpointForCompile {
                    checkpoint_id: checkpoint_id.clone(),
                    summary_kind: summary_kind.clone(),
                    summary_artifact_id: summary_artifact_id.clone(),
                    to_seq: *to_seq,
                });
            } else if let Some(current) = best.as_mut() {
                if *to_seq == current.to_seq {
                    current.checkpoint_id = checkpoint_id.clone();
                    current.summary_kind = summary_kind.clone();
                    current.summary_artifact_id = summary_artifact_id.clone();
                }
            }
        }

        Ok(best)
    }

    pub(crate) fn hierarchical_compaction_checkpoints_for_compile_v1(
        &self,
        continuity_id: &str,
        from_seq: u64,
        max_levels: usize,
    ) -> Result<Vec<CompactionCheckpointForCompile>, String> {
        if max_levels == 0 {
            return Ok(Vec::new());
        }

        if let Ok(Some(entries)) = self
            .stream_cache
            .hierarchical_compaction_checkpoints_before_or_at_seq_v1(
                continuity_id,
                from_seq,
                max_levels,
                Some(COMPACTION_SUMMARY_KIND_CUMULATIVE_V1),
            )
        {
            let mut out: Vec<CompactionCheckpointForCompile> = entries
                .into_iter()
                .map(|entry| CompactionCheckpointForCompile {
                    checkpoint_id: entry.checkpoint_id,
                    summary_kind: entry.summary_kind,
                    summary_artifact_id: entry.summary_artifact_id,
                    to_seq: entry.to_seq,
                })
                .collect();
            out.sort_by(|a, b| a.to_seq.cmp(&b.to_seq));
            return Ok(out);
        }

        let events = self
            .replay_events(continuity_id)
            .map_err(|err| format!("continuity replay failed: {err}"))?;
        if events.is_empty() {
            return Ok(Vec::new());
        }

        let mut latest_by_to_seq: HashMap<u64, (u64, CompactionCheckpointForCompile)> =
            HashMap::new();
        for event in &events {
            let EventKind::ContinuityCompactionCheckpointCreated {
                checkpoint_id,
                summary_kind,
                summary_artifact_id,
                to_seq,
                ..
            } = &event.kind
            else {
                continue;
            };
            if *to_seq > from_seq || summary_kind != COMPACTION_SUMMARY_KIND_CUMULATIVE_V1 {
                continue;
            }

            let record = CompactionCheckpointForCompile {
                checkpoint_id: checkpoint_id.clone(),
                summary_kind: summary_kind.clone(),
                summary_artifact_id: summary_artifact_id.clone(),
                to_seq: *to_seq,
            };

            match latest_by_to_seq.get(to_seq) {
                Some((existing_seq, _)) if *existing_seq >= event.seq => {}
                _ => {
                    latest_by_to_seq.insert(*to_seq, (event.seq, record));
                }
            }
        }

        let mut unique: Vec<CompactionCheckpointForCompile> = latest_by_to_seq
            .into_values()
            .map(|(_, record)| record)
            .collect();
        unique.sort_by(|a, b| a.to_seq.cmp(&b.to_seq));

        let Some(latest) = unique.last().cloned() else {
            return Ok(Vec::new());
        };

        let mut selected: Vec<CompactionCheckpointForCompile> = vec![latest.clone()];
        let mut current_to_seq = latest.to_seq;
        while selected.len() < max_levels {
            if current_to_seq <= 1 {
                break;
            }
            let threshold = current_to_seq / 2;
            if threshold == 0 {
                break;
            }
            let idx = match unique.binary_search_by(|entry| entry.to_seq.cmp(&threshold)) {
                Ok(idx) => idx,
                Err(0) => break,
                Err(idx) => idx.saturating_sub(1),
            };
            let Some(candidate) = unique.get(idx).cloned() else {
                break;
            };
            if candidate.to_seq >= current_to_seq {
                break;
            }
            selected.push(candidate.clone());
            current_to_seq = candidate.to_seq;
        }
        selected.sort_by(|a, b| a.to_seq.cmp(&b.to_seq));
        Ok(selected)
    }
}

fn resolve_cutpoint_from_tail(
    message_events: &[(u64, String)],
    head_seq: u64,
    anchor_message_id: &str,
) -> Option<(u64, u64)> {
    let anchor_idx = message_events
        .iter()
        .position(|(_, id)| id == anchor_message_id)?;
    let message_seq = message_events.get(anchor_idx).map(|(seq, _)| *seq)?;
    let next_message_seq = message_events.get(anchor_idx + 1).map(|(seq, _)| *seq);
    let from_seq = match next_message_seq {
        Some(next_seq) => next_seq.saturating_sub(1),
        None => head_seq,
    };
    Some((message_seq, from_seq))
}

pub(super) fn resolve_context_compile_cutpoint_full(
    continuity_events: &[Event],
    message_id: &str,
) -> Result<(u64, Option<String>), String> {
    let head_seq = continuity_events
        .last()
        .map(|event| event.seq)
        .unwrap_or_default();

    let mut message_seq: Option<u64> = None;
    let mut next_message_seq: Option<u64> = None;

    for event in continuity_events {
        if !matches!(event.kind, EventKind::ContinuityMessageAppended { .. }) {
            continue;
        }

        if message_seq.is_none() {
            if event.id == message_id {
                message_seq = Some(event.seq);
            }
            continue;
        }

        next_message_seq = Some(event.seq);
        break;
    }

    let Some(message_seq) = message_seq else {
        return Err(format!("continuity message not found: {message_id}"));
    };

    let from_seq = match next_message_seq {
        Some(next_seq) => next_seq.saturating_sub(1),
        None => head_seq,
    };

    Ok((from_seq.max(message_seq), Some(message_id.to_string())))
}
