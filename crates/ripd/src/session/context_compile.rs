use super::*;

fn openresponses_items_from_context_bundle(
    workspace_root: &Path,
    bundle: &ContextBundleV1,
) -> Result<Vec<ItemParam>, String> {
    let mut out = Vec::new();
    for item in bundle.items() {
        match item {
            ContextBundleItemV1::Message { role, content, .. } => {
                out.push(ItemParam::message_text(role.clone(), content.clone()));
            }
            ContextBundleItemV1::SummaryRef { artifact_id, note } => {
                let summary = read_compaction_summary_v1(workspace_root, artifact_id)?;
                let mut content = String::new();
                content.push_str("Compaction summary (earlier context)\n");
                if let Some(note) = note.as_ref() {
                    content.push_str(note);
                    content.push('\n');
                }
                content.push_str(summary.summary_markdown());
                out.push(ItemParam::message_text("system".to_string(), content));
            }
        }
    }
    Ok(out)
}

pub(super) struct CompiledContextForRun {
    pub(super) bundle_artifact_id: String,
    pub(super) items: Vec<ItemParam>,
    pub(super) from_seq: u64,
    pub(super) from_message_id: Option<String>,
}

pub(super) struct ContextSelectionDecisionForRun {
    pub(super) compiler_id: String,
    pub(super) compiler_strategy: String,
    pub(super) limits: Value,
    pub(super) compaction_checkpoint: Option<rip_kernel::ContextSelectionCompactionCheckpointV1>,
    pub(super) compaction_checkpoints: Vec<rip_kernel::ContextSelectionCompactionCheckpointV1>,
    pub(super) resets: Vec<rip_kernel::ContextSelectionResetV1>,
    pub(super) reason: Option<Value>,
}

pub(super) struct ContextCompileOutcomeForRun {
    pub(super) decision: ContextSelectionDecisionForRun,
    pub(super) compiled: CompiledContextForRun,
}

