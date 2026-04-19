use super::*;
use uuid::Uuid;

impl ContinuityStore {
    pub fn compaction_auto_v1(
        &self,
        thread_id: &str,
        req: CompactionAutoV1Request,
    ) -> Result<CompactionAutoV1Response, String> {
        let mut response = self.compaction_auto_spawn_job_v1(thread_id, req.clone())?;
        if response.status != "spawned" {
            return Ok(response);
        }

        let job_id = response
            .job_id
            .clone()
            .ok_or_else(|| "compaction auto spawned without job_id".to_string())?;

        match self.compaction_auto_run_spawned_job_v1(
            thread_id,
            &job_id,
            response.stride_messages,
            &response.cut_rule_id,
            &response.planned,
            (req.actor_id.as_str(), req.origin.as_str()),
        ) {
            Ok(result) => {
                response.status = "completed".to_string();
                response.result = result;
            }
            Err(err) => {
                response.status = "failed".to_string();
                response.error = Some(err);
            }
        }

        Ok(response)
    }

    pub fn compaction_auto_schedule_v1(
        &self,
        thread_id: &str,
        req: CompactionAutoScheduleV1Request,
    ) -> Result<CompactionAutoScheduleV1Response, String> {
        let mut response = self.compaction_auto_schedule_spawn_job_v1(thread_id, req.clone())?;
        if response.decision != "scheduled" || !response.execute {
            return Ok(response);
        }

        let job_id = response
            .job_id
            .clone()
            .ok_or_else(|| "compaction auto schedule spawned without job_id".to_string())?;

        match self.compaction_auto_run_spawned_job_v1(
            thread_id,
            &job_id,
            response.stride_messages,
            &response.cut_rule_id,
            &response.planned,
            (req.actor_id.as_str(), req.origin.as_str()),
        ) {
            Ok(result) => {
                response.decision = "completed".to_string();
                response.result = result;
            }
            Err(err) => {
                response.decision = "failed".to_string();
                response.error = Some(err);
            }
        }

        Ok(response)
    }

