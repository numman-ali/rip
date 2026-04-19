use super::*;
use uuid::Uuid;

impl ContinuityStore {
    pub fn list(&self) -> Vec<ContinuityMeta> {
        let index = self.index.lock().expect("continuity index mutex");
        index
            .continuities
            .iter()
            .map(|(id, meta)| ContinuityMeta {
                continuity_id: id.clone(),
                created_at_ms: meta.created_at_ms,
                title: meta.title.clone(),
                archived: meta.archived,
            })
            .collect()
    }

    pub fn get(&self, continuity_id: &str) -> Option<ContinuityMeta> {
        let index = self.index.lock().expect("continuity index mutex");
        let meta = index.continuities.get(continuity_id)?;
        Some(ContinuityMeta {
            continuity_id: continuity_id.to_string(),
            created_at_ms: meta.created_at_ms,
            title: meta.title.clone(),
            archived: meta.archived,
        })
    }

    pub fn append_message(
        &self,
        continuity_id: &str,
        actor_id: String,
        origin: String,
        content: String,
    ) -> Result<String, String> {
        self.append_with_next_seq(continuity_id, "append continuity message", |seq| {
            let message_id = Uuid::new_v4().to_string();
            let event = Event {
                id: message_id.clone(),
                session_id: continuity_id.to_string(),
                timestamp_ms: now_ms(),
                seq,
                kind: EventKind::ContinuityMessageAppended {
                    actor_id,
                    origin,
                    content,
                },
            };
            (event, message_id)
        })
    }

    pub fn append_run_spawned(
        &self,
        continuity_id: &str,
        message_id: &str,
        session_id: &str,
        actor_id: String,
        origin: String,
    ) -> Result<String, String> {
        self.append_with_next_seq(continuity_id, "append continuity run spawned", |seq| {
            let id = Uuid::new_v4().to_string();
            let event = Event {
                id: id.clone(),
                session_id: continuity_id.to_string(),
                timestamp_ms: now_ms(),
                seq,
                kind: EventKind::ContinuityRunSpawned {
                    run_session_id: session_id.to_string(),
                    message_id: message_id.to_string(),
                    actor_id: Some(actor_id),
                    origin: Some(origin),
                },
            };
            (event, id)
        })
    }

    pub(crate) fn append_context_selection_decided(
        &self,
        continuity_id: &str,
        payload: ContextSelectionDecidedPayload,
    ) -> Result<String, String> {
        self.append_with_next_seq(
            continuity_id,
            "append continuity context selection decided",
            |seq| {
                let id = Uuid::new_v4().to_string();
                let event = Event {
                    id: id.clone(),
                    session_id: continuity_id.to_string(),
                    timestamp_ms: now_ms(),
                    seq,
                    kind: EventKind::ContinuityContextSelectionDecided {
                        run_session_id: payload.run_session_id,
                        message_id: payload.message_id,
                        compiler_id: payload.compiler_id,
                        compiler_strategy: payload.compiler_strategy,
                        limits: payload.limits,
                        compaction_checkpoint: payload.compaction_checkpoint,
                        compaction_checkpoints: payload.compaction_checkpoints,
                        resets: payload.resets,
                        reason: payload.reason,
                        actor_id: payload.actor_id,
                        origin: payload.origin,
                    },
                };
                (event, id)
            },
        )
    }

    pub(crate) fn append_context_compiled(
        &self,
        continuity_id: &str,
        payload: ContextCompiledPayload,
    ) -> Result<String, String> {
        self.append_with_next_seq(continuity_id, "append continuity context compiled", |seq| {
            let id = Uuid::new_v4().to_string();
            let event = Event {
                id: id.clone(),
                session_id: continuity_id.to_string(),
                timestamp_ms: now_ms(),
                seq,
                kind: EventKind::ContinuityContextCompiled {
                    run_session_id: payload.run_session_id,
                    bundle_artifact_id: payload.bundle_artifact_id,
                    compiler_id: payload.compiler_id,
                    compiler_strategy: payload.compiler_strategy,
                    from_seq: payload.from_seq,
                    from_message_id: payload.from_message_id,
                    actor_id: payload.actor_id,
                    origin: payload.origin,
                },
            };
            (event, id)
        })
    }