pub(super) fn compile_context_bundle_for_run(
    continuities: &ContinuityStore,
    event_log: &EventLog,
    snapshot_dir: &Path,
    run: &ContinuityRunLink,
    run_session_id: &str,
) -> Result<ContextCompileOutcomeForRun, String> {
    let input = continuities
        .load_context_compile_input_recent_messages_v1(&run.continuity_id, &run.message_id)?;

    let limits = serde_json::json!({
        "recent_messages_v1_limit": RECENT_MESSAGES_V1_LIMIT,
        "hierarchical_summaries_v1_max_refs": HIERARCHICAL_SUMMARIES_V1_MAX_REFS,
    });

    let checkpoint_hierarchy: Vec<CompactionCheckpointForCompile> = continuities
        .hierarchical_compaction_checkpoints_for_compile_v1(
            &run.continuity_id,
            input.from_seq,
            HIERARCHICAL_SUMMARIES_V1_MAX_REFS,
        )?;

    let latest_checkpoint_any: Option<CompactionCheckpointForCompile> = continuities
        .latest_compaction_checkpoint_for_compile_v1(&run.continuity_id, input.from_seq)?;

    let mut resets: Vec<rip_kernel::ContextSelectionResetV1> = Vec::new();
    let (strategy, reason) = if checkpoint_hierarchy.is_empty() {
        if let Some(checkpoint) = latest_checkpoint_any.as_ref() {
            if checkpoint.summary_kind != COMPACTION_SUMMARY_KIND_CUMULATIVE_V1 {
                resets.push(rip_kernel::ContextSelectionResetV1 {
                    input: "compaction_checkpoint".to_string(),
                    action: "ignored".to_string(),
                    reason: "unsupported_summary_kind".to_string(),
                    ref_: Some(serde_json::json!({
                        "summary_kind": checkpoint.summary_kind,
                    })),
                });
                (
                    CONTEXT_COMPILER_STRATEGY_RECENT_MESSAGES_V1,
                    Some(serde_json::json!({
                        "selected": CONTEXT_COMPILER_STRATEGY_RECENT_MESSAGES_V1,
                        "cause": "unsupported_compaction_summary_kind",
                    })),
                )
            } else {
                (
                    CONTEXT_COMPILER_STRATEGY_RECENT_MESSAGES_V1,
                    Some(serde_json::json!({
                        "selected": CONTEXT_COMPILER_STRATEGY_RECENT_MESSAGES_V1,
                        "cause": "no_supported_compaction_checkpoint",
                    })),
                )
            }
        } else {
            (
                CONTEXT_COMPILER_STRATEGY_RECENT_MESSAGES_V1,
                Some(serde_json::json!({
                    "selected": CONTEXT_COMPILER_STRATEGY_RECENT_MESSAGES_V1,
                    "cause": "no_compaction_checkpoint",
                })),
            )
        }
    } else if checkpoint_hierarchy.len() >= 2 {
        (
            CONTEXT_COMPILER_STRATEGY_HIERARCHICAL_SUMMARIES_RECENT_MESSAGES_V1,
            Some(serde_json::json!({
                "selected": CONTEXT_COMPILER_STRATEGY_HIERARCHICAL_SUMMARIES_RECENT_MESSAGES_V1,
                "cause": "compaction_checkpoint_hierarchy",
                "levels": checkpoint_hierarchy.len(),
                "to_seqs": checkpoint_hierarchy.iter().map(|c| c.to_seq).collect::<Vec<_>>(),
            })),
        )
    } else {
        (
            CONTEXT_COMPILER_STRATEGY_SUMMARIES_RECENT_MESSAGES_V1,
            Some(serde_json::json!({
                "selected": CONTEXT_COMPILER_STRATEGY_SUMMARIES_RECENT_MESSAGES_V1,
                "cause": "compaction_checkpoint",
            })),
        )
    };

    let mut compaction_checkpoints: Vec<rip_kernel::ContextSelectionCompactionCheckpointV1> =
        Vec::new();
    for checkpoint in &checkpoint_hierarchy {
        compaction_checkpoints.push(rip_kernel::ContextSelectionCompactionCheckpointV1 {
            checkpoint_id: checkpoint.checkpoint_id.clone(),
            summary_kind: checkpoint.summary_kind.clone(),
            summary_artifact_id: checkpoint.summary_artifact_id.clone(),
            to_seq: checkpoint.to_seq,
        });
    }
    let compaction_checkpoint = compaction_checkpoints.last().cloned();

    let bundle = match strategy {
        CONTEXT_COMPILER_STRATEGY_HIERARCHICAL_SUMMARIES_RECENT_MESSAGES_V1 => {
            let summaries: Vec<HierarchicalSummaryRefV1> = checkpoint_hierarchy
                .iter()
                .map(|checkpoint| HierarchicalSummaryRefV1 {
                    artifact_id: checkpoint.summary_artifact_id.clone(),
                    to_seq: checkpoint.to_seq,
                })
                .collect();
            compile_hierarchical_summaries_recent_messages_v1(
                CompileHierarchicalSummariesRecentMessagesV1Request {
                    continuity_id: &run.continuity_id,
                    continuity_events: &input.continuity_events,
                    event_log,
                    snapshot_dir,
                    from_seq: input.from_seq,
                    from_message_id: input.from_message_id.clone(),
                    run_session_id,
                    actor_id: &run.actor_id,
                    origin: &run.origin,
                    summaries,
                },
            )?
        }
        CONTEXT_COMPILER_STRATEGY_SUMMARIES_RECENT_MESSAGES_V1 => {
            let checkpoint = checkpoint_hierarchy.last().ok_or_else(|| {
                "missing compaction checkpoint for summaries strategy".to_string()
            })?;
            compile_summaries_recent_messages_v1(CompileSummariesRecentMessagesV1Request {
                continuity_id: &run.continuity_id,
                continuity_events: &input.continuity_events,
                event_log,
                snapshot_dir,
                from_seq: input.from_seq,
                from_message_id: input.from_message_id.clone(),
                run_session_id,
                actor_id: &run.actor_id,
                origin: &run.origin,
                summary_artifact_id: &checkpoint.summary_artifact_id,
                summary_to_seq: checkpoint.to_seq,
            })?
        }
        _ => compile_recent_messages_v1(CompileRecentMessagesV1Request {
            continuity_id: &run.continuity_id,
            continuity_events: &input.continuity_events,
            event_log,
            snapshot_dir,
            from_seq: input.from_seq,
            from_message_id: input.from_message_id.clone(),
            run_session_id,
            actor_id: &run.actor_id,
            origin: &run.origin,
        })?,
    };

    let artifact_id = write_bundle_v1(continuities.workspace_root(), &bundle)?;
    let items = openresponses_items_from_context_bundle(continuities.workspace_root(), &bundle)?;
    Ok(ContextCompileOutcomeForRun {
        decision: ContextSelectionDecisionForRun {
            compiler_id: CONTEXT_COMPILER_ID_V1.to_string(),
            compiler_strategy: strategy.to_string(),
            limits,
            compaction_checkpoint,
            compaction_checkpoints,
            resets,
            reason,
        },
        compiled: CompiledContextForRun {
            bundle_artifact_id: artifact_id,
            items,
            from_seq: input.from_seq,
            from_message_id: input.from_message_id,
        },
    })
}
