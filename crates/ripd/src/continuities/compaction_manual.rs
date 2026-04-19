use super::*;

impl ContinuityStore {
    pub fn compaction_checkpoint_cumulative_v1(
        &self,
        thread_id: &str,
        req: CompactionCheckpointCumulativeV1Request,
    ) -> Result<(String, String, u64, String, String), String> {
        let summary_markdown = req.summary_markdown;
        let summary_artifact_id = req.summary_artifact_id;
        let to_message_id = req.to_message_id;
        let to_seq = req.to_seq;
        let stride_messages = req.stride_messages;
        let actor_id = req.actor_id;
        let origin = req.origin;

        if summary_markdown.is_none() && summary_artifact_id.is_none() {
            return Err(
                "compaction checkpoint requires summary_markdown and/or summary_artifact_id"
                    .to_string(),
            );
        }
        if to_message_id.is_some() && to_seq.is_some() {
            return Err(
                "compaction checkpoint requires only one of to_message_id or to_seq".to_string(),
            );
        }

        let events = self
            .replay_events(thread_id)
            .map_err(|err| format!("compaction thread replay failed: {err}"))?;
        if events.is_empty() {
            return Err("compaction thread continuity stream does not exist".to_string());
        }

        let message_events: Vec<(u64, String)> = events
            .iter()
            .filter_map(|event| match &event.kind {
                EventKind::ContinuityMessageAppended { .. } => Some((event.seq, event.id.clone())),
                _ => None,
            })
            .collect();
        if message_events.is_empty() {
            return Err("compaction requires at least one message in the thread".to_string());
        }

        let (to_seq, to_message_id, cut_rule_id) = if let Some(message_id) = to_message_id.clone() {
            let Some((seq, _)) = message_events.iter().find(|(_, id)| id == &message_id) else {
                return Err(format!("compaction to_message_id not found: {message_id}"));
            };
            (*seq, message_id, "manual_v1".to_string())
        } else if let Some(to_seq) = to_seq {
            let Some((_, message_id)) = message_events.iter().find(|(seq, _)| *seq == to_seq)
            else {
                return Err(format!(
                    "compaction to_seq must be a message boundary: seq={to_seq}"
                ));
            };
            (to_seq, message_id.clone(), "manual_v1".to_string())
        } else {
            let stride = stride_messages.unwrap_or(10_000);
            if stride == 0 {
                return Err("compaction stride_messages must be > 0".to_string());
            }
            let message_count = message_events.len() as u64;
            let target = (message_count / stride) * stride;
            if target == 0 {
                return Err(format!(
                    "compaction stride_messages not reached: stride={stride}, messages={message_count}"
                ));
            }
            let idx = (target - 1) as usize;
            let (seq, message_id) = message_events
                .get(idx)
                .ok_or_else(|| "compaction stride cutpoint out of range".to_string())?;
            (
                *seq,
                message_id.clone(),
                format!("stride_messages_v1/{stride}"),
            )
        };

        let mut base_summary_artifact_id: Option<String> = None;
        let mut best_checkpoint_to_seq: u64 = 0;
        let mut best_checkpoint_event_seq: u64 = 0;
        for event in &events {
            let EventKind::ContinuityCompactionCheckpointCreated {
                summary_kind,
                summary_artifact_id,
                to_seq: checkpoint_to_seq,
                ..
            } = &event.kind
            else {
                continue;
            };
            if summary_kind != COMPACTION_SUMMARY_KIND_CUMULATIVE_V1 {
                continue;
            }
            if *checkpoint_to_seq >= to_seq {
                continue;
            }
            if *checkpoint_to_seq > best_checkpoint_to_seq
                || (*checkpoint_to_seq == best_checkpoint_to_seq
                    && event.seq > best_checkpoint_event_seq)
            {
                best_checkpoint_to_seq = *checkpoint_to_seq;
                best_checkpoint_event_seq = event.seq;
                base_summary_artifact_id = Some(summary_artifact_id.clone());
            }
        }

        let summary_artifact_id = if let Some(artifact_id) = summary_artifact_id {
            let summary = read_compaction_summary_v1(&self.workspace_root, &artifact_id)?;
            if summary.schema() != COMPACTION_SUMMARY_SCHEMA_V1 {
                return Err(format!(
                    "compaction summary schema mismatch: expected {}, got {}",
                    COMPACTION_SUMMARY_SCHEMA_V1,
                    summary.schema()
                ));
            }
            if summary.kind() != COMPACTION_SUMMARY_KIND_CUMULATIVE_V1 {
                return Err(format!(
                    "compaction summary kind mismatch: expected {}, got {}",
                    COMPACTION_SUMMARY_KIND_CUMULATIVE_V1,
                    summary.kind()
                ));
            }
            if summary.coverage_thread_id() != thread_id {
                return Err("compaction summary thread_id mismatch".to_string());
            }
            if summary.coverage_to_seq() != to_seq {
                return Err("compaction summary to_seq mismatch".to_string());
            }
            artifact_id
        } else {
            let markdown = summary_markdown.expect("checked");
            let summary = CompactionSummaryV1::new_cumulative_source_cut(
                crate::compaction_summary::NewCumulativeCompactionSummaryV1 {
                    thread_id: thread_id.to_string(),
                    to_seq,
                    to_message_id: Some(to_message_id.clone()),
                    actor_id: actor_id.clone(),
                    origin: origin.clone(),
                    produced_by: Some(("manual".to_string(), "rip-cli".to_string())),
                    base_summary_artifact_id,
                    basis_note: None,
                    summary_markdown: markdown,
                },
            );
            write_compaction_summary_v1(&self.workspace_root, &summary)?
        };

        let checkpoint_id = self.append_compaction_checkpoint_created(
            thread_id,
            CompactionCheckpointCreatedPayload {
                cut_rule_id: cut_rule_id.clone(),
                summary_kind: COMPACTION_SUMMARY_KIND_CUMULATIVE_V1.to_string(),
                summary_artifact_id: summary_artifact_id.clone(),
                from_seq: 0,
                from_message_id: None,
                to_seq,
                to_message_id: Some(to_message_id.clone()),
                actor_id,
                origin,
            },
        )?;

        Ok((
            checkpoint_id,
            summary_artifact_id,
            to_seq,
            to_message_id,
            cut_rule_id,
        ))
    }