    pub(crate) fn append_provider_cursor_updated(
        &self,
        continuity_id: &str,
        payload: ProviderCursorUpdatedPayload,
    ) -> Result<String, String> {
        self.append_with_next_seq(
            continuity_id,
            "append continuity provider cursor updated",
            |seq| {
                let id = Uuid::new_v4().to_string();
                let event = Event {
                    id: id.clone(),
                    session_id: continuity_id.to_string(),
                    timestamp_ms: now_ms(),
                    seq,
                    kind: EventKind::ContinuityProviderCursorUpdated {
                        provider: payload.provider,
                        endpoint: payload.endpoint,
                        model: payload.model,
                        cursor: payload.cursor,
                        action: payload.action,
                        reason: payload.reason,
                        run_session_id: payload.run_session_id,
                        actor_id: payload.actor_id,
                        origin: payload.origin,
                    },
                };
                (event, id)
            },
        )
    }

    pub(super) fn append_compaction_checkpoint_created(
        &self,
        continuity_id: &str,
        payload: CompactionCheckpointCreatedPayload,
    ) -> Result<String, String> {
        self.append_with_next_seq(
            continuity_id,
            "append continuity compaction checkpoint",
            |seq| {
                let checkpoint_id = Uuid::new_v4().to_string();
                let event = Event {
                    id: checkpoint_id.clone(),
                    session_id: continuity_id.to_string(),
                    timestamp_ms: now_ms(),
                    seq,
                    kind: EventKind::ContinuityCompactionCheckpointCreated {
                        checkpoint_id: checkpoint_id.clone(),
                        cut_rule_id: payload.cut_rule_id,
                        summary_kind: payload.summary_kind,
                        summary_artifact_id: payload.summary_artifact_id,
                        from_seq: payload.from_seq,
                        from_message_id: payload.from_message_id,
                        to_seq: payload.to_seq,
                        to_message_id: payload.to_message_id,
                        actor_id: payload.actor_id,
                        origin: payload.origin,
                    },
                };
                (event, checkpoint_id)
            },
        )
    }

    pub(super) fn append_compaction_auto_schedule_decided(
        &self,
        continuity_id: &str,
        payload: CompactionAutoScheduleDecidedPayload,
    ) -> Result<String, String> {
        self.append_with_next_seq(
            continuity_id,
            "append continuity compaction schedule decided",
            |seq| {
                let id = payload.decision_id.clone();
                let event = Event {
                    id: id.clone(),
                    session_id: continuity_id.to_string(),
                    timestamp_ms: now_ms(),
                    seq,
                    kind: EventKind::ContinuityCompactionAutoScheduleDecided {
                        decision_id: payload.decision_id,
                        policy_id: payload.policy_id,
                        decision: payload.decision,
                        execute: payload.execute,
                        stride_messages: payload.stride_messages,
                        max_new_checkpoints: payload.max_new_checkpoints,
                        block_on_inflight: payload.block_on_inflight,
                        message_count: payload.message_count,
                        cut_rule_id: payload.cut_rule_id,
                        planned: payload.planned,
                        job_id: payload.job_id,
                        job_kind: payload.job_kind,
                        reason: payload.reason,
                        actor_id: payload.actor_id,
                        origin: payload.origin,
                    },
                };
                (event, id)
            },
        )
    }

    pub(super) fn append_job_spawned(
        &self,
        continuity_id: &str,
        job_id: &str,
        job_kind: &str,
        details: Option<serde_json::Value>,
        actor_id: String,
        origin: String,
    ) -> Result<String, String> {
        self.append_with_next_seq(continuity_id, "append continuity job spawned", |seq| {
            let id = Uuid::new_v4().to_string();
            let event = Event {
                id: id.clone(),
                session_id: continuity_id.to_string(),
                timestamp_ms: now_ms(),
                seq,
                kind: EventKind::ContinuityJobSpawned {
                    job_id: job_id.to_string(),
                    job_kind: job_kind.to_string(),
                    details,
                    actor_id,
                    origin,
                },
            };
            (event, id)
        })
    }