    pub(crate) fn compaction_auto_schedule_spawn_job_v1(
        &self,
        thread_id: &str,
        req: CompactionAutoScheduleV1Request,
    ) -> Result<CompactionAutoScheduleV1Response, String> {
        let stride = req.stride_messages.unwrap_or(10_000);
        if stride == 0 {
            return Err("invalid_stride".to_string());
        }
        let max_new_checkpoints = req.max_new_checkpoints.unwrap_or(1).clamp(1, 32);
        let block_on_inflight = req.block_on_inflight.unwrap_or(true);
        let execute = req.execute.unwrap_or(true);
        let dry_run = req.dry_run.unwrap_or(false);

        let cut_rule_id = format!("stride_messages_v1/{stride}");
        let policy_id = format!(
            "compaction_auto_schedule_v1/stride_messages_v1/{stride}/max_new_checkpoints_v1/{max_new_checkpoints}/block_on_inflight_v1/{block_on_inflight}"
        );

        let cut_points = self.compaction_cut_points_v1(
            thread_id,
            CompactionCutPointsV1Request {
                stride_messages: Some(stride),
                limit: Some(32),
            },
        )?;

        let mut planned: Vec<CompactionPlannedCutPointV1> = Vec::new();
        for cp in &cut_points.cut_points {
            if planned.len() as u32 >= max_new_checkpoints {
                break;
            }
            if cp.already_checkpointed {
                continue;
            }
            planned.push(CompactionPlannedCutPointV1 {
                target_message_ordinal: cp.target_message_ordinal,
                to_seq: cp.to_seq,
                to_message_id: cp.to_message_id.clone(),
            });
        }

        if planned.is_empty() {
            return Ok(CompactionAutoScheduleV1Response {
                thread_id: thread_id.to_string(),
                decision_id: None,
                policy_id,
                decision: "noop".to_string(),
                execute,
                stride_messages: stride,
                max_new_checkpoints,
                block_on_inflight,
                message_count: cut_points.message_count,
                cut_rule_id,
                planned,
                job_id: None,
                job_kind: None,
                result: Vec::new(),
                error: None,
            });
        }

        if dry_run {
            return Ok(CompactionAutoScheduleV1Response {
                thread_id: thread_id.to_string(),
                decision_id: None,
                policy_id,
                decision: "dry_run".to_string(),
                execute,
                stride_messages: stride,
                max_new_checkpoints,
                block_on_inflight,
                message_count: cut_points.message_count,
                cut_rule_id,
                planned,
                job_id: None,
                job_kind: None,
                result: Vec::new(),
                error: None,
            });
        }

        let inflight_job_id = if block_on_inflight {
            self.find_inflight_compaction_job_id_best_effort_v1(thread_id)
        } else {
            None
        };

        if let Some(job_id) = inflight_job_id {
            let decision_id = Uuid::new_v4().to_string();
            let planned_frame = planned
                .iter()
                .map(|p| CompactionPlannedCutPoint {
                    target_message_ordinal: p.target_message_ordinal,
                    to_seq: p.to_seq,
                    to_message_id: p.to_message_id.clone(),
                })
                .collect();
            self.append_compaction_auto_schedule_decided(
                thread_id,
                CompactionAutoScheduleDecidedPayload {
                    decision_id: decision_id.clone(),
                    policy_id: policy_id.clone(),
                    decision: "skipped_inflight".to_string(),
                    execute,
                    stride_messages: stride,
                    max_new_checkpoints,
                    block_on_inflight,
                    message_count: cut_points.message_count,
                    cut_rule_id: cut_rule_id.clone(),
                    planned: planned_frame,
                    job_id: None,
                    job_kind: None,
                    reason: Some(serde_json::json!({
                        "kind": "inflight_job",
                        "job_id": job_id,
                    })),
                    actor_id: req.actor_id.clone(),
                    origin: req.origin.clone(),
                },
            )?;

            return Ok(CompactionAutoScheduleV1Response {
                thread_id: thread_id.to_string(),
                decision_id: Some(decision_id),
                policy_id,
                decision: "skipped_inflight".to_string(),
                execute,
                stride_messages: stride,
                max_new_checkpoints,
                block_on_inflight,
                message_count: cut_points.message_count,
                cut_rule_id,
                planned,
                job_id: None,
                job_kind: None,
                result: Vec::new(),
                error: None,
            });
        }

        let spawned = self.compaction_auto_spawn_job_v1(
            thread_id,
            CompactionAutoV1Request {
                stride_messages: Some(stride),
                max_new_checkpoints: Some(max_new_checkpoints),
                dry_run: Some(false),
                actor_id: req.actor_id.clone(),
                origin: req.origin.clone(),
            },
        )?;

        if spawned.status != "spawned" {
            return Ok(CompactionAutoScheduleV1Response {
                thread_id: thread_id.to_string(),
                decision_id: None,
                policy_id,
                decision: "noop".to_string(),
                execute,
                stride_messages: stride,
                max_new_checkpoints,
                block_on_inflight,
                message_count: cut_points.message_count,
                cut_rule_id,
                planned,
                job_id: None,
                job_kind: None,
                result: Vec::new(),
                error: None,
            });
        }

        let decision_id = Uuid::new_v4().to_string();
        let planned_frame = planned
            .iter()
            .map(|p| CompactionPlannedCutPoint {
                target_message_ordinal: p.target_message_ordinal,
                to_seq: p.to_seq,
                to_message_id: p.to_message_id.clone(),
            })
            .collect();
        self.append_compaction_auto_schedule_decided(
            thread_id,
            CompactionAutoScheduleDecidedPayload {
                decision_id: decision_id.clone(),
                policy_id: policy_id.clone(),
                decision: "scheduled".to_string(),
                execute,
                stride_messages: stride,
                max_new_checkpoints,
                block_on_inflight,
                message_count: cut_points.message_count,
                cut_rule_id: cut_rule_id.clone(),
                planned: planned_frame,
                job_id: spawned.job_id.clone(),
                job_kind: spawned.job_kind.clone(),
                reason: None,
                actor_id: req.actor_id.clone(),
                origin: req.origin.clone(),
            },
        )?;

        Ok(CompactionAutoScheduleV1Response {
            thread_id: thread_id.to_string(),
            decision_id: Some(decision_id),
            policy_id,
            decision: "scheduled".to_string(),
            execute,
            stride_messages: stride,
            max_new_checkpoints,
            block_on_inflight,
            message_count: cut_points.message_count,
            cut_rule_id,
            planned,
            job_id: spawned.job_id,
            job_kind: spawned.job_kind,
            result: Vec::new(),
            error: None,
        })
    }