    pub fn compaction_cut_points_v1(
        &self,
        thread_id: &str,
        req: CompactionCutPointsV1Request,
    ) -> Result<CompactionCutPointsV1Response, String> {
        let stride = req.stride_messages.unwrap_or(10_000);
        if stride == 0 {
            return Err("invalid_stride".to_string());
        }
        let limit = req.limit.unwrap_or(1).clamp(1, 32) as u64;

        let mut replayed: Option<Vec<Event>> = None;
        let mut message_events: Option<Vec<(u64, String)>> = None;
        let mut checkpoint_index: Option<Vec<(u64, u64, String)>> = None;

        let message_count = match self.stream_cache.message_count_messages_runs_v1(thread_id) {
            Ok(Some(count)) => count,
            _ => {
                let events = self
                    .replay_events(thread_id)
                    .map_err(|err| format!("continuity replay failed: {err}"))?;
                if events.is_empty() {
                    return Err("thread_not_found".to_string());
                }
                replayed = Some(events);
                match self.stream_cache.message_count_messages_runs_v1(thread_id) {
                    Ok(Some(count)) => count,
                    _ => {
                        let events = replayed.as_ref().expect("set");
                        let msgs: Vec<(u64, String)> = events
                            .iter()
                            .filter_map(|event| match &event.kind {
                                EventKind::ContinuityMessageAppended { .. } => {
                                    Some((event.seq, event.id.clone()))
                                }
                                _ => None,
                            })
                            .collect();
                        let count = msgs.len() as u64;
                        message_events = Some(msgs);
                        count
                    }
                }
            }
        };

        let cut_rule_id = format!("stride_messages_v1/{stride}");

        let mut cut_points: Vec<CompactionCutPointV1> = Vec::new();
        let latest_multiple = (message_count / stride) * stride;
        if latest_multiple == 0 {
            return Ok(CompactionCutPointsV1Response {
                thread_id: thread_id.to_string(),
                stride_messages: stride,
                message_count,
                cut_rule_id,
                cut_points,
            });
        }

        for i in 0..limit {
            let ordinal = latest_multiple.saturating_sub(i.saturating_mul(stride));
            if ordinal == 0 {
                break;
            }

            let resolved = self
                .stream_cache
                .message_by_ordinal_messages_runs_v1(thread_id, ordinal)
                .ok()
                .flatten()
                .or_else(|| {
                    message_events.as_ref().and_then(|events| {
                        let idx = (ordinal - 1) as usize;
                        let (seq, id) = events.get(idx)?.clone();
                        Some((seq, id))
                    })
                });
            let (to_seq, to_message_id) = match resolved {
                Some((to_seq, to_message_id)) => (to_seq, to_message_id),
                None => {
                    if replayed.is_none() {
                        let events = self
                            .replay_events(thread_id)
                            .map_err(|err| format!("continuity replay failed: {err}"))?;
                        if events.is_empty() {
                            return Err("thread_not_found".to_string());
                        }
                        replayed = Some(events);
                    }
                    if message_events.is_none() {
                        let events = replayed.as_ref().expect("set");
                        let msgs: Vec<(u64, String)> = events
                            .iter()
                            .filter_map(|event| match &event.kind {
                                EventKind::ContinuityMessageAppended { .. } => {
                                    Some((event.seq, event.id.clone()))
                                }
                                _ => None,
                            })
                            .collect();
                        message_events = Some(msgs);
                    }
                    let msgs = message_events.as_ref().expect("set");
                    let idx = (ordinal - 1) as usize;
                    let Some((to_seq, to_message_id)) = msgs.get(idx).cloned() else {
                        continue;
                    };
                    (to_seq, to_message_id)
                }
            };

            let mut best_checkpoint_to_seq: Option<u64> = None;
            let mut best_checkpoint_seq: u64 = 0;
            let mut best_checkpoint_id: Option<String> = None;

            let cache_best = self
                .stream_cache
                .latest_compaction_checkpoint_before_or_at_seq_v1(thread_id, to_seq);
            match cache_best {
                Ok(Some(event)) => {
                    if let EventKind::ContinuityCompactionCheckpointCreated {
                        checkpoint_id,
                        to_seq: checkpoint_to_seq,
                        ..
                    } = &event.kind
                    {
                        best_checkpoint_to_seq = Some(*checkpoint_to_seq);
                        best_checkpoint_seq = event.seq;
                        best_checkpoint_id = Some(checkpoint_id.clone());
                    }
                }
                Ok(None) => {
                    if self
                        .stream_cache
                        .try_read_last_seq(thread_id)
                        .ok()
                        .flatten()
                        .is_none()
                        && replayed.is_none()
                    {
                        let events = self
                            .replay_events(thread_id)
                            .map_err(|err| format!("continuity replay failed: {err}"))?;
                        if events.is_empty() {
                            return Err("thread_not_found".to_string());
                        }
                        replayed = Some(events);
                    }
                }
                Err(_) => {
                    if replayed.is_none() {
                        let events = self
                            .replay_events(thread_id)
                            .map_err(|err| format!("continuity replay failed: {err}"))?;
                        if events.is_empty() {
                            return Err("thread_not_found".to_string());
                        }
                        replayed = Some(events);
                    }
                }
            }

            if best_checkpoint_to_seq.is_none() {
                if let Some(events) = replayed.as_ref() {
                    let idx = checkpoint_index.get_or_insert_with(|| {
                        events
                            .iter()
                            .filter_map(|event| match &event.kind {
                                EventKind::ContinuityCompactionCheckpointCreated {
                                    checkpoint_id,
                                    to_seq,
                                    ..
                                } => Some((*to_seq, event.seq, checkpoint_id.clone())),
                                _ => None,
                            })
                            .collect()
                    });
                    for (checkpoint_to_seq, checkpoint_seq, checkpoint_id) in idx.iter() {
                        if *checkpoint_to_seq > to_seq {
                            continue;
                        }
                        match best_checkpoint_to_seq {
                            None => {
                                best_checkpoint_to_seq = Some(*checkpoint_to_seq);
                                best_checkpoint_seq = *checkpoint_seq;
                                best_checkpoint_id = Some(checkpoint_id.clone());
                            }
                            Some(current_to_seq) => {
                                if *checkpoint_to_seq > current_to_seq
                                    || (*checkpoint_to_seq == current_to_seq
                                        && *checkpoint_seq > best_checkpoint_seq)
                                {
                                    best_checkpoint_to_seq = Some(*checkpoint_to_seq);
                                    best_checkpoint_seq = *checkpoint_seq;
                                    best_checkpoint_id = Some(checkpoint_id.clone());
                                }
                            }
                        }
                    }
                }
            }

            let already_checkpointed = best_checkpoint_to_seq == Some(to_seq);
            let latest_checkpoint_id = if already_checkpointed {
                best_checkpoint_id.clone()
            } else {
                None
            };

            cut_points.push(CompactionCutPointV1 {
                target_message_ordinal: ordinal,
                to_seq,
                to_message_id,
                already_checkpointed,
                latest_checkpoint_id,
            });
        }

        Ok(CompactionCutPointsV1Response {
            thread_id: thread_id.to_string(),
            stride_messages: stride,
            message_count,
            cut_rule_id,
            cut_points,
        })
    }
}