    pub(super) fn append_job_ended(
        &self,
        continuity_id: &str,
        payload: JobEndedPayload,
    ) -> Result<String, String> {
        self.append_with_next_seq(continuity_id, "append continuity job ended", |seq| {
            let id = Uuid::new_v4().to_string();
            let event = Event {
                id: id.clone(),
                session_id: continuity_id.to_string(),
                timestamp_ms: now_ms(),
                seq,
                kind: EventKind::ContinuityJobEnded {
                    job_id: payload.job_id,
                    job_kind: payload.job_kind,
                    status: payload.status,
                    result: payload.result,
                    error: payload.error,
                    actor_id: payload.actor_id,
                    origin: payload.origin,
                },
            };
            (event, id)
        })
    }

    pub fn append_run_ended(
        &self,
        continuity_id: &str,
        message_id: &str,
        session_id: &str,
        reason: String,
        actor_id: String,
        origin: String,
    ) -> Result<String, String> {
        self.append_with_next_seq(continuity_id, "append continuity run ended", |seq| {
            let id = Uuid::new_v4().to_string();
            let event = Event {
                id: id.clone(),
                session_id: continuity_id.to_string(),
                timestamp_ms: now_ms(),
                seq,
                kind: EventKind::ContinuityRunEnded {
                    run_session_id: session_id.to_string(),
                    message_id: message_id.to_string(),
                    reason,
                    actor_id: Some(actor_id),
                    origin: Some(origin),
                },
            };
            (event, id)
        })
    }

    pub fn append_tool_side_effects(
        &self,
        run: &ContinuityRunLink,
        run_session_id: &str,
        effects: ToolSideEffects,
    ) -> Result<String, String> {
        let continuity_id = run.continuity_id.as_str();
        self.append_with_next_seq(
            continuity_id,
            "append continuity tool side effects",
            |seq| {
                let id = Uuid::new_v4().to_string();
                let event = Event {
                    id: id.clone(),
                    session_id: continuity_id.to_string(),
                    timestamp_ms: now_ms(),
                    seq,
                    kind: EventKind::ContinuityToolSideEffects {
                        run_session_id: run_session_id.to_string(),
                        tool_id: effects.tool_id,
                        tool_name: effects.tool_name,
                        affected_paths: effects.affected_paths,
                        checkpoint_id: effects.checkpoint_id,
                        actor_id: run.actor_id.clone(),
                        origin: run.origin.clone(),
                    },
                };
                (event, id)
            },
        )
    }

    fn append_with_next_seq<T>(
        &self,
        continuity_id: &str,
        context: &str,
        build: impl FnOnce(u64) -> (Event, T),
    ) -> Result<T, String> {
        let mut next_seq = self.next_seq.lock().expect("continuity seq mutex");
        let seq = match next_seq.get(continuity_id).cloned() {
            Some(seq) => seq,
            None => {
                let seq = self
                    .load_next_seq_for(continuity_id)
                    .map_err(|err| format!("resolve continuity seq: {err}"))?;
                next_seq.insert(continuity_id.to_string(), seq);
                seq
            }
        };
        let (event, result) = build(seq);
        self.event_log
            .append(&event)
            .map_err(|err| format!("{context}: {err}"))?;
        self.stream_cache.append_best_effort(&event);
        let _ = self.sender.send(event);
        next_seq.insert(continuity_id.to_string(), seq + 1);
        Ok(result)
    }

    fn load_next_seq_for(&self, continuity_id: &str) -> Result<u64, io::Error> {
        if let Ok(Some(last_seq)) = self.stream_cache.try_read_last_seq(continuity_id) {
            return Ok(last_seq.saturating_add(1));
        }

        let events = self.replay_events(continuity_id)?;
        let last = events.last().ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "continuity stream does not exist")
        })?;
        Ok(last.seq.saturating_add(1))
    }
}