    pub(crate) fn compaction_auto_spawn_job_v1(
        &self,
        thread_id: &str,
        req: CompactionAutoV1Request,
    ) -> Result<CompactionAutoV1Response, String> {
        let stride = req.stride_messages.unwrap_or(10_000);
        if stride == 0 {
            return Err("invalid_stride".to_string());
        }
        let max_new = req.max_new_checkpoints.unwrap_or(1).clamp(1, 32) as u64;
        let dry_run = req.dry_run.unwrap_or(false);
        let cut_rule_id = format!("stride_messages_v1/{stride}");

        let cut_points = self.compaction_cut_points_v1(
            thread_id,
            CompactionCutPointsV1Request {
                stride_messages: Some(stride),
                limit: Some(32),
            },
        )?;

        let mut planned: Vec<CompactionPlannedCutPointV1> = Vec::new();
        for cp in &cut_points.cut_points {
            if planned.len() as u64 >= max_new {
                break;
            }
            if cp.already_checkpointed {
                continue;
            }
            planned.push(CompactionPlannedCutPointV1 {
                target_message_ordinal: cp.target_message_ordinal,
                to_seq: cp.to_seq,
                to_message_id: cp.to_message_id.clone(),
            });
        }

        if planned.is_empty() || dry_run {
            return Ok(CompactionAutoV1Response {
                thread_id: thread_id.to_string(),
                job_id: None,
                job_kind: None,
                status: "noop".to_string(),
                stride_messages: stride,
                message_count: cut_points.message_count,
                cut_rule_id,
                planned,
                result: Vec::new(),
                error: None,
            });
        }

        let job_id = Uuid::new_v4().to_string();
        let details_cut_rule_id = cut_rule_id.clone();
        let details_planned = planned.clone();
        let details = serde_json::json!({
            "schema": "rip.job.compaction_summarizer.v1",
            "cut_rule_id": details_cut_rule_id,
            "stride_messages": stride,
            "planned": details_planned,
        });

        self.append_job_spawned(
            thread_id,
            &job_id,
            COMPACTION_JOB_KIND_SUMMARIZER_V1,
            Some(details),
            req.actor_id.clone(),
            req.origin.clone(),
        )?;

        Ok(CompactionAutoV1Response {
            thread_id: thread_id.to_string(),
            job_id: Some(job_id),
            job_kind: Some(COMPACTION_JOB_KIND_SUMMARIZER_V1.to_string()),
            status: "spawned".to_string(),
            stride_messages: stride,
            message_count: cut_points.message_count,
            cut_rule_id,
            planned,
            result: Vec::new(),
            error: None,
        })
    }

    pub(crate) fn compaction_auto_run_spawned_job_v1(
        &self,
        thread_id: &str,
        job_id: &str,
        stride_messages: u64,
        cut_rule_id: &str,
        planned: &[CompactionPlannedCutPointV1],
        provenance: (&str, &str),
    ) -> Result<Vec<CompactionAutoResultCheckpointV1>, String> {
        let (actor_id, origin) = provenance;
        let mut created: Vec<CompactionAutoResultCheckpointV1> = Vec::new();

        let continuity_events = self
            .replay_events(thread_id)
            .map_err(|err| format!("continuity replay failed: {err}"))?;
        if continuity_events.is_empty() {
            return Err("thread_not_found".to_string());
        }

        struct MessageRef<'a> {
            seq: u64,
            id: &'a str,
            actor_id: &'a str,
            content: &'a str,
        }

