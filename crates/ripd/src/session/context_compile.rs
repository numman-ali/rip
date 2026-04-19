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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compaction_summary::{
        write_compaction_summary_v1, CompactionSummaryV1, NewCumulativeCompactionSummaryV1,
    };
    use crate::context_bundle::{
        ContextBundleCompilerV1, ContextBundleItemV1, ContextBundleProvenanceV1,
        ContextBundleSourceV1, ContextBundleV1,
    };
    use rip_kernel::{Event, EventKind};
    use tempfile::tempdir;

    fn continuity_store_for_context_compile(
        dir: &tempfile::TempDir,
    ) -> (Arc<EventLog>, ContinuityStore, PathBuf) {
        let data_dir = dir.path().join("data");
        let workspace_root = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_root).expect("workspace");
        let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
        let store = ContinuityStore::new(data_dir.clone(), workspace_root, event_log.clone())
            .expect("store");
        (event_log, store, data_dir)
    }

    #[test]
    fn openresponses_items_from_context_bundle_expands_summary_refs_and_reports_missing_artifacts()
    {
        let dir = tempdir().expect("tmp");
        let workspace_root = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_root).expect("workspace");

        let summary =
            CompactionSummaryV1::new_cumulative_source_cut(NewCumulativeCompactionSummaryV1 {
                thread_id: "thread-1".to_string(),
                to_seq: 7,
                to_message_id: Some("m7".to_string()),
                actor_id: "alice".to_string(),
                origin: "cli".to_string(),
                produced_by: None,
                base_summary_artifact_id: None,
                basis_note: None,
                summary_markdown: "summary body".to_string(),
            });
        let artifact_id =
            write_compaction_summary_v1(&workspace_root, &summary).expect("write summary");

        let bundle = ContextBundleV1::new(
            ContextBundleCompilerV1 {
                id: "rip.context_compiler.v1".to_string(),
                strategy: "summaries_recent_messages_v1".to_string(),
            },
            ContextBundleSourceV1 {
                thread_id: "thread-1".to_string(),
                from_seq: 7,
                from_message_id: Some("m7".to_string()),
            },
            ContextBundleProvenanceV1 {
                run_session_id: "run-1".to_string(),
                actor_id: "alice".to_string(),
                origin: "cli".to_string(),
            },
            vec![
                ContextBundleItemV1::Message {
                    role: "user".to_string(),
                    content: "hello".to_string(),
                    actor_id: Some("alice".to_string()),
                    origin: Some("cli".to_string()),
                    thread_seq: Some(7),
                    thread_event_id: Some("m7".to_string()),
                },
                ContextBundleItemV1::SummaryRef {
                    artifact_id: artifact_id.clone(),
                    note: Some("carry this forward".to_string()),
                },
            ],
        );

        let items =
            openresponses_items_from_context_bundle(&workspace_root, &bundle).expect("items");
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|item| item.errors().is_empty()));
        assert_eq!(
            items[0]
                .value()
                .get("role")
                .and_then(|value| value.as_str()),
            Some("user")
        );
        assert_eq!(
            items[1]
                .value()
                .get("role")
                .and_then(|value| value.as_str()),
            Some("system")
        );
        let summary_content = items[1]
            .value()
            .get("content")
            .and_then(|value| value.as_str())
            .expect("summary content");
        assert!(summary_content.contains("Compaction summary (earlier context)"));
        assert!(summary_content.contains("carry this forward"));
        assert!(summary_content.contains("summary body"));

        let missing = ContextBundleV1::new(
            ContextBundleCompilerV1 {
                id: "rip.context_compiler.v1".to_string(),
                strategy: "summaries_recent_messages_v1".to_string(),
            },
            ContextBundleSourceV1 {
                thread_id: "thread-1".to_string(),
                from_seq: 7,
                from_message_id: Some("m7".to_string()),
            },
            ContextBundleProvenanceV1 {
                run_session_id: "run-1".to_string(),
                actor_id: "alice".to_string(),
                origin: "cli".to_string(),
            },
            vec![ContextBundleItemV1::SummaryRef {
                artifact_id: "missing".to_string(),
                note: None,
            }],
        );
        let err = openresponses_items_from_context_bundle(&workspace_root, &missing)
            .expect_err("missing artifact should fail");
        assert!(err.contains("artifact read failed"));
    }

    #[test]
    fn compile_context_bundle_for_run_ignores_unsupported_checkpoint_kind_and_records_reset() {
        let dir = tempdir().expect("tmp");
        let (event_log, store, data_dir) = continuity_store_for_context_compile(&dir);
        let snapshot_dir = dir.path().join("snapshots");
        std::fs::create_dir_all(&snapshot_dir).expect("snapshots");

        let continuity_id = store.ensure_default().expect("ensure");
        let m1 = store
            .append_message(
                &continuity_id,
                "alice".to_string(),
                "cli".to_string(),
                "hello".to_string(),
            )
            .expect("append");

        event_log
            .append(&Event {
                id: "checkpoint-event".to_string(),
                session_id: continuity_id.clone(),
                timestamp_ms: 0,
                seq: 2,
                kind: EventKind::ContinuityCompactionCheckpointCreated {
                    checkpoint_id: "checkpoint-1".to_string(),
                    cut_rule_id: "manual_v1".to_string(),
                    summary_kind: "unsupported_v1".to_string(),
                    summary_artifact_id: "artifact-1".to_string(),
                    from_seq: 0,
                    from_message_id: None,
                    to_seq: 1,
                    to_message_id: Some(m1.clone()),
                    actor_id: "alice".to_string(),
                    origin: "cli".to_string(),
                },
            })
            .expect("append checkpoint event");
        std::fs::remove_dir_all(data_dir.join("continuity_streams")).expect("remove sidecars");

        let (event_log, store, _data_dir) = {
            let workspace_root = dir.path().join("workspace");
            let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
            let store =
                ContinuityStore::new(data_dir, workspace_root, event_log.clone()).expect("store");
            (event_log, store, ())
        };

        let outcome = compile_context_bundle_for_run(
            &store,
            &event_log,
            &snapshot_dir,
            &ContinuityRunLink {
                continuity_id,
                message_id: m1,
                actor_id: "alice".to_string(),
                origin: "cli".to_string(),
            },
            "run-1",
        )
        .expect("compile");

        assert_eq!(outcome.decision.compiler_strategy, "recent_messages_v1");
        assert!(outcome.decision.compaction_checkpoint.is_none());
        assert!(outcome.decision.compaction_checkpoints.is_empty());
        assert_eq!(outcome.decision.resets.len(), 1);
        assert_eq!(outcome.decision.resets[0].input, "compaction_checkpoint");
        assert_eq!(outcome.decision.resets[0].action, "ignored");
        assert_eq!(
            outcome.decision.resets[0].reason,
            "unsupported_summary_kind"
        );
        assert_eq!(
            outcome.decision.resets[0]
                .ref_
                .as_ref()
                .and_then(|value| value.get("summary_kind"))
                .and_then(|value| value.as_str()),
            Some("unsupported_v1")
        );
        assert_eq!(
            outcome
                .decision
                .reason
                .as_ref()
                .and_then(|reason| reason.get("cause"))
                .and_then(|value| value.as_str()),
            Some("unsupported_compaction_summary_kind")
        );
        assert!(outcome
            .compiled
            .items
            .iter()
            .all(|item| item.errors().is_empty()));
        assert_eq!(outcome.compiled.items.len(), 1);
    }
}
