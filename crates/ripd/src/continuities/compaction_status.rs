use super::*;

impl ContinuityStore {
    pub fn compaction_status_v1(
        &self,
        thread_id: &str,
        req: CompactionStatusV1Request,
    ) -> Result<CompactionStatusV1Response, String> {
        let stride = req.stride_messages.unwrap_or(10_000);
        if stride == 0 {
            return Err("invalid_stride".to_string());
        }

        let cut_points = self.compaction_cut_points_v1(
            thread_id,
            CompactionCutPointsV1Request {
                stride_messages: Some(stride),
                limit: Some(32),
            },
        )?;

        let next_cut_point = cut_points
            .cut_points
            .iter()
            .find(|cp| !cp.already_checkpointed)
            .map(|cp| CompactionPlannedCutPointV1 {
                target_message_ordinal: cp.target_message_ordinal,
                to_seq: cp.to_seq,
                to_message_id: cp.to_message_id.clone(),
            });

        let inflight_job_id = self.find_inflight_compaction_job_id_best_effort_v1(thread_id);

        let mut latest_checkpoint: Option<CompactionStatusCheckpointV1> = None;
        let mut last_schedule_decision: Option<CompactionStatusScheduleDecisionV1> = None;
        let mut last_job_outcome: Option<CompactionStatusJobOutcomeV1> = None;

        if let Ok(Some(event)) = self
            .stream_cache
            .latest_compaction_checkpoint_before_or_at_seq_v1(thread_id, u64::MAX)
        {
            if let EventKind::ContinuityCompactionCheckpointCreated {
                checkpoint_id,
                cut_rule_id,
                summary_kind,
                summary_artifact_id,
                to_seq,
                to_message_id,
                ..
            } = &event.kind
            {
                latest_checkpoint = Some(CompactionStatusCheckpointV1 {
                    checkpoint_id: checkpoint_id.clone(),
                    cut_rule_id: cut_rule_id.clone(),
                    summary_kind: summary_kind.clone(),
                    summary_artifact_id: summary_artifact_id.clone(),
                    to_seq: *to_seq,
                    to_message_id: to_message_id.clone(),
                });
            }
        }

        const INITIAL_TAIL_BYTES: usize = 256 * 1024;
        const MAX_TAIL_BYTES: usize = 8 * 1024 * 1024;
        const MAX_TAIL_EVENTS: usize = 10_000;

        let mut tail_bytes = INITIAL_TAIL_BYTES;
        while tail_bytes <= MAX_TAIL_BYTES
            && (last_schedule_decision.is_none() || last_job_outcome.is_none())
        {
            match self
                .stream_cache
                .scan_tail(thread_id, MAX_TAIL_EVENTS, tail_bytes)
            {
                Ok(Some(tail)) => {
                    for event in tail.events.iter().rev() {
                        if last_schedule_decision.is_none() {
                            if let EventKind::ContinuityCompactionAutoScheduleDecided {
                                decision_id,
                                policy_id,
                                decision,
                                execute,
                                stride_messages,
                                max_new_checkpoints,
                                block_on_inflight,
                                message_count,
                                cut_rule_id,
                                planned,
                                job_id,
                                job_kind,
                                actor_id,
                                origin,
                                ..
                            } = &event.kind
                            {
                                let planned_v1 = planned
                                    .iter()
                                    .map(|p| CompactionPlannedCutPointV1 {
                                        target_message_ordinal: p.target_message_ordinal,
                                        to_seq: p.to_seq,
                                        to_message_id: p.to_message_id.clone(),
                                    })
                                    .collect();
                                last_schedule_decision = Some(CompactionStatusScheduleDecisionV1 {
                                    decision_id: decision_id.clone(),
                                    policy_id: policy_id.clone(),
                                    decision: decision.clone(),
                                    execute: *execute,
                                    stride_messages: *stride_messages,
                                    max_new_checkpoints: *max_new_checkpoints,
                                    block_on_inflight: *block_on_inflight,
                                    message_count: *message_count,
                                    cut_rule_id: cut_rule_id.clone(),
                                    planned: planned_v1,
                                    job_id: job_id.clone(),
                                    job_kind: job_kind.clone(),
                                    actor_id: actor_id.clone(),
                                    origin: origin.clone(),
                                    seq: event.seq,
                                    timestamp_ms: event.timestamp_ms,
                                });
                            }
                        }

                        if last_job_outcome.is_none() {
                            if let EventKind::ContinuityJobEnded {
                                job_id,
                                job_kind,
                                status,
                                result,
                                error,
                                actor_id,
                                origin,
                            } = &event.kind
                            {
                                if job_kind == COMPACTION_JOB_KIND_SUMMARIZER_V1 {
                                    let created = parse_compaction_job_created_checkpoints(result);
                                    last_job_outcome = Some(CompactionStatusJobOutcomeV1 {
                                        job_id: job_id.clone(),
                                        job_kind: job_kind.clone(),
                                        status: status.clone(),
                                        error: error.clone(),
                                        created,
                                        actor_id: actor_id.clone(),
                                        origin: origin.clone(),
                                        seq: event.seq,
                                        timestamp_ms: event.timestamp_ms,
                                    });
                                }
                            }
                        }

                        if last_schedule_decision.is_some() && last_job_outcome.is_some() {
                            break;
                        }
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

        let need_replay = latest_checkpoint.is_none()
            || last_schedule_decision.is_none()
            || last_job_outcome.is_none();

        if need_replay {
            let events = self
                .replay_events(thread_id)
                .map_err(|err| format!("continuity replay failed: {err}"))?;
            if events.is_empty() {
                return Err("thread_not_found".to_string());
            }

            if latest_checkpoint.is_none() {
                let mut best: Option<CompactionStatusCheckpointV1> = None;
                let mut best_to_seq: u64 = 0;
                let mut best_event_seq: u64 = 0;
                for event in &events {
                    let EventKind::ContinuityCompactionCheckpointCreated {
                        checkpoint_id,
                        cut_rule_id,
                        summary_kind,
                        summary_artifact_id,
                        to_seq,
                        to_message_id,
                        ..
                    } = &event.kind
                    else {
                        continue;
                    };

                    if *to_seq > best_to_seq
                        || (*to_seq == best_to_seq && event.seq > best_event_seq)
                    {
                        best_to_seq = *to_seq;
                        best_event_seq = event.seq;
                        best = Some(CompactionStatusCheckpointV1 {
                            checkpoint_id: checkpoint_id.clone(),
                            cut_rule_id: cut_rule_id.clone(),
                            summary_kind: summary_kind.clone(),
                            summary_artifact_id: summary_artifact_id.clone(),
                            to_seq: *to_seq,
                            to_message_id: to_message_id.clone(),
                        });
                    }
                }
                latest_checkpoint = best;
            }

            for event in events.iter().rev() {
                if last_schedule_decision.is_none() {
                    if let EventKind::ContinuityCompactionAutoScheduleDecided {
                        decision_id,
                        policy_id,
                        decision,
                        execute,
                        stride_messages,
                        max_new_checkpoints,
                        block_on_inflight,
                        message_count,
                        cut_rule_id,
                        planned,
                        job_id,
                        job_kind,
                        actor_id,
                        origin,
                        ..
                    } = &event.kind
                    {
                        let planned_v1 = planned
                            .iter()
                            .map(|p| CompactionPlannedCutPointV1 {
                                target_message_ordinal: p.target_message_ordinal,
                                to_seq: p.to_seq,
                                to_message_id: p.to_message_id.clone(),
                            })
                            .collect();
                        last_schedule_decision = Some(CompactionStatusScheduleDecisionV1 {
                            decision_id: decision_id.clone(),
                            policy_id: policy_id.clone(),
                            decision: decision.clone(),
                            execute: *execute,
                            stride_messages: *stride_messages,
                            max_new_checkpoints: *max_new_checkpoints,
                            block_on_inflight: *block_on_inflight,
                            message_count: *message_count,
                            cut_rule_id: cut_rule_id.clone(),
                            planned: planned_v1,
                            job_id: job_id.clone(),
                            job_kind: job_kind.clone(),
                            actor_id: actor_id.clone(),
                            origin: origin.clone(),
                            seq: event.seq,
                            timestamp_ms: event.timestamp_ms,
                        });
                    }
                }

                if last_job_outcome.is_none() {
                    if let EventKind::ContinuityJobEnded {
                        job_id,
                        job_kind,
                        status,
                        result,
                        error,
                        actor_id,
                        origin,
                    } = &event.kind
                    {
                        if job_kind == COMPACTION_JOB_KIND_SUMMARIZER_V1 {
                            let created = parse_compaction_job_created_checkpoints(result);
                            last_job_outcome = Some(CompactionStatusJobOutcomeV1 {
                                job_id: job_id.clone(),
                                job_kind: job_kind.clone(),
                                status: status.clone(),
                                error: error.clone(),
                                created,
                                actor_id: actor_id.clone(),
                                origin: origin.clone(),
                                seq: event.seq,
                                timestamp_ms: event.timestamp_ms,
                            });
                        }
                    }
                }

                if last_schedule_decision.is_some() && last_job_outcome.is_some() {
                    break;
                }
            }
        }

        Ok(CompactionStatusV1Response {
            thread_id: thread_id.to_string(),
            stride_messages: stride,
            message_count: cut_points.message_count,
            latest_checkpoint,
            next_cut_point,
            inflight_job_id,
            last_schedule_decision,
            last_job_outcome,
        })
    }

    pub(super) fn find_inflight_compaction_job_id_best_effort_v1(
        &self,
        continuity_id: &str,
    ) -> Option<String> {
        const MAX_TAIL_EVENTS: usize = 512;
        const MAX_TAIL_BYTES: usize = 512 * 1024;

        let tail = self
            .stream_cache
            .scan_tail(continuity_id, MAX_TAIL_EVENTS, MAX_TAIL_BYTES)
            .ok()
            .flatten()?;

        let mut ended: std::collections::HashSet<String> = std::collections::HashSet::new();
        for event in tail.events.iter().rev() {
            match &event.kind {
                EventKind::ContinuityJobEnded {
                    job_id, job_kind, ..
                } => {
                    if job_kind == COMPACTION_JOB_KIND_SUMMARIZER_V1 {
                        ended.insert(job_id.clone());
                    }
                }
                EventKind::ContinuityJobSpawned {
                    job_id, job_kind, ..
                } => {
                    if job_kind == COMPACTION_JOB_KIND_SUMMARIZER_V1 && !ended.contains(job_id) {
                        return Some(job_id.clone());
                    }
                }
                _ => {}
            }
        }
        None
    }
}

fn parse_compaction_job_created_checkpoints(
    result: &Option<serde_json::Value>,
) -> Vec<CompactionAutoResultCheckpointV1> {
    result
        .as_ref()
        .and_then(|result| result.get("created"))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default()
}