        let mut messages: Vec<MessageRef<'_>> = Vec::new();
        for event in &continuity_events {
            let EventKind::ContinuityMessageAppended {
                actor_id, content, ..
            } = &event.kind
            else {
                continue;
            };
            messages.push(MessageRef {
                seq: event.seq,
                id: event.id.as_str(),
                actor_id,
                content,
            });
        }

        fn upper_bound_message_seq(messages: &[MessageRef<'_>], target_seq: u64) -> usize {
            match messages.binary_search_by(|m| m.seq.cmp(&target_seq)) {
                Ok(idx) => idx.saturating_add(1),
                Err(idx) => idx,
            }
        }

        let run_result: Result<(), String> = (|| {
            let mut planned_sorted: Vec<CompactionPlannedCutPointV1> = planned.to_vec();
            planned_sorted.sort_by(|a, b| {
                a.to_seq
                    .cmp(&b.to_seq)
                    .then(a.to_message_id.cmp(&b.to_message_id))
            });

            for cut in &planned_sorted {
                let mut base_summary_artifact_id: Option<String> = None;
                let mut base_to_seq: u64 = 0;
                if cut.to_seq > 0 {
                    let mut search_max = cut.to_seq.saturating_sub(1);
                    while search_max > 0 {
                        let cache_best = self
                            .stream_cache
                            .latest_compaction_checkpoint_before_or_at_seq_v1(
                                thread_id, search_max,
                            );

                        match cache_best {
                            Ok(Some(event)) => {
                                let EventKind::ContinuityCompactionCheckpointCreated {
                                    summary_kind,
                                    summary_artifact_id,
                                    to_seq: checkpoint_to_seq,
                                    ..
                                } = &event.kind
                                else {
                                    break;
                                };
                                if summary_kind == COMPACTION_SUMMARY_KIND_CUMULATIVE_V1 {
                                    base_summary_artifact_id = Some(summary_artifact_id.clone());
                                    base_to_seq = *checkpoint_to_seq;
                                    break;
                                }
                                search_max = checkpoint_to_seq.saturating_sub(1);
                            }
                            Ok(None) | Err(_) => {
                                let mut best_to_seq: u64 = 0;
                                let mut best_event_seq: u64 = 0;
                                for event in &continuity_events {
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
                                    if *checkpoint_to_seq >= cut.to_seq {
                                        continue;
                                    }
                                    if *checkpoint_to_seq > best_to_seq
                                        || (*checkpoint_to_seq == best_to_seq
                                            && event.seq > best_event_seq)
                                    {
                                        best_to_seq = *checkpoint_to_seq;
                                        best_event_seq = event.seq;
                                        base_summary_artifact_id =
                                            Some(summary_artifact_id.clone());
                                    }
                                }
                                base_to_seq = best_to_seq;
                                break;
                            }
                        }
                    }
                }

                let mut base_summary_markdown: Option<String> = None;
                let mut bootstrap = base_summary_artifact_id.is_none();
                let mut basis_note: Option<String> = None;

                if let Some(base_id) = base_summary_artifact_id.as_deref() {
                    match read_compaction_summary_v1(&self.workspace_root, base_id) {
                        Ok(summary) => {
                            let markdown = summary.summary_markdown().to_string();
                            if summary_markdown_is_legacy_metadata_placeholder(&markdown) {
                                bootstrap = true;
                                basis_note =
                                    Some("bootstrap_from_truth_v0.2/legacy_base".to_string());
                            } else {
                                base_summary_markdown = Some(markdown);
                            }
                        }
                        Err(err) => {
                            bootstrap = true;
                            basis_note =
                                Some(format!("bootstrap_from_truth_v0.2/base_read_failed: {err}"));
                        }
                    }
                }

                let start_seq_exclusive = if bootstrap { 0 } else { base_to_seq };
                let start_idx = upper_bound_message_seq(&messages, start_seq_exclusive);
                let end_idx = upper_bound_message_seq(&messages, cut.to_seq);
                let Some(last) = messages.get(end_idx.saturating_sub(1)) else {
                    return Err(format!(
                        "compaction cut point message not found: to_seq={} to_message_id={}",
                        cut.to_seq, cut.to_message_id
                    ));
                };
                if last.seq != cut.to_seq || last.id != cut.to_message_id {
                    return Err(format!(
                        "compaction cut point message mismatch: expected to_seq={} to_message_id={}, got to_seq={} to_message_id={}",
                        cut.to_seq,
                        cut.to_message_id,
                        last.seq,
                        last.id
                    ));
                }

                let mut acc = AutoSummaryAccumulator::default();
                for msg in messages[start_idx..end_idx].iter() {
                    acc.observe_message(msg.actor_id, msg.content);
                }
                let delta = acc.finish();

                let summary_markdown =
                    render_auto_compaction_summary_markdown_v0_2(RenderAutoSummaryMarkdownParams {
                        thread_id,
                        cut_rule_id,
                        stride_messages,
                        target_message_ordinal: cut.target_message_ordinal,
                        to_seq: cut.to_seq,
                        to_message_id: &cut.to_message_id,
                        base_summary_artifact_id: base_summary_artifact_id.as_deref(),
                        base_summary_markdown: base_summary_markdown.as_deref(),
                        basis_note: basis_note.as_deref(),
                        delta,
                        bootstrap,
                    });

                let summary = CompactionSummaryV1::new_cumulative_source_cut(
                    crate::compaction_summary::NewCumulativeCompactionSummaryV1 {
                        thread_id: thread_id.to_string(),
                        to_seq: cut.to_seq,
                        to_message_id: Some(cut.to_message_id.clone()),
                        actor_id: actor_id.to_string(),
                        origin: origin.to_string(),
                        produced_by: Some(("job".to_string(), job_id.to_string())),
                        base_summary_artifact_id,
                        basis_note,
                        summary_markdown,
                    },
                );
                let summary_artifact_id =
                    write_compaction_summary_v1(&self.workspace_root, &summary)?;

                let checkpoint_id = self.append_compaction_checkpoint_created(
                    thread_id,
                    CompactionCheckpointCreatedPayload {
                        cut_rule_id: cut_rule_id.to_string(),
                        summary_kind: COMPACTION_SUMMARY_KIND_CUMULATIVE_V1.to_string(),
                        summary_artifact_id: summary_artifact_id.clone(),
                        from_seq: 0,
                        from_message_id: None,
                        to_seq: cut.to_seq,
                        to_message_id: Some(cut.to_message_id.clone()),
                        actor_id: actor_id.to_string(),
                        origin: origin.to_string(),
                    },
                )?;

                created.push(CompactionAutoResultCheckpointV1 {
                    checkpoint_id,
                    summary_artifact_id,
                    to_seq: cut.to_seq,
                    to_message_id: cut.to_message_id.clone(),
                    cut_rule_id: cut_rule_id.to_string(),
                });
            }
            Ok(())
        })();

        match run_result {
            Ok(()) => {
                let result_created = created.clone();
                let result = serde_json::json!({
                    "schema": "rip.job_result.compaction_summarizer.v1",
                    "created": result_created,
                });
                self.append_job_ended(
                    thread_id,
                    JobEndedPayload {
                        job_id: job_id.to_string(),
                        job_kind: COMPACTION_JOB_KIND_SUMMARIZER_V1.to_string(),
                        status: "completed".to_string(),
                        result: Some(result),
                        error: None,
                        actor_id: actor_id.to_string(),
                        origin: origin.to_string(),
                    },
                )?;
                Ok(created)
            }
            Err(err) => {
                let result_created = created.clone();
                let result = serde_json::json!({
                    "schema": "rip.job_result.compaction_summarizer.v1",
                    "created": result_created,
                });
                let _ = self.append_job_ended(
                    thread_id,
                    JobEndedPayload {
                        job_id: job_id.to_string(),
                        job_kind: COMPACTION_JOB_KIND_SUMMARIZER_V1.to_string(),
                        status: "failed".to_string(),
                        result: Some(result),
                        error: Some(err.clone()),
                        actor_id: actor_id.to_string(),
                        origin: origin.to_string(),
                    },
                );
                Err(err)
            }
        }
    }
}
