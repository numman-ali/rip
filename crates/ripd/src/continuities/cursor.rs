use super::*;

impl ContinuityStore {
    pub fn provider_cursor_status_v1(
        &self,
        thread_id: &str,
        _req: ProviderCursorStatusV1Request,
    ) -> Result<ProviderCursorStatusV1Response, String> {
        if self.get(thread_id).is_none() {
            return Err("not_found".to_string());
        }

        #[derive(Clone, PartialEq, Eq, Hash)]
        struct Key {
            provider: String,
            endpoint: Option<String>,
            model: Option<String>,
        }

        let mut active: Option<ProviderCursorStatusCursorV1> = None;
        let mut by_key: HashMap<Key, ProviderCursorStatusCursorV1> = HashMap::new();

        const INITIAL_TAIL_BYTES: usize = 256 * 1024;
        const MAX_TAIL_BYTES: usize = 8 * 1024 * 1024;
        const MAX_TAIL_EVENTS: usize = 10_000;
        const MAX_KEYS: usize = 32;

        let mut tail_bytes = INITIAL_TAIL_BYTES;
        let mut scanned_sidecar = false;
        while tail_bytes <= MAX_TAIL_BYTES {
            match self
                .stream_cache
                .scan_tail(thread_id, MAX_TAIL_EVENTS, tail_bytes)
            {
                Ok(Some(tail)) => {
                    scanned_sidecar = true;
                    for event in tail.events.iter().rev() {
                        let EventKind::ContinuityProviderCursorUpdated {
                            provider,
                            endpoint,
                            model,
                            cursor,
                            action,
                            reason,
                            run_session_id,
                            actor_id,
                            origin,
                        } = &event.kind
                        else {
                            continue;
                        };

                        let cursor_row = ProviderCursorStatusCursorV1 {
                            cursor_event_id: event.id.clone(),
                            provider: provider.clone(),
                            endpoint: endpoint.clone(),
                            model: model.clone(),
                            cursor: cursor.clone(),
                            action: action.clone(),
                            reason: reason.clone(),
                            run_session_id: run_session_id.clone(),
                            actor_id: actor_id.clone(),
                            origin: origin.clone(),
                            seq: event.seq,
                            timestamp_ms: event.timestamp_ms,
                        };

                        if active.is_none() {
                            active = Some(cursor_row.clone());
                        }

                        let key = Key {
                            provider: provider.clone(),
                            endpoint: endpoint.clone(),
                            model: model.clone(),
                        };
                        by_key.entry(key).or_insert(cursor_row);

                        if by_key.len() >= MAX_KEYS {
                            break;
                        }
                    }

                    if tail.complete || by_key.len() >= MAX_KEYS {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }

            tail_bytes = (tail_bytes * 2).min(MAX_TAIL_BYTES);
        }

        if !scanned_sidecar {
            let events = self
                .replay_events(thread_id)
                .map_err(|err| format!("continuity replay failed: {err}"))?;

            for event in events.iter().rev() {
                let EventKind::ContinuityProviderCursorUpdated {
                    provider,
                    endpoint,
                    model,
                    cursor,
                    action,
                    reason,
                    run_session_id,
                    actor_id,
                    origin,
                } = &event.kind
                else {
                    continue;
                };

                let cursor_row = ProviderCursorStatusCursorV1 {
                    cursor_event_id: event.id.clone(),
                    provider: provider.clone(),
                    endpoint: endpoint.clone(),
                    model: model.clone(),
                    cursor: cursor.clone(),
                    action: action.clone(),
                    reason: reason.clone(),
                    run_session_id: run_session_id.clone(),
                    actor_id: actor_id.clone(),
                    origin: origin.clone(),
                    seq: event.seq,
                    timestamp_ms: event.timestamp_ms,
                };

                if active.is_none() {
                    active = Some(cursor_row.clone());
                }

                let key = Key {
                    provider: provider.clone(),
                    endpoint: endpoint.clone(),
                    model: model.clone(),
                };
                by_key.entry(key).or_insert(cursor_row);

                if by_key.len() >= MAX_KEYS {
                    break;
                }
            }
        }

        let mut cursors: Vec<ProviderCursorStatusCursorV1> = by_key.into_values().collect();
        cursors.sort_by(|a, b| {
            (
                a.provider.as_str(),
                a.endpoint.as_deref().unwrap_or(""),
                a.model.as_deref().unwrap_or(""),
            )
                .cmp(&(
                    b.provider.as_str(),
                    b.endpoint.as_deref().unwrap_or(""),
                    b.model.as_deref().unwrap_or(""),
                ))
        });

        Ok(ProviderCursorStatusV1Response {
            thread_id: thread_id.to_string(),
            active,
            cursors,
        })
    }

    pub fn provider_cursor_rotate_v1(
        &self,
        thread_id: &str,
        mut req: ProviderCursorRotateV1Request,
    ) -> Result<ProviderCursorRotateV1Response, String> {
        if self.get(thread_id).is_none() {
            return Err("not_found".to_string());
        }

        if req.actor_id.trim().is_empty() {
            req.actor_id = "user".to_string();
        }
        if req.origin.trim().is_empty() {
            req.origin = "unknown".to_string();
        }

        let matches_filter = |provider: &str,
                              endpoint: Option<&str>,
                              model: Option<&str>,
                              req: &ProviderCursorRotateV1Request|
         -> bool {
            if let Some(filter) = req.provider.as_deref() {
                if provider != filter {
                    return false;
                }
            }
            if let Some(filter) = req.endpoint.as_deref() {
                if endpoint != Some(filter) {
                    return false;
                }
            }
            if let Some(filter) = req.model.as_deref() {
                if model != Some(filter) {
                    return false;
                }
            }
            true
        };

        const INITIAL_TAIL_BYTES: usize = 256 * 1024;
        const MAX_TAIL_BYTES: usize = 8 * 1024 * 1024;
        const MAX_TAIL_EVENTS: usize = 10_000;

        let mut target: Option<(String, Option<String>, Option<String>)> = None;
        let mut tail_bytes = INITIAL_TAIL_BYTES;
        while tail_bytes <= MAX_TAIL_BYTES && target.is_none() {
            match self
                .stream_cache
                .scan_tail(thread_id, MAX_TAIL_EVENTS, tail_bytes)
            {
                Ok(Some(tail)) => {
                    for event in tail.events.iter().rev() {
                        let EventKind::ContinuityProviderCursorUpdated {
                            provider,
                            endpoint,
                            model,
                            ..
                        } = &event.kind
                        else {
                            continue;
                        };
                        if !matches_filter(provider, endpoint.as_deref(), model.as_deref(), &req) {
                            continue;
                        }
                        target = Some((provider.clone(), endpoint.clone(), model.clone()));
                        break;
                    }
                    if tail.complete {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
            tail_bytes = (tail_bytes * 2).min(MAX_TAIL_BYTES);
        }

        if target.is_none() {
            let events = self
                .replay_events(thread_id)
                .map_err(|err| format!("continuity replay failed: {err}"))?;
            for event in events.iter().rev() {
                let EventKind::ContinuityProviderCursorUpdated {
                    provider,
                    endpoint,
                    model,
                    ..
                } = &event.kind
                else {
                    continue;
                };
                if !matches_filter(provider, endpoint.as_deref(), model.as_deref(), &req) {
                    continue;
                }
                target = Some((provider.clone(), endpoint.clone(), model.clone()));
                break;
            }
        }

        let Some((provider, endpoint, model)) = target else {
            return Ok(ProviderCursorRotateV1Response {
                thread_id: thread_id.to_string(),
                rotated: false,
                provider: None,
                endpoint: None,
                model: None,
                cursor_event_id: None,
            });
        };

        let id = self.append_provider_cursor_updated(
            thread_id,
            ProviderCursorUpdatedPayload {
                provider: provider.clone(),
                endpoint: endpoint.clone(),
                model: model.clone(),
                cursor: None,
                action: "rotated".to_string(),
                reason: req.reason.clone(),
                run_session_id: None,
                actor_id: req.actor_id,
                origin: req.origin,
            },
        )?;

        Ok(ProviderCursorRotateV1Response {
            thread_id: thread_id.to_string(),
            rotated: true,
            provider: Some(provider),
            endpoint,
            model,
            cursor_event_id: Some(id),
        })
    }

    pub fn context_selection_status_v1(
        &self,
        thread_id: &str,
        req: ContextSelectionStatusV1Request,
    ) -> Result<ContextSelectionStatusV1Response, String> {
        if self.get(thread_id).is_none() {
            return Err("not_found".to_string());
        }

        const DEFAULT_LIMIT: usize = 10;
        const MAX_LIMIT: usize = 50;
        const INITIAL_TAIL_BYTES: usize = 256 * 1024;
        const MAX_TAIL_BYTES: usize = 8 * 1024 * 1024;
        const MAX_TAIL_EVENTS: usize = 100_000;

        let mut limit = req.limit.unwrap_or(DEFAULT_LIMIT as u32) as usize;
        limit = limit.min(MAX_LIMIT);

        let mut decisions: Vec<ContextSelectionStatusDecisionV1> = Vec::new();

        let mut tail_complete = false;
        let mut scanned_sidecar = false;
        let mut tail_bytes = INITIAL_TAIL_BYTES;
        while tail_bytes <= MAX_TAIL_BYTES && decisions.len() < limit {
            match self
                .stream_cache
                .scan_tail(thread_id, MAX_TAIL_EVENTS, tail_bytes)
            {
                Ok(Some(tail)) => {
                    scanned_sidecar = true;
                    for event in tail.events.iter().rev() {
                        let EventKind::ContinuityContextSelectionDecided {
                            run_session_id,
                            message_id,
                            compiler_id,
                            compiler_strategy,
                            limits,
                            compaction_checkpoint,
                            compaction_checkpoints,
                            resets,
                            reason,
                            actor_id,
                            origin,
                        } = &event.kind
                        else {
                            continue;
                        };

                        let checkpoint = compaction_checkpoint.as_ref().map(|ckpt| {
                            ContextSelectionStatusCheckpointV1 {
                                checkpoint_id: ckpt.checkpoint_id.clone(),
                                summary_kind: ckpt.summary_kind.clone(),
                                summary_artifact_id: ckpt.summary_artifact_id.clone(),
                                to_seq: ckpt.to_seq,
                            }
                        });

                        let checkpoints = compaction_checkpoints
                            .iter()
                            .map(|ckpt| ContextSelectionStatusCheckpointV1 {
                                checkpoint_id: ckpt.checkpoint_id.clone(),
                                summary_kind: ckpt.summary_kind.clone(),
                                summary_artifact_id: ckpt.summary_artifact_id.clone(),
                                to_seq: ckpt.to_seq,
                            })
                            .collect();

                        let resets_v1 = resets
                            .iter()
                            .map(|reset| ContextSelectionStatusResetV1 {
                                input: reset.input.clone(),
                                action: reset.action.clone(),
                                reason: reset.reason.clone(),
                                ref_: reset.ref_.clone(),
                            })
                            .collect();

                        decisions.push(ContextSelectionStatusDecisionV1 {
                            decision_event_id: event.id.clone(),
                            run_session_id: run_session_id.clone(),
                            message_id: message_id.clone(),
                            compiler_id: compiler_id.clone(),
                            compiler_strategy: compiler_strategy.clone(),
                            limits: limits.clone(),
                            compaction_checkpoint: checkpoint,
                            compaction_checkpoints: checkpoints,
                            resets: resets_v1,
                            reason: reason.clone(),
                            actor_id: actor_id.clone(),
                            origin: origin.clone(),
                            seq: event.seq,
                            timestamp_ms: event.timestamp_ms,
                        });

                        if decisions.len() >= limit {
                            break;
                        }
                    }

                    if tail.complete {
                        tail_complete = true;
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }

            tail_bytes = (tail_bytes * 2).min(MAX_TAIL_BYTES);
        }

        if !scanned_sidecar || (!tail_complete && decisions.len() < limit) {
            let events = self
                .replay_events(thread_id)
                .map_err(|err| format!("continuity replay failed: {err}"))?;

            decisions.clear();
            for event in events.iter().rev() {
                let EventKind::ContinuityContextSelectionDecided {
                    run_session_id,
                    message_id,
                    compiler_id,
                    compiler_strategy,
                    limits,
                    compaction_checkpoint,
                    compaction_checkpoints,
                    resets,
                    reason,
                    actor_id,
                    origin,
                } = &event.kind
                else {
                    continue;
                };

                let checkpoint =
                    compaction_checkpoint
                        .as_ref()
                        .map(|ckpt| ContextSelectionStatusCheckpointV1 {
                            checkpoint_id: ckpt.checkpoint_id.clone(),
                            summary_kind: ckpt.summary_kind.clone(),
                            summary_artifact_id: ckpt.summary_artifact_id.clone(),
                            to_seq: ckpt.to_seq,
                        });

                let checkpoints = compaction_checkpoints
                    .iter()
                    .map(|ckpt| ContextSelectionStatusCheckpointV1 {
                        checkpoint_id: ckpt.checkpoint_id.clone(),
                        summary_kind: ckpt.summary_kind.clone(),
                        summary_artifact_id: ckpt.summary_artifact_id.clone(),
                        to_seq: ckpt.to_seq,
                    })
                    .collect();

                let resets_v1 = resets
                    .iter()
                    .map(|reset| ContextSelectionStatusResetV1 {
                        input: reset.input.clone(),
                        action: reset.action.clone(),
                        reason: reset.reason.clone(),
                        ref_: reset.ref_.clone(),
                    })
                    .collect();

                decisions.push(ContextSelectionStatusDecisionV1 {
                    decision_event_id: event.id.clone(),
                    run_session_id: run_session_id.clone(),
                    message_id: message_id.clone(),
                    compiler_id: compiler_id.clone(),
                    compiler_strategy: compiler_strategy.clone(),
                    limits: limits.clone(),
                    compaction_checkpoint: checkpoint,
                    compaction_checkpoints: checkpoints,
                    resets: resets_v1,
                    reason: reason.clone(),
                    actor_id: actor_id.clone(),
                    origin: origin.clone(),
                    seq: event.seq,
                    timestamp_ms: event.timestamp_ms,
                });

                if decisions.len() >= limit {
                    break;
                }
            }
        }

        Ok(ContextSelectionStatusV1Response {
            thread_id: thread_id.to_string(),
            decisions,
        })
    }
}
