use super::compile::resolve_context_compile_cutpoint_full;
use super::*;
use crate::context_compiler::{
    compile_recent_messages_v1, compile_summaries_recent_messages_v1,
    CompileRecentMessagesV1Request, CompileSummariesRecentMessagesV1Request,
};
use rip_kernel::StreamKind;
use rip_log::write_snapshot;
use tempfile::tempdir;

fn store_for(dir: &tempfile::TempDir) -> (Arc<EventLog>, ContinuityStore, PathBuf) {
    let data_dir = dir.path().join("data");
    let workspace_root = dir.path().join("workspace");
    fs::create_dir_all(&workspace_root).expect("workspace");
    let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
    let store =
        ContinuityStore::new(data_dir.clone(), workspace_root, event_log.clone()).expect("store");
    (event_log, store, data_dir)
}

#[test]
fn ensure_default_creates_and_is_idempotent() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, data_dir) = store_for(&dir);

    let first = store.ensure_default().expect("ensure");
    let second = store.ensure_default().expect("ensure");
    assert_eq!(first, second);

    let index = fs::read_to_string(index_path(&data_dir)).expect("index file");
    assert!(index.contains(&first));

    let events = event_log
        .replay_stream(StreamKind::Continuity, &first)
        .expect("replay");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].seq, 0);
    match &events[0].kind {
        EventKind::ContinuityCreated { workspace, .. } => {
            assert!(!workspace.is_empty());
        }
        other => panic!("expected continuity_created, got {other:?}"),
    }
}

#[test]
fn provider_cursor_status_survives_sidecar_rotation() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, data_dir) = store_for(&dir);

    let thread_id = store.ensure_default().expect("ensure");
    store
        .append_provider_cursor_updated(
            &thread_id,
            ProviderCursorUpdatedPayload {
                provider: "openresponses".to_string(),
                endpoint: Some("http://example.test/v1/responses".to_string()),
                model: Some("fixture-model".to_string()),
                cursor: Some(serde_json::json!({
                    "previous_response_id": "resp_1"
                })),
                action: "set".to_string(),
                reason: Some("test".to_string()),
                run_session_id: Some("session-1".to_string()),
                actor_id: "user".to_string(),
                origin: "test".to_string(),
            },
        )
        .expect("append cursor");

    let first = store
        .provider_cursor_status_v1(&thread_id, ProviderCursorStatusV1Request {})
        .expect("status");
    assert_eq!(first.thread_id, thread_id);
    let active = first.active.expect("active");
    assert_eq!(active.action, "set");
    assert_eq!(
        active
            .cursor
            .as_ref()
            .and_then(|value| value.get("previous_response_id"))
            .and_then(|value| value.as_str()),
        Some("resp_1")
    );

    let _ = std::fs::remove_dir_all(data_dir.join("continuity_streams"));

    let second = store
        .provider_cursor_status_v1(&thread_id, ProviderCursorStatusV1Request {})
        .expect("status after cache delete");
    let active = second.active.expect("active");
    assert_eq!(active.action, "set");
    assert_eq!(
        active
            .cursor
            .as_ref()
            .and_then(|value| value.get("previous_response_id"))
            .and_then(|value| value.as_str()),
        Some("resp_1")
    );

    let rotated = store
        .provider_cursor_rotate_v1(
            &thread_id,
            ProviderCursorRotateV1Request {
                provider: None,
                endpoint: None,
                model: None,
                reason: Some("manual".to_string()),
                actor_id: "user".to_string(),
                origin: "test".to_string(),
            },
        )
        .expect("rotate");
    assert!(rotated.rotated);

    let status = store
        .provider_cursor_status_v1(&thread_id, ProviderCursorStatusV1Request {})
        .expect("status after rotate");
    let active = status.active.expect("active");
    assert_eq!(active.action, "rotated");
    assert!(active.cursor.is_none());
}

#[test]
fn context_selection_status_survives_sidecar_rotation_and_orders_latest_first() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, data_dir) = store_for(&dir);

    let thread_id = store.ensure_default().expect("ensure");

    let m1 = store
        .append_message(
            &thread_id,
            "alice".to_string(),
            "cli".to_string(),
            "m1".to_string(),
        )
        .expect("append message");
    store
        .append_run_spawned(
            &thread_id,
            &m1,
            "session-1",
            "alice".to_string(),
            "cli".to_string(),
        )
        .expect("run spawned");
    store
        .append_context_selection_decided(
            &thread_id,
            ContextSelectionDecidedPayload {
                run_session_id: "session-1".to_string(),
                message_id: m1.clone(),
                compiler_id: "rip.context_compiler.v1".to_string(),
                compiler_strategy: "recent_messages_v1".to_string(),
                limits: serde_json::json!({ "recent_messages_v1_limit": 16 }),
                compaction_checkpoint: None,
                compaction_checkpoints: Vec::new(),
                resets: Vec::new(),
                reason: Some(serde_json::json!({
                    "selected": "recent_messages_v1",
                    "cause": "test",
                })),
                actor_id: "alice".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("selection decided");

    let m2 = store
        .append_message(
            &thread_id,
            "alice".to_string(),
            "cli".to_string(),
            "m2".to_string(),
        )
        .expect("append message");
    store
        .append_run_spawned(
            &thread_id,
            &m2,
            "session-2",
            "alice".to_string(),
            "cli".to_string(),
        )
        .expect("run spawned");
    store
        .append_context_selection_decided(
            &thread_id,
            ContextSelectionDecidedPayload {
                run_session_id: "session-2".to_string(),
                message_id: m2.clone(),
                compiler_id: "rip.context_compiler.v1".to_string(),
                compiler_strategy: "summaries_recent_messages_v1".to_string(),
                limits: serde_json::json!({ "recent_messages_v1_limit": 16 }),
                compaction_checkpoint: Some(rip_kernel::ContextSelectionCompactionCheckpointV1 {
                    checkpoint_id: "ckpt-1".to_string(),
                    summary_kind: "cumulative_v1".to_string(),
                    summary_artifact_id: "artifact-1".to_string(),
                    to_seq: 1,
                }),
                compaction_checkpoints: vec![rip_kernel::ContextSelectionCompactionCheckpointV1 {
                    checkpoint_id: "ckpt-1".to_string(),
                    summary_kind: "cumulative_v1".to_string(),
                    summary_artifact_id: "artifact-1".to_string(),
                    to_seq: 1,
                }],
                resets: Vec::new(),
                reason: Some(serde_json::json!({
                    "selected": "summaries_recent_messages_v1",
                    "cause": "compaction_checkpoint",
                })),
                actor_id: "alice".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("selection decided");

    let status = store
        .context_selection_status_v1(
            &thread_id,
            ContextSelectionStatusV1Request { limit: Some(2) },
        )
        .expect("status");
    assert_eq!(status.thread_id, thread_id);
    assert_eq!(status.decisions.len(), 2);
    assert_eq!(status.decisions[0].run_session_id, "session-2");
    assert_eq!(status.decisions[0].message_id, m2);
    assert_eq!(
        status.decisions[0].compiler_strategy,
        "summaries_recent_messages_v1"
    );
    assert!(status.decisions[0].compaction_checkpoint.is_some());
    assert_eq!(status.decisions[1].run_session_id, "session-1");
    assert_eq!(status.decisions[1].message_id, m1);
    assert_eq!(status.decisions[1].compiler_strategy, "recent_messages_v1");

    let _ = std::fs::remove_dir_all(data_dir.join("continuity_streams"));

    let status = store
        .context_selection_status_v1(
            &thread_id,
            ContextSelectionStatusV1Request { limit: Some(1) },
        )
        .expect("status after cache delete");
    assert_eq!(status.decisions.len(), 1);
    assert_eq!(status.decisions[0].run_session_id, "session-2");
}

#[test]
fn continuity_sidecar_contains_appended_frames_and_is_preferred_for_replay() {
    use std::io::Write;

    let dir = tempdir().expect("tmp");
    let (_event_log, store, data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    let message_id = store
        .append_message(
            &continuity_id,
            "alice".to_string(),
            "cli".to_string(),
            "hello".to_string(),
        )
        .expect("append message");
    store
        .append_run_spawned(
            &continuity_id,
            &message_id,
            "session-1",
            "alice".to_string(),
            "cli".to_string(),
        )
        .expect("run spawned");
    store
        .append_context_selection_decided(
            &continuity_id,
            ContextSelectionDecidedPayload {
                run_session_id: "session-1".to_string(),
                message_id: message_id.clone(),
                compiler_id: "rip.context_compiler.v1".to_string(),
                compiler_strategy: "recent_messages_v1".to_string(),
                limits: serde_json::json!({ "recent_messages_v1_limit": 16 }),
                compaction_checkpoint: None,
                compaction_checkpoints: Vec::new(),
                resets: Vec::new(),
                reason: Some(serde_json::json!({
                    "selected": "recent_messages_v1",
                    "cause": "test",
                })),
                actor_id: "alice".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("context selection decided");
    store
        .append_context_compiled(
            &continuity_id,
            ContextCompiledPayload {
                run_session_id: "session-1".to_string(),
                bundle_artifact_id: "artifact-1".to_string(),
                compiler_id: "rip.context_compiler.v1".to_string(),
                compiler_strategy: "recent_messages_v1".to_string(),
                from_seq: 1,
                from_message_id: Some(message_id.clone()),
                actor_id: "alice".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("context compiled");
    store
        .append_run_ended(
            &continuity_id,
            &message_id,
            "session-1",
            "completed".to_string(),
            "alice".to_string(),
            "cli".to_string(),
        )
        .expect("run ended");

    let sidecar_path = data_dir
        .join("continuity_streams")
        .join(format!("{continuity_id}.jsonl"));
    assert!(sidecar_path.exists(), "expected continuity sidecar file");
    let sidecar = fs::read_to_string(&sidecar_path).expect("read sidecar");
    assert!(
        sidecar.contains("continuity_context_compiled"),
        "expected continuity_context_compiled in sidecar"
    );
    assert!(
        sidecar.contains("continuity_context_selection_decided"),
        "expected continuity_context_selection_decided in sidecar"
    );

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(data_dir.join("events.jsonl"))
        .expect("open global log");
    writeln!(file, "not json").expect("write corrupt line");

    let events = store.replay_events(&continuity_id).expect("replay");
    assert!(
        events
            .iter()
            .any(|event| matches!(event.kind, EventKind::ContinuityContextCompiled { .. })),
        "expected continuity_context_compiled in replay"
    );
    assert!(
        events.iter().any(|event| matches!(
            event.kind,
            EventKind::ContinuityContextSelectionDecided { .. }
        )),
        "expected continuity_context_selection_decided in replay"
    );
}

#[test]
fn ensure_default_recovers_from_missing_index() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, data_dir) = store_for(&dir);

    let first = store.ensure_default().expect("ensure");
    fs::remove_file(index_path(&data_dir)).expect("remove index");

    let (_event_log2, store2, _data_dir2) = store_for(&dir);
    let second = store2.ensure_default().expect("ensure");
    assert_eq!(first, second);
}

#[test]
fn append_message_increments_seq() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    let m1 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "hello".to_string(),
        )
        .expect("append");
    let m2 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "world".to_string(),
        )
        .expect("append");
    assert_ne!(m1, m2);

    let events = event_log
        .replay_stream(StreamKind::Continuity, &continuity_id)
        .expect("replay");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].seq, 0);
    assert_eq!(events[1].seq, 1);
    assert_eq!(events[2].seq, 2);
    match &events[2].kind {
        EventKind::ContinuityMessageAppended { content, .. } => assert_eq!(content, "world"),
        other => panic!("expected message, got {other:?}"),
    }
}

#[test]
fn append_run_spawned_advances_seq() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    let message_id = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "hello".to_string(),
        )
        .expect("append");
    store
        .append_run_spawned(
            &continuity_id,
            &message_id,
            "session-1",
            "user".to_string(),
            "cli".to_string(),
        )
        .expect("run spawned");

    let events = event_log
        .replay_stream(StreamKind::Continuity, &continuity_id)
        .expect("replay");
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].seq, 0);
    assert_eq!(events[1].seq, 1);
    assert_eq!(events[2].seq, 2);
    match &events[2].kind {
        EventKind::ContinuityRunSpawned {
            run_session_id,
            actor_id,
            origin,
            ..
        } => {
            assert_eq!(run_session_id, "session-1");
            assert_eq!(actor_id.as_deref(), Some("user"));
            assert_eq!(origin.as_deref(), Some("cli"));
        }
        other => panic!("expected run_spawned, got {other:?}"),
    }
}

#[test]
fn append_run_ended_advances_seq() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    let message_id = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "hello".to_string(),
        )
        .expect("append");
    store
        .append_run_spawned(
            &continuity_id,
            &message_id,
            "session-1",
            "user".to_string(),
            "cli".to_string(),
        )
        .expect("run spawned");
    store
        .append_run_ended(
            &continuity_id,
            &message_id,
            "session-1",
            "completed".to_string(),
            "user".to_string(),
            "cli".to_string(),
        )
        .expect("run ended");

    let events = event_log
        .replay_stream(StreamKind::Continuity, &continuity_id)
        .expect("replay");
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].seq, 0);
    assert_eq!(events[1].seq, 1);
    assert_eq!(events[2].seq, 2);
    assert_eq!(events[3].seq, 3);
    match &events[3].kind {
        EventKind::ContinuityRunEnded {
            run_session_id,
            message_id: mid,
            reason,
            actor_id,
            origin,
        } => {
            assert_eq!(run_session_id, "session-1");
            assert_eq!(mid, &message_id);
            assert_eq!(reason, "completed");
            assert_eq!(actor_id.as_deref(), Some("user"));
            assert_eq!(origin.as_deref(), Some("cli"));
        }
        other => panic!("expected run_ended, got {other:?}"),
    }
}

#[test]
fn append_message_recovers_seq_from_sidecar_when_next_seq_cache_missing() {
    use std::io::Write;

    let dir = tempdir().expect("tmp");
    let (_event_log, store, data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "first".to_string(),
        )
        .expect("append");

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(data_dir.join("events.jsonl"))
        .expect("open global log");
    writeln!(file, "not json").expect("corrupt global log");

    let (_event_log2, store2, _data_dir2) = store_for(&dir);
    store2
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "second".to_string(),
        )
        .expect("append after restart");
}

#[test]
fn compaction_checkpoint_cumulative_v1_writes_artifact_and_appends_frame() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, _data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    let m1 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "hello".to_string(),
        )
        .expect("append");
    let _m2 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "world".to_string(),
        )
        .expect("append");

    let (checkpoint_id, summary_artifact_id, to_seq, to_message_id, cut_rule_id) = store
        .compaction_checkpoint_cumulative_v1(
            &continuity_id,
            CompactionCheckpointCumulativeV1Request {
                summary_markdown: Some("summary".to_string()),
                summary_artifact_id: None,
                to_message_id: Some(m1.clone()),
                to_seq: None,
                stride_messages: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("checkpoint");

    assert_eq!(to_message_id, m1);
    assert_eq!(cut_rule_id, "manual_v1");

    let blob_path = store
        .workspace_root()
        .join(".rip")
        .join("artifacts")
        .join("blobs")
        .join(&summary_artifact_id);
    assert!(blob_path.exists(), "summary artifact blob should exist");

    let events = store.replay_events(&continuity_id).expect("replay");
    let checkpoint_event = events
        .iter()
        .find(|event| event.id == checkpoint_id)
        .expect("checkpoint event");
    match &checkpoint_event.kind {
        EventKind::ContinuityCompactionCheckpointCreated {
            checkpoint_id: cid,
            cut_rule_id: rule,
            summary_kind,
            summary_artifact_id: aid,
            from_seq,
            to_seq: t_seq,
            to_message_id: t_mid,
            actor_id,
            origin,
            ..
        } => {
            assert_eq!(cid, &checkpoint_id);
            assert_eq!(rule, "manual_v1");
            assert_eq!(summary_kind, COMPACTION_SUMMARY_KIND_CUMULATIVE_V1);
            assert_eq!(aid, &summary_artifact_id);
            assert_eq!(*from_seq, 0);
            assert_eq!(*t_seq, to_seq);
            assert_eq!(t_mid.as_deref(), Some(to_message_id.as_str()));
            assert_eq!(actor_id, "user");
            assert_eq!(origin, "cli");
        }
        other => panic!("expected compaction checkpoint frame, got {other:?}"),
    }

    let latest = store
        .latest_compaction_checkpoint_for_compile_v1(&continuity_id, checkpoint_event.seq)
        .expect("lookup")
        .expect("latest");
    assert_eq!(latest.summary_artifact_id, summary_artifact_id);
    assert_eq!(latest.to_seq, to_seq);
}

#[test]
fn latest_compaction_checkpoint_for_compile_tie_breaks_by_stream_order() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, _data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    let m1 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "hello".to_string(),
        )
        .expect("append");
    let _m2 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "world".to_string(),
        )
        .expect("append");

    let (_checkpoint1_id, summary1_artifact_id, to_seq1, _to_mid1, _cut_rule_id1) = store
        .compaction_checkpoint_cumulative_v1(
            &continuity_id,
            CompactionCheckpointCumulativeV1Request {
                summary_markdown: Some("summary-1".to_string()),
                summary_artifact_id: None,
                to_message_id: Some(m1.clone()),
                to_seq: None,
                stride_messages: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("checkpoint1");
    let (_checkpoint2_id, summary2_artifact_id, to_seq2, _to_mid2, _cut_rule_id2) = store
        .compaction_checkpoint_cumulative_v1(
            &continuity_id,
            CompactionCheckpointCumulativeV1Request {
                summary_markdown: Some("summary-2".to_string()),
                summary_artifact_id: None,
                to_message_id: Some(m1.clone()),
                to_seq: None,
                stride_messages: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("checkpoint2");

    assert_eq!(to_seq1, to_seq2);
    assert_ne!(summary1_artifact_id, summary2_artifact_id);

    let events = store.replay_events(&continuity_id).expect("replay");
    let from_seq = events.last().map(|event| event.seq).unwrap_or_default();

    let latest = store
        .latest_compaction_checkpoint_for_compile_v1(&continuity_id, from_seq)
        .expect("lookup")
        .expect("some");
    assert_eq!(latest.to_seq, to_seq1);
    assert_eq!(latest.summary_artifact_id, summary2_artifact_id);
}

#[test]
fn compaction_cut_points_v1_falls_back_when_ordinal_index_missing() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    let _m1 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m1".to_string(),
        )
        .expect("append");
    let m2 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m2".to_string(),
        )
        .expect("append");
    let _m3 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m3".to_string(),
        )
        .expect("append");
    let m4 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m4".to_string(),
        )
        .expect("append");

    let req = CompactionCutPointsV1Request {
        stride_messages: Some(2),
        limit: Some(2),
    };
    let first = store
        .compaction_cut_points_v1(&continuity_id, req.clone())
        .expect("cut points");
    assert_eq!(first.message_count, 4);
    assert_eq!(first.cut_points.len(), 2);
    assert_eq!(first.cut_points[0].target_message_ordinal, 4);
    assert_eq!(first.cut_points[0].to_message_id, m4);
    assert_eq!(first.cut_points[1].target_message_ordinal, 2);
    assert_eq!(first.cut_points[1].to_message_id, m2);

    let ord_path = data_dir
        .join("continuity_streams")
        .join(format!("{continuity_id}.mr.msgord.v1.bin"));
    assert!(ord_path.exists(), "expected ordinal index to exist");
    fs::remove_file(&ord_path).expect("remove ordinal index");

    let second = store
        .compaction_cut_points_v1(&continuity_id, req)
        .expect("cut points after deleting ordinal index");
    assert_eq!(second.message_count, 4);
    assert_eq!(second.cut_points.len(), 2);
    assert_eq!(second.cut_points[0].target_message_ordinal, 4);
    assert_eq!(second.cut_points[0].to_message_id, m4);
    assert_eq!(second.cut_points[1].target_message_ordinal, 2);
    assert_eq!(second.cut_points[1].to_message_id, m2);
}

#[test]
fn compaction_auto_schedule_is_replay_safe_under_concurrent_calls() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);
    let store = Arc::new(store);

    let continuity_id = store.ensure_default().expect("ensure");
    let _m1 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m1".to_string(),
        )
        .expect("append");
    let _m2 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m2".to_string(),
        )
        .expect("append");

    std::thread::scope(|scope| {
        for _ in 0..4 {
            let store = store.clone();
            let continuity_id = continuity_id.clone();
            scope.spawn(move || {
                let _ = store.compaction_auto_schedule_spawn_job_v1(
                    &continuity_id,
                    CompactionAutoScheduleV1Request {
                        stride_messages: Some(2),
                        max_new_checkpoints: Some(1),
                        block_on_inflight: Some(true),
                        execute: Some(false),
                        dry_run: Some(false),
                        actor_id: "alice".to_string(),
                        origin: "test".to_string(),
                    },
                );
            });
        }
    });

    let events = event_log
        .replay_stream(StreamKind::Continuity, &continuity_id)
        .expect("replay");
    assert!(!events.is_empty());
    for (idx, event) in events.iter().enumerate() {
        assert_eq!(event.seq, idx as u64, "expected contiguous seq values");
    }
    assert!(
        events.iter().any(|event| matches!(
            event.kind,
            EventKind::ContinuityCompactionAutoScheduleDecided { .. }
        )),
        "expected at least one continuity_compaction_auto_schedule_decided"
    );
}

#[test]
fn compaction_status_v1_reports_next_cut_point_and_latest_checkpoint() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, _data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    let _m1 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m1".to_string(),
        )
        .expect("append");
    let m2 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m2".to_string(),
        )
        .expect("append");

    let first = store
        .compaction_status_v1(
            &continuity_id,
            CompactionStatusV1Request {
                stride_messages: Some(1),
            },
        )
        .expect("status");
    assert_eq!(first.thread_id, continuity_id);
    assert_eq!(first.message_count, 2);
    assert!(first.latest_checkpoint.is_none());
    assert_eq!(
        first
            .next_cut_point
            .as_ref()
            .map(|cp| cp.to_message_id.as_str()),
        Some(m2.as_str())
    );

    store
        .compaction_checkpoint_cumulative_v1(
            &continuity_id,
            CompactionCheckpointCumulativeV1Request {
                summary_markdown: Some("summary".to_string()),
                summary_artifact_id: None,
                to_message_id: Some(m2.clone()),
                to_seq: None,
                stride_messages: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("checkpoint");

    let second = store
        .compaction_status_v1(
            &continuity_id,
            CompactionStatusV1Request {
                stride_messages: Some(1),
            },
        )
        .expect("status after checkpoint");
    assert!(second.latest_checkpoint.is_some());
    assert_eq!(
        second
            .latest_checkpoint
            .as_ref()
            .and_then(|c| c.to_message_id.as_deref()),
        Some(m2.as_str())
    );
}

#[test]
fn compaction_auto_summary_bootstraps_from_legacy_placeholder_base() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, _data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    let _m1 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m1".to_string(),
        )
        .expect("append");
    let m2 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m2".to_string(),
        )
        .expect("append");
    let _m3 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m3".to_string(),
        )
        .expect("append");
    let m4 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m4".to_string(),
        )
        .expect("append");

    let legacy_markdown = format!(
        "# Compaction summary (auto)\n\n- kind: {kind}\n- cut_rule_id: stride_messages_v1/2\n- stride_messages: 2\n- target_message_ordinal: 2\n- to_seq: 2\n- to_message_id: {m2}\n",
        kind = COMPACTION_SUMMARY_KIND_CUMULATIVE_V1
    );
    assert!(
        crate::compaction_auto_summary::summary_markdown_is_legacy_metadata_placeholder(
            &legacy_markdown
        ),
        "expected legacy placeholder detector to match"
    );
    store
        .compaction_checkpoint_cumulative_v1(
            &continuity_id,
            CompactionCheckpointCumulativeV1Request {
                summary_markdown: Some(legacy_markdown),
                summary_artifact_id: None,
                to_message_id: Some(m2.clone()),
                to_seq: None,
                stride_messages: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("seed legacy checkpoint");

    let resp = store
        .compaction_auto_v1(
            &continuity_id,
            CompactionAutoV1Request {
                stride_messages: Some(2),
                max_new_checkpoints: Some(1),
                dry_run: Some(false),
                actor_id: "alice".to_string(),
                origin: "test".to_string(),
            },
        )
        .expect("compaction auto");
    assert_eq!(resp.status, "completed");
    assert_eq!(resp.result.len(), 1);
    let artifact_id = resp.result[0].summary_artifact_id.clone();

    let summary =
        crate::compaction_summary::read_compaction_summary_v1(store.workspace_root(), &artifact_id)
            .expect("read summary artifact");
    let markdown = summary.summary_markdown();
    assert!(
        markdown.contains("## Cumulative Summary"),
        "expected v0.2 cumulative section"
    );
    assert!(
        markdown.contains("## Recent Delta Highlights"),
        "expected v0.2 highlights section"
    );
    assert!(
        markdown.contains("m4") || markdown.contains(m4.as_str()),
        "expected summary to include message content"
    );
    assert!(
        !crate::compaction_auto_summary::summary_markdown_is_legacy_metadata_placeholder(markdown),
        "expected upgraded summary to not be legacy placeholder"
    );
    assert!(
        markdown.chars().count() <= crate::compaction_auto_summary::MAX_SUMMARY_MARKDOWN_CHARS,
        "expected summary_markdown to be bounded"
    );
}

#[test]
fn tail_context_compile_input_matches_full_replay_for_recent_messages_v1() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);
    let snapshot_dir = dir.path().join("snapshots");

    let continuity_id = store.ensure_default().expect("ensure");
    let mut messages: Vec<(String, String)> = Vec::new();

    for idx in 0..20 {
        let message_id = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                format!("m{idx}:{}", "x".repeat(20_000)),
            )
            .expect("append message");
        let session_id = format!("session-{idx}");

        let session_events = vec![
            Event {
                id: format!("se{idx}-0"),
                session_id: session_id.clone(),
                timestamp_ms: 0,
                seq: 0,
                kind: EventKind::SessionStarted {
                    input: "hi".to_string(),
                },
            },
            Event {
                id: format!("se{idx}-1"),
                session_id: session_id.clone(),
                timestamp_ms: 1,
                seq: 1,
                kind: EventKind::OutputTextDelta {
                    delta: format!("a{idx}"),
                },
            },
            Event {
                id: format!("se{idx}-2"),
                session_id: session_id.clone(),
                timestamp_ms: 2,
                seq: 2,
                kind: EventKind::SessionEnded {
                    reason: "completed".to_string(),
                },
            },
        ];
        write_snapshot(&snapshot_dir, &session_id, &session_events).expect("snapshot");

        store
            .append_run_spawned(
                &continuity_id,
                &message_id,
                &session_id,
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_context_compiled(
                &continuity_id,
                ContextCompiledPayload {
                    run_session_id: session_id.clone(),
                    bundle_artifact_id: "artifact-1".to_string(),
                    compiler_id: "rip.context_compiler.v1".to_string(),
                    compiler_strategy: "recent_messages_v1".to_string(),
                    from_seq: 0,
                    from_message_id: Some(message_id.clone()),
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("context compiled");
        store
            .append_run_ended(
                &continuity_id,
                &message_id,
                &session_id,
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");

        messages.push((message_id, session_id));
    }

    let anchor_message_id = messages
        .last()
        .map(|(mid, _)| mid.clone())
        .expect("messages");

    let full_events = store.replay_events(&continuity_id).expect("replay full");
    let (full_from_seq, full_from_message_id) =
        resolve_context_compile_cutpoint_full(&full_events, &anchor_message_id).expect("cutpoint");

    let full_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
        continuity_id: &continuity_id,
        continuity_events: &full_events,
        event_log: event_log.as_ref(),
        snapshot_dir: &snapshot_dir,
        from_seq: full_from_seq,
        from_message_id: full_from_message_id.clone(),
        run_session_id: "run-session",
        actor_id: "user",
        origin: "cli",
    })
    .expect("compile full");

    let tail_input = store
        .load_context_compile_input_recent_messages_v1(&continuity_id, &anchor_message_id)
        .expect("tail input");
    assert_eq!(tail_input.from_seq, full_from_seq);
    assert_eq!(tail_input.from_message_id, full_from_message_id);

    let tail_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
        continuity_id: &continuity_id,
        continuity_events: &tail_input.continuity_events,
        event_log: event_log.as_ref(),
        snapshot_dir: &snapshot_dir,
        from_seq: tail_input.from_seq,
        from_message_id: tail_input.from_message_id.clone(),
        run_session_id: "run-session",
        actor_id: "user",
        origin: "cli",
    })
    .expect("compile tail");

    let full_json = serde_json::to_value(&full_bundle).expect("full json");
    let tail_json = serde_json::to_value(&tail_bundle).expect("tail json");
    assert_eq!(tail_json, full_json);
}

#[test]
fn window_context_compile_input_matches_full_replay_for_recent_messages_v1_non_tail_anchor() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);
    let snapshot_dir = dir.path().join("snapshots");

    const MSG_LEN: usize = 60_000;
    const MSG_COUNT: usize = 200;

    let continuity_id = store.ensure_default().expect("ensure");
    let mut message_ids: Vec<String> = Vec::new();

    for idx in 0..MSG_COUNT {
        let message_id = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                format!("m{idx}:{}", "x".repeat(MSG_LEN)),
            )
            .expect("append message");
        let session_id = format!("session-{idx}");

        let session_events = vec![
            Event {
                id: format!("se{idx}-0"),
                session_id: session_id.clone(),
                timestamp_ms: 0,
                seq: 0,
                kind: EventKind::SessionStarted {
                    input: "hi".to_string(),
                },
            },
            Event {
                id: format!("se{idx}-1"),
                session_id: session_id.clone(),
                timestamp_ms: 1,
                seq: 1,
                kind: EventKind::OutputTextDelta {
                    delta: format!("a{idx}"),
                },
            },
            Event {
                id: format!("se{idx}-2"),
                session_id: session_id.clone(),
                timestamp_ms: 2,
                seq: 2,
                kind: EventKind::SessionEnded {
                    reason: "completed".to_string(),
                },
            },
        ];
        write_snapshot(&snapshot_dir, &session_id, &session_events).expect("snapshot");

        store
            .append_run_spawned(
                &continuity_id,
                &message_id,
                &session_id,
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_context_compiled(
                &continuity_id,
                ContextCompiledPayload {
                    run_session_id: session_id.clone(),
                    bundle_artifact_id: "artifact-1".to_string(),
                    compiler_id: "rip.context_compiler.v1".to_string(),
                    compiler_strategy: "recent_messages_v1".to_string(),
                    from_seq: 0,
                    from_message_id: Some(message_id.clone()),
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("context compiled");
        store
            .append_run_ended(
                &continuity_id,
                &message_id,
                &session_id,
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");

        message_ids.push(message_id);
    }

    let anchor_message_id = message_ids.get(40).cloned().expect("anchor message id");

    let full_events = store.replay_events(&continuity_id).expect("replay full");
    let (full_from_seq, full_from_message_id) =
        resolve_context_compile_cutpoint_full(&full_events, &anchor_message_id).expect("cutpoint");

    let full_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
        continuity_id: &continuity_id,
        continuity_events: &full_events,
        event_log: event_log.as_ref(),
        snapshot_dir: &snapshot_dir,
        from_seq: full_from_seq,
        from_message_id: full_from_message_id.clone(),
        run_session_id: "run-session",
        actor_id: "user",
        origin: "cli",
    })
    .expect("compile full");

    let window_input = store
        .load_context_compile_input_recent_messages_v1(&continuity_id, &anchor_message_id)
        .expect("window input");
    assert_eq!(window_input.from_seq, full_from_seq);
    assert_eq!(window_input.from_message_id, full_from_message_id);
    assert!(
        window_input.continuity_events.len() <= 128,
        "expected bounded window, got {} events",
        window_input.continuity_events.len()
    );

    let window_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
        continuity_id: &continuity_id,
        continuity_events: &window_input.continuity_events,
        event_log: event_log.as_ref(),
        snapshot_dir: &snapshot_dir,
        from_seq: window_input.from_seq,
        from_message_id: window_input.from_message_id.clone(),
        run_session_id: "run-session",
        actor_id: "user",
        origin: "cli",
    })
    .expect("compile window");

    let full_json = serde_json::to_value(&full_bundle).expect("full json");
    let window_json = serde_json::to_value(&window_bundle).expect("window json");
    assert_eq!(window_json, full_json);
}

#[test]
fn window_context_compile_input_matches_full_replay_with_dense_tool_side_effects() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);
    let snapshot_dir = dir.path().join("snapshots");

    const MSG_COUNT: usize = 60;
    const TOOL_EVENTS_PER_MESSAGE: usize = 250;

    let continuity_id = store.ensure_default().expect("ensure");
    let mut message_ids: Vec<String> = Vec::new();

    for idx in 0..MSG_COUNT {
        let message_id = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                format!("m{idx}"),
            )
            .expect("append message");
        let session_id = format!("session-{idx}");

        let session_events = vec![
            Event {
                id: format!("se{idx}-0"),
                session_id: session_id.clone(),
                timestamp_ms: 0,
                seq: 0,
                kind: EventKind::SessionStarted {
                    input: "hi".to_string(),
                },
            },
            Event {
                id: format!("se{idx}-1"),
                session_id: session_id.clone(),
                timestamp_ms: 1,
                seq: 1,
                kind: EventKind::OutputTextDelta {
                    delta: format!("a{idx}"),
                },
            },
            Event {
                id: format!("se{idx}-2"),
                session_id: session_id.clone(),
                timestamp_ms: 2,
                seq: 2,
                kind: EventKind::SessionEnded {
                    reason: "completed".to_string(),
                },
            },
        ];
        write_snapshot(&snapshot_dir, &session_id, &session_events).expect("snapshot");

        store
            .append_run_spawned(
                &continuity_id,
                &message_id,
                &session_id,
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");

        for tool_idx in 0..TOOL_EVENTS_PER_MESSAGE {
            store
                .append_tool_side_effects(
                    &ContinuityRunLink {
                        continuity_id: continuity_id.clone(),
                        message_id: message_id.clone(),
                        actor_id: "user".to_string(),
                        origin: "cli".to_string(),
                    },
                    &session_id,
                    ToolSideEffects {
                        tool_id: format!("tool-{idx}-{tool_idx}"),
                        tool_name: "write".to_string(),
                        affected_paths: Some(vec![format!("file-{tool_idx}.txt")]),
                        checkpoint_id: None,
                    },
                )
                .expect("tool side effects");
        }

        store
            .append_run_ended(
                &continuity_id,
                &message_id,
                &session_id,
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");

        message_ids.push(message_id);
    }

    let anchor_message_id = message_ids.get(20).cloned().expect("anchor message id");

    let full_events = store.replay_events(&continuity_id).expect("replay full");
    let (full_from_seq, full_from_message_id) =
        resolve_context_compile_cutpoint_full(&full_events, &anchor_message_id).expect("cutpoint");

    let full_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
        continuity_id: &continuity_id,
        continuity_events: &full_events,
        event_log: event_log.as_ref(),
        snapshot_dir: &snapshot_dir,
        from_seq: full_from_seq,
        from_message_id: full_from_message_id.clone(),
        run_session_id: "run-session",
        actor_id: "user",
        origin: "cli",
    })
    .expect("compile full");

    let window_input = store
        .load_context_compile_input_recent_messages_v1(&continuity_id, &anchor_message_id)
        .expect("window input");
    assert_eq!(window_input.from_seq, full_from_seq);
    assert_eq!(window_input.from_message_id, full_from_message_id);
    assert!(
        window_input.continuity_events.iter().all(|event| matches!(
            event.kind,
            EventKind::ContinuityMessageAppended { .. } | EventKind::ContinuityRunEnded { .. }
        )),
        "expected message+run-ended-only window events"
    );

    let window_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
        continuity_id: &continuity_id,
        continuity_events: &window_input.continuity_events,
        event_log: event_log.as_ref(),
        snapshot_dir: &snapshot_dir,
        from_seq: window_input.from_seq,
        from_message_id: window_input.from_message_id.clone(),
        run_session_id: "run-session",
        actor_id: "user",
        origin: "cli",
    })
    .expect("compile window");

    let full_json = serde_json::to_value(&full_bundle).expect("full json");
    let window_json = serde_json::to_value(&window_bundle).expect("window json");
    assert_eq!(window_json, full_json);
}

#[test]
fn window_context_compile_input_matches_full_replay_with_dense_tool_side_effects_and_compaction_summary(
) {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);
    let snapshot_dir = dir.path().join("snapshots");

    const MSG_COUNT: usize = 60;
    const TOOL_EVENTS_PER_MESSAGE: usize = 250;

    let continuity_id = store.ensure_default().expect("ensure");
    let mut message_ids: Vec<String> = Vec::new();

    for idx in 0..MSG_COUNT {
        let message_id = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                format!("m{idx}"),
            )
            .expect("append message");
        let session_id = format!("session-{idx}");

        let session_events = vec![
            Event {
                id: format!("se{idx}-0"),
                session_id: session_id.clone(),
                timestamp_ms: 0,
                seq: 0,
                kind: EventKind::SessionStarted {
                    input: "hi".to_string(),
                },
            },
            Event {
                id: format!("se{idx}-1"),
                session_id: session_id.clone(),
                timestamp_ms: 1,
                seq: 1,
                kind: EventKind::OutputTextDelta {
                    delta: format!("a{idx}"),
                },
            },
            Event {
                id: format!("se{idx}-2"),
                session_id: session_id.clone(),
                timestamp_ms: 2,
                seq: 2,
                kind: EventKind::SessionEnded {
                    reason: "completed".to_string(),
                },
            },
        ];
        write_snapshot(&snapshot_dir, &session_id, &session_events).expect("snapshot");

        store
            .append_run_spawned(
                &continuity_id,
                &message_id,
                &session_id,
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");

        for tool_idx in 0..TOOL_EVENTS_PER_MESSAGE {
            store
                .append_tool_side_effects(
                    &ContinuityRunLink {
                        continuity_id: continuity_id.clone(),
                        message_id: message_id.clone(),
                        actor_id: "user".to_string(),
                        origin: "cli".to_string(),
                    },
                    &session_id,
                    ToolSideEffects {
                        tool_id: format!("tool-{idx}-{tool_idx}"),
                        tool_name: "write".to_string(),
                        affected_paths: Some(vec![format!("file-{tool_idx}.txt")]),
                        checkpoint_id: None,
                    },
                )
                .expect("tool side effects");
        }

        store
            .append_run_ended(
                &continuity_id,
                &message_id,
                &session_id,
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");

        message_ids.push(message_id);
    }

    let cut_message_id = message_ids.get(10).cloned().expect("cut message id");
    store
        .compaction_checkpoint_cumulative_v1(
            &continuity_id,
            CompactionCheckpointCumulativeV1Request {
                summary_markdown: Some("summary".to_string()),
                summary_artifact_id: None,
                to_message_id: Some(cut_message_id.clone()),
                to_seq: None,
                stride_messages: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("compaction checkpoint");

    let anchor_message_id = message_ids.get(20).cloned().expect("anchor message id");

    let full_events = store.replay_events(&continuity_id).expect("replay full");
    let (full_from_seq, full_from_message_id) =
        resolve_context_compile_cutpoint_full(&full_events, &anchor_message_id).expect("cutpoint");

    let mut best: Option<(u64, u64, String)> = None;
    for event in &full_events {
        let EventKind::ContinuityCompactionCheckpointCreated {
            summary_kind,
            summary_artifact_id,
            to_seq,
            ..
        } = &event.kind
        else {
            continue;
        };
        if summary_kind != crate::compaction_summary::COMPACTION_SUMMARY_KIND_CUMULATIVE_V1 {
            continue;
        }
        if *to_seq > full_from_seq {
            continue;
        }
        match best.as_ref() {
            Some((best_to_seq, best_event_seq, _))
                if (*to_seq < *best_to_seq)
                    || (*to_seq == *best_to_seq && event.seq <= *best_event_seq) => {}
            _ => {
                best = Some((*to_seq, event.seq, summary_artifact_id.clone()));
            }
        }
    }
    let (summary_to_seq, _event_seq, summary_artifact_id) =
        best.expect("expected compaction checkpoint in full replay");

    let full_bundle =
        compile_summaries_recent_messages_v1(CompileSummariesRecentMessagesV1Request {
            continuity_id: &continuity_id,
            continuity_events: &full_events,
            event_log: event_log.as_ref(),
            snapshot_dir: &snapshot_dir,
            from_seq: full_from_seq,
            from_message_id: full_from_message_id.clone(),
            run_session_id: "run-session",
            actor_id: "user",
            origin: "cli",
            summary_artifact_id: &summary_artifact_id,
            summary_to_seq,
        })
        .expect("compile full");

    let window_input = store
        .load_context_compile_input_recent_messages_v1(&continuity_id, &anchor_message_id)
        .expect("window input");
    assert_eq!(window_input.from_seq, full_from_seq);
    assert_eq!(window_input.from_message_id, full_from_message_id);
    assert!(
        window_input.continuity_events.iter().all(|event| matches!(
            event.kind,
            EventKind::ContinuityMessageAppended { .. } | EventKind::ContinuityRunEnded { .. }
        )),
        "expected message+run-ended-only window events"
    );

    let checkpoint = store
        .latest_compaction_checkpoint_for_compile_v1(&continuity_id, window_input.from_seq)
        .expect("checkpoint lookup")
        .expect("checkpoint");
    assert_eq!(checkpoint.summary_artifact_id, summary_artifact_id);
    assert_eq!(checkpoint.to_seq, summary_to_seq);

    let window_bundle =
        compile_summaries_recent_messages_v1(CompileSummariesRecentMessagesV1Request {
            continuity_id: &continuity_id,
            continuity_events: &window_input.continuity_events,
            event_log: event_log.as_ref(),
            snapshot_dir: &snapshot_dir,
            from_seq: window_input.from_seq,
            from_message_id: window_input.from_message_id.clone(),
            run_session_id: "run-session",
            actor_id: "user",
            origin: "cli",
            summary_artifact_id: &checkpoint.summary_artifact_id,
            summary_to_seq: checkpoint.to_seq,
        })
        .expect("compile window");

    let full_json = serde_json::to_value(&full_bundle).expect("full json");
    let window_json = serde_json::to_value(&window_bundle).expect("window json");
    assert_eq!(window_json, full_json);
}

#[test]
fn window_context_compile_input_works_when_global_log_is_corrupt() {
    use std::io::Write;

    let dir = tempdir().expect("tmp");
    let (event_log, store, data_dir) = store_for(&dir);
    let snapshot_dir = dir.path().join("snapshots");

    const MSG_LEN: usize = 60_000;
    const MSG_COUNT: usize = 200;

    let continuity_id = store.ensure_default().expect("ensure");
    let mut message_ids: Vec<String> = Vec::new();

    for idx in 0..MSG_COUNT {
        let message_id = store
            .append_message(
                &continuity_id,
                "user".to_string(),
                "cli".to_string(),
                format!("m{idx}:{}", "x".repeat(MSG_LEN)),
            )
            .expect("append message");
        let session_id = format!("session-{idx}");

        let session_events = vec![
            Event {
                id: format!("se{idx}-0"),
                session_id: session_id.clone(),
                timestamp_ms: 0,
                seq: 0,
                kind: EventKind::SessionStarted {
                    input: "hi".to_string(),
                },
            },
            Event {
                id: format!("se{idx}-1"),
                session_id: session_id.clone(),
                timestamp_ms: 1,
                seq: 1,
                kind: EventKind::OutputTextDelta {
                    delta: format!("a{idx}"),
                },
            },
            Event {
                id: format!("se{idx}-2"),
                session_id: session_id.clone(),
                timestamp_ms: 2,
                seq: 2,
                kind: EventKind::SessionEnded {
                    reason: "completed".to_string(),
                },
            },
        ];
        write_snapshot(&snapshot_dir, &session_id, &session_events).expect("snapshot");

        store
            .append_run_spawned(
                &continuity_id,
                &message_id,
                &session_id,
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run spawned");
        store
            .append_context_compiled(
                &continuity_id,
                ContextCompiledPayload {
                    run_session_id: session_id.clone(),
                    bundle_artifact_id: "artifact-1".to_string(),
                    compiler_id: "rip.context_compiler.v1".to_string(),
                    compiler_strategy: "recent_messages_v1".to_string(),
                    from_seq: 0,
                    from_message_id: Some(message_id.clone()),
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            )
            .expect("context compiled");
        store
            .append_run_ended(
                &continuity_id,
                &message_id,
                &session_id,
                "completed".to_string(),
                "user".to_string(),
                "cli".to_string(),
            )
            .expect("run ended");

        message_ids.push(message_id);
    }

    let anchor_message_id = message_ids.get(40).cloned().expect("anchor message id");

    let full_events = store.replay_events(&continuity_id).expect("replay full");
    let (full_from_seq, full_from_message_id) =
        resolve_context_compile_cutpoint_full(&full_events, &anchor_message_id).expect("cutpoint");
    let expected_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
        continuity_id: &continuity_id,
        continuity_events: &full_events,
        event_log: event_log.as_ref(),
        snapshot_dir: &snapshot_dir,
        from_seq: full_from_seq,
        from_message_id: full_from_message_id.clone(),
        run_session_id: "run-session",
        actor_id: "user",
        origin: "cli",
    })
    .expect("compile full");
    let expected_json = serde_json::to_value(&expected_bundle).expect("bundle json");

    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(data_dir.join("events.jsonl"))
        .expect("open global log");
    writeln!(file, "not json").expect("corrupt global log");

    let (event_log2, store2, _data_dir2) = store_for(&dir);
    let window_input = store2
        .load_context_compile_input_recent_messages_v1(&continuity_id, &anchor_message_id)
        .expect("window input after restart");
    assert_eq!(window_input.from_seq, full_from_seq);
    assert_eq!(window_input.from_message_id, full_from_message_id);
    assert!(
        window_input.continuity_events.len() <= 128,
        "expected bounded window, got {} events",
        window_input.continuity_events.len()
    );

    let window_bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
        continuity_id: &continuity_id,
        continuity_events: &window_input.continuity_events,
        event_log: event_log2.as_ref(),
        snapshot_dir: &snapshot_dir,
        from_seq: window_input.from_seq,
        from_message_id: window_input.from_message_id.clone(),
        run_session_id: "run-session",
        actor_id: "user",
        origin: "cli",
    })
    .expect("compile window");
    let window_json = serde_json::to_value(&window_bundle).expect("bundle json");
    assert_eq!(window_json, expected_json);
}

#[test]
fn append_tool_side_effects_advances_seq() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    let message_id = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "hello".to_string(),
        )
        .expect("append");
    store
        .append_run_spawned(
            &continuity_id,
            &message_id,
            "session-1",
            "user".to_string(),
            "cli".to_string(),
        )
        .expect("run spawned");
    store
        .append_tool_side_effects(
            &ContinuityRunLink {
                continuity_id: continuity_id.clone(),
                message_id: message_id.clone(),
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
            "session-1",
            ToolSideEffects {
                tool_id: "tool-1".to_string(),
                tool_name: "write".to_string(),
                affected_paths: Some(vec!["a.txt".to_string()]),
                checkpoint_id: Some("checkpoint-1".to_string()),
            },
        )
        .expect("tool side effects");

    let events = event_log
        .replay_stream(StreamKind::Continuity, &continuity_id)
        .expect("replay");
    assert_eq!(events.len(), 4);
    assert_eq!(events[3].seq, 3);
    match &events[3].kind {
        EventKind::ContinuityToolSideEffects {
            run_session_id,
            tool_id,
            tool_name,
            affected_paths,
            checkpoint_id,
            actor_id,
            origin,
        } => {
            assert_eq!(run_session_id, "session-1");
            assert_eq!(tool_id, "tool-1");
            assert_eq!(tool_name, "write");
            assert_eq!(affected_paths.as_deref(), Some(&["a.txt".to_string()][..]));
            assert_eq!(checkpoint_id.as_deref(), Some("checkpoint-1"));
            assert_eq!(actor_id, "user");
            assert_eq!(origin, "cli");
        }
        other => panic!("expected tool side effects, got {other:?}"),
    }
}

#[test]
fn branch_creates_child_with_cutpoint_and_provenance() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);

    let parent_thread_id = store.ensure_default().expect("ensure");
    let m1 = store
        .append_message(
            &parent_thread_id,
            "user".to_string(),
            "cli".to_string(),
            "turn1".to_string(),
        )
        .expect("append");
    store
        .append_run_spawned(
            &parent_thread_id,
            &m1,
            "session-1",
            "user".to_string(),
            "cli".to_string(),
        )
        .expect("run spawned");
    store
        .append_run_ended(
            &parent_thread_id,
            &m1,
            "session-1",
            "completed".to_string(),
            "user".to_string(),
            "cli".to_string(),
        )
        .expect("run ended");
    let _m2 = store
        .append_message(
            &parent_thread_id,
            "user".to_string(),
            "cli".to_string(),
            "turn2".to_string(),
        )
        .expect("append");

    let (child_thread_id, parent_seq, parent_message_id) = store
        .branch(
            &parent_thread_id,
            Some("child".to_string()),
            Some(m1.clone()),
            None,
            "alice".to_string(),
            "team".to_string(),
        )
        .expect("branch");

    assert_eq!(parent_seq, 3, "expected cut to include run_ended");
    assert_eq!(parent_message_id.as_deref(), Some(m1.as_str()));

    let child_events = event_log
        .replay_stream(StreamKind::Continuity, &child_thread_id)
        .expect("replay child");
    assert_eq!(child_events.len(), 2);
    assert_eq!(child_events[0].seq, 0);
    assert_eq!(child_events[1].seq, 1);
    match &child_events[1].kind {
        EventKind::ContinuityBranched {
            parent_thread_id: parent_id,
            parent_seq: cut_seq,
            parent_message_id: cut_message_id,
            actor_id,
            origin,
        } => {
            assert_eq!(parent_id, &parent_thread_id);
            assert_eq!(*cut_seq, 3);
            assert_eq!(cut_message_id.as_deref(), Some(m1.as_str()));
            assert_eq!(actor_id, "alice");
            assert_eq!(origin, "team");
        }
        other => panic!("expected continuity_branched, got {other:?}"),
    }

    store
        .append_message(
            &child_thread_id,
            "user".to_string(),
            "cli".to_string(),
            "child turn".to_string(),
        )
        .expect("append child");
    let child_events = event_log
        .replay_stream(StreamKind::Continuity, &child_thread_id)
        .expect("replay child");
    assert_eq!(child_events.len(), 3);
    assert_eq!(
        child_events[2].seq, 2,
        "expected seq to continue after branch"
    );
}

#[test]
fn branch_rejects_conflicting_cut_selectors() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, _data_dir) = store_for(&dir);

    let parent_thread_id = store.ensure_default().expect("ensure");
    let err = store
        .branch(
            &parent_thread_id,
            None,
            Some("m1".to_string()),
            Some(1),
            "user".to_string(),
            "cli".to_string(),
        )
        .expect_err("expected error");
    assert!(err.contains("from_message_id") && err.contains("from_seq"));
}

#[test]
fn branch_and_handoff_support_from_seq_and_head_defaults() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);

    let parent_thread_id = store.ensure_default().expect("ensure");
    let m1 = store
        .append_message(
            &parent_thread_id,
            "user".to_string(),
            "cli".to_string(),
            "turn1".to_string(),
        )
        .expect("append");
    store
        .append_run_spawned(
            &parent_thread_id,
            &m1,
            "session-1",
            "user".to_string(),
            "cli".to_string(),
        )
        .expect("run spawned");
    store
        .append_run_ended(
            &parent_thread_id,
            &m1,
            "session-1",
            "completed".to_string(),
            "user".to_string(),
            "cli".to_string(),
        )
        .expect("run ended");
    let m2 = store
        .append_message(
            &parent_thread_id,
            "user".to_string(),
            "cli".to_string(),
            "turn2".to_string(),
        )
        .expect("append");

    let (branch_from_seq_id, branch_from_seq, branch_message_id) = store
        .branch(
            &parent_thread_id,
            None,
            None,
            Some(2),
            "alice".to_string(),
            "team".to_string(),
        )
        .expect("branch from_seq");
    assert_eq!(branch_from_seq, 2);
    assert_eq!(branch_message_id.as_deref(), Some(m1.as_str()));

    let branch_events = event_log
        .replay_stream(StreamKind::Continuity, &branch_from_seq_id)
        .expect("replay child");
    match &branch_events[1].kind {
        EventKind::ContinuityBranched {
            parent_seq,
            parent_message_id,
            ..
        } => {
            assert_eq!(*parent_seq, 2);
            assert_eq!(parent_message_id.as_deref(), Some(m1.as_str()));
        }
        other => panic!("expected continuity_branched, got {other:?}"),
    }

    let (_branch_head_id, branch_head_seq, branch_head_message_id) = store
        .branch(
            &parent_thread_id,
            None,
            None,
            None,
            "alice".to_string(),
            "team".to_string(),
        )
        .expect("branch head");
    assert_eq!(branch_head_seq, 4);
    assert_eq!(branch_head_message_id.as_deref(), Some(m2.as_str()));

    let (handoff_from_seq_id, handoff_from_seq, handoff_message_id) = store
        .handoff(
            &parent_thread_id,
            None,
            (Some("summary".to_string()), None),
            None,
            Some(2),
            ("alice".to_string(), "team".to_string()),
        )
        .expect("handoff from_seq");
    assert_eq!(handoff_from_seq, 2);
    assert_eq!(handoff_message_id.as_deref(), Some(m1.as_str()));

    let handoff_events = event_log
        .replay_stream(StreamKind::Continuity, &handoff_from_seq_id)
        .expect("replay handoff");
    match &handoff_events[1].kind {
        EventKind::ContinuityHandoffCreated {
            from_seq,
            from_message_id,
            ..
        } => {
            assert_eq!(*from_seq, 2);
            assert_eq!(from_message_id.as_deref(), Some(m1.as_str()));
        }
        other => panic!("expected continuity_handoff_created, got {other:?}"),
    }

    let (_handoff_head_id, handoff_head_seq, handoff_head_message_id) = store
        .handoff(
            &parent_thread_id,
            None,
            (Some("summary".to_string()), None),
            None,
            None,
            ("alice".to_string(), "team".to_string()),
        )
        .expect("handoff head");
    assert_eq!(handoff_head_seq, 4);
    assert_eq!(handoff_head_message_id.as_deref(), Some(m2.as_str()));
}

#[test]
fn handoff_creates_child_with_cutpoint_provenance_and_summary() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);

    let from_thread_id = store.ensure_default().expect("ensure");
    let m1 = store
        .append_message(
            &from_thread_id,
            "user".to_string(),
            "cli".to_string(),
            "turn1".to_string(),
        )
        .expect("append");
    store
        .append_run_spawned(
            &from_thread_id,
            &m1,
            "session-1",
            "user".to_string(),
            "cli".to_string(),
        )
        .expect("run spawned");
    store
        .append_run_ended(
            &from_thread_id,
            &m1,
            "session-1",
            "completed".to_string(),
            "user".to_string(),
            "cli".to_string(),
        )
        .expect("run ended");
    let _m2 = store
        .append_message(
            &from_thread_id,
            "user".to_string(),
            "cli".to_string(),
            "turn2".to_string(),
        )
        .expect("append");

    let (child_thread_id, from_seq, from_message_id) = store
        .handoff(
            &from_thread_id,
            Some("handoff".to_string()),
            (Some("summary".to_string()), None),
            Some(m1.clone()),
            None,
            ("alice".to_string(), "team".to_string()),
        )
        .expect("handoff");

    assert_eq!(from_seq, 3, "expected cut to include run_ended");
    assert_eq!(from_message_id.as_deref(), Some(m1.as_str()));

    let child_events = event_log
        .replay_stream(StreamKind::Continuity, &child_thread_id)
        .expect("replay child");
    assert_eq!(child_events.len(), 2);
    assert_eq!(child_events[0].seq, 0);
    assert_eq!(child_events[1].seq, 1);
    let artifact_id = match &child_events[1].kind {
        EventKind::ContinuityHandoffCreated {
            from_thread_id: event_from_id,
            from_seq: cut_seq,
            from_message_id: cut_message_id,
            summary_artifact_id,
            summary_markdown,
            actor_id,
            origin,
        } => {
            assert_eq!(event_from_id, &from_thread_id);
            assert_eq!(*cut_seq, 3);
            assert_eq!(cut_message_id.as_deref(), Some(m1.as_str()));
            let artifact_id = summary_artifact_id.as_deref().expect("summary_artifact_id");
            assert_eq!(artifact_id.len(), 64);
            assert_eq!(summary_markdown.as_deref(), Some("summary"));
            assert_eq!(actor_id, "alice");
            assert_eq!(origin, "team");
            artifact_id.to_string()
        }
        other => panic!("expected continuity_handoff_created, got {other:?}"),
    };

    let blob_path = dir
        .path()
        .join("workspace")
        .join(".rip")
        .join("artifacts")
        .join("blobs")
        .join(&artifact_id);
    let bytes = fs::read(&blob_path).expect("read bundle artifact");
    let json: serde_json::Value = serde_json::from_slice(&bytes).expect("bundle json");
    assert_eq!(
        json.get("schema").and_then(|v| v.as_str()),
        Some("rip.handoff_context_bundle.v1")
    );
    assert_eq!(
        json.get("summary_markdown").and_then(|v| v.as_str()),
        Some("summary")
    );
    let thread_refs = json
        .get("refs")
        .and_then(|v| v.get("threads"))
        .and_then(|v| v.as_array())
        .expect("thread refs");
    assert_eq!(thread_refs.len(), 1);
    assert_eq!(
        thread_refs[0].get("thread_id").and_then(|v| v.as_str()),
        Some(from_thread_id.as_str())
    );
    assert_eq!(thread_refs[0].get("seq").and_then(|v| v.as_u64()), Some(3));
    assert_eq!(
        thread_refs[0].get("message_id").and_then(|v| v.as_str()),
        Some(m1.as_str())
    );

    store
        .append_message(
            &child_thread_id,
            "user".to_string(),
            "cli".to_string(),
            "child turn".to_string(),
        )
        .expect("append child");
    let child_events = event_log
        .replay_stream(StreamKind::Continuity, &child_thread_id)
        .expect("replay child");
    assert_eq!(child_events.len(), 3);
    assert_eq!(
        child_events[2].seq, 2,
        "expected seq to continue after handoff"
    );
}

#[test]
fn handoff_rejects_missing_summary() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, _data_dir) = store_for(&dir);

    let from_thread_id = store.ensure_default().expect("ensure");
    let err = store
        .handoff(
            &from_thread_id,
            None,
            (None, None),
            None,
            None,
            ("user".to_string(), "cli".to_string()),
        )
        .expect_err("expected error");
    assert!(err.contains("summary"), "expected summary validation");
}

#[test]
fn handoff_rejects_conflicting_cut_selectors() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, _data_dir) = store_for(&dir);

    let from_thread_id = store.ensure_default().expect("ensure");
    let err = store
        .handoff(
            &from_thread_id,
            None,
            (Some("summary".to_string()), None),
            Some("m1".to_string()),
            Some(1),
            ("user".to_string(), "cli".to_string()),
        )
        .expect_err("expected error");
    assert!(err.contains("from_message_id") && err.contains("from_seq"));
}

#[test]
fn list_and_get_reflect_created_thread() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, _data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");

    let all = store.list();
    assert!(all.iter().any(|meta| meta.continuity_id == continuity_id));

    let meta = store.get(&continuity_id).expect("meta");
    assert_eq!(meta.continuity_id, continuity_id);
    assert!(!meta.archived);
}

#[test]
fn append_message_unknown_continuity_is_error() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, _data_dir) = store_for(&dir);

    let err = store
        .append_message(
            "missing-thread-id",
            "user".to_string(),
            "cli".to_string(),
            "hello".to_string(),
        )
        .expect_err("expected error");
    assert!(err.contains("continuity stream does not exist"));
}

#[test]
fn append_run_spawned_unknown_continuity_is_error() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, _data_dir) = store_for(&dir);

    let err = store
        .append_run_spawned(
            "missing-thread-id",
            "message-1",
            "session-1",
            "user".to_string(),
            "cli".to_string(),
        )
        .expect_err("expected error");
    assert!(err.contains("continuity stream does not exist"));
}

#[test]
fn new_ignores_invalid_index_json() {
    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_root = dir.path().join("workspace");
    fs::create_dir_all(&workspace_root).expect("workspace");

    let path = index_path(&data_dir);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(&path, b"not json").expect("write");

    let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
    let store = ContinuityStore::new(data_dir.clone(), workspace_root, event_log).expect("store");

    let continuity_id = store.ensure_default().expect("ensure");
    assert!(!continuity_id.is_empty());
}

#[test]
fn new_resets_index_on_version_mismatch() {
    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_root = dir.path().join("workspace");
    fs::create_dir_all(&workspace_root).expect("workspace");

    let legacy_id = "legacy-thread-id";
    let legacy = serde_json::json!({
        "version": 0,
        "workspaces": {
            workspace_key(&workspace_root): legacy_id,
        },
        "continuities": {
            legacy_id: {
                "created_at_ms": 0,
                "title": null,
                "archived": false,
            }
        }
    });
    let path = index_path(&data_dir);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(&path, legacy.to_string()).expect("write");

    let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
    let store = ContinuityStore::new(data_dir.clone(), workspace_root, event_log).expect("store");

    let continuity_id = store.ensure_default().expect("ensure");
    assert_ne!(continuity_id, legacy_id);
}

#[test]
fn context_compile_and_checkpoint_lookups_fall_back_without_sidecars() {
    let dir = tempdir().expect("tmp");
    let (_event_log, store, data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    let m1 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m1".to_string(),
        )
        .expect("append");
    let m2 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m2".to_string(),
        )
        .expect("append");
    let _m3 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m3".to_string(),
        )
        .expect("append");
    let _m4 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m4".to_string(),
        )
        .expect("append");

    let (_ckpt1, _summary1, to_seq1, _to_mid1, _cut_rule1) = store
        .compaction_checkpoint_cumulative_v1(
            &continuity_id,
            CompactionCheckpointCumulativeV1Request {
                summary_markdown: Some("summary-1".to_string()),
                summary_artifact_id: None,
                to_message_id: Some(m1.clone()),
                to_seq: None,
                stride_messages: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("checkpoint1");
    let (_ckpt2, _summary2, _to_seq2, _to_mid2, _cut_rule2) = store
        .compaction_checkpoint_cumulative_v1(
            &continuity_id,
            CompactionCheckpointCumulativeV1Request {
                summary_markdown: Some("summary-2".to_string()),
                summary_artifact_id: None,
                to_message_id: Some(m2.clone()),
                to_seq: None,
                stride_messages: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("checkpoint2");
    let (_ckpt3, summary3, to_seq3, _to_mid3, _cut_rule3) = store
        .compaction_checkpoint_cumulative_v1(
            &continuity_id,
            CompactionCheckpointCumulativeV1Request {
                summary_markdown: Some("summary-3".to_string()),
                summary_artifact_id: None,
                to_message_id: Some(m2.clone()),
                to_seq: None,
                stride_messages: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("checkpoint3");

    let full_events = store.replay_events(&continuity_id).expect("replay full");
    let (full_from_seq, full_from_message_id) =
        resolve_context_compile_cutpoint_full(&full_events, &m2).expect("cutpoint");

    fs::remove_dir_all(data_dir.join("continuity_streams")).expect("remove sidecars");
    let input = store
        .load_context_compile_input_recent_messages_v1(&continuity_id, &m2)
        .expect("input");
    assert_eq!(input.from_seq, full_from_seq);
    assert_eq!(input.from_message_id, full_from_message_id);
    assert_eq!(
        serde_json::to_value(&input.continuity_events).expect("input json"),
        serde_json::to_value(&full_events).expect("full json")
    );

    let head_seq = full_events
        .last()
        .map(|event| event.seq)
        .unwrap_or_default();

    fs::remove_dir_all(data_dir.join("continuity_streams")).expect("remove sidecars again");
    let latest = store
        .latest_compaction_checkpoint_for_compile_v1(&continuity_id, head_seq)
        .expect("latest lookup")
        .expect("latest");
    assert_eq!(latest.to_seq, to_seq3);
    assert_eq!(latest.summary_artifact_id, summary3);

    fs::remove_dir_all(data_dir.join("continuity_streams")).expect("remove sidecars third");
    let hierarchy = store
        .hierarchical_compaction_checkpoints_for_compile_v1(&continuity_id, head_seq, 2)
        .expect("hierarchy");
    assert_eq!(
        hierarchy
            .iter()
            .map(|entry| entry.to_seq)
            .collect::<Vec<_>>(),
        vec![to_seq1, to_seq3]
    );
    assert_eq!(
        hierarchy
            .last()
            .map(|entry| entry.summary_artifact_id.as_str()),
        Some(summary3.as_str())
    );
}

#[test]
fn compaction_auto_helpers_cover_noop_and_failed_job_paths() {
    let dir = tempdir().expect("tmp");
    let (event_log, store, _data_dir) = store_for(&dir);

    let continuity_id = store.ensure_default().expect("ensure");
    let m1 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m1".to_string(),
        )
        .expect("append");
    let _m2 = store
        .append_message(
            &continuity_id,
            "user".to_string(),
            "cli".to_string(),
            "m2".to_string(),
        )
        .expect("append");

    let (_checkpoint_id, artifact_id, _to_seq, _to_mid, _cut_rule) = store
        .compaction_checkpoint_cumulative_v1(
            &continuity_id,
            CompactionCheckpointCumulativeV1Request {
                summary_markdown: Some("summary".to_string()),
                summary_artifact_id: None,
                to_message_id: Some(m1.clone()),
                to_seq: None,
                stride_messages: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("checkpoint");

    let noop = store
        .compaction_auto_spawn_job_v1(
            &continuity_id,
            CompactionAutoV1Request {
                stride_messages: Some(1),
                max_new_checkpoints: Some(1),
                dry_run: Some(true),
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("dry run");
    assert_eq!(noop.status, "noop");

    let scheduled = store
        .compaction_auto_schedule_v1(
            &continuity_id,
            CompactionAutoScheduleV1Request {
                stride_messages: Some(1),
                max_new_checkpoints: Some(1),
                block_on_inflight: Some(false),
                execute: Some(false),
                dry_run: Some(false),
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect("scheduled");
    assert_eq!(scheduled.decision, "scheduled");
    assert!(scheduled.job_id.is_some());

    let invalid = store
        .compaction_auto_schedule_spawn_job_v1(
            &continuity_id,
            CompactionAutoScheduleV1Request {
                stride_messages: Some(0),
                max_new_checkpoints: Some(1),
                block_on_inflight: Some(true),
                execute: Some(true),
                dry_run: Some(false),
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        )
        .expect_err("invalid stride");
    assert!(invalid.contains("invalid_stride"));

    let blob_path = dir
        .path()
        .join("workspace")
        .join(".rip")
        .join("artifacts")
        .join("blobs")
        .join(&artifact_id);
    fs::remove_file(&blob_path).expect("remove summary artifact");

    let err = store
        .compaction_auto_run_spawned_job_v1(
            &continuity_id,
            "job-failed",
            1,
            "stride_messages_v1/1",
            &[CompactionPlannedCutPointV1 {
                target_message_ordinal: 2,
                to_seq: 2,
                to_message_id: "00000000-0000-0000-0000-000000000099".to_string(),
            }],
            ("user", "cli"),
        )
        .expect_err("failed job");
    assert!(err.contains("compaction cut point message mismatch"));

    let ended = event_log
        .replay_stream(StreamKind::Continuity, &continuity_id)
        .expect("replay")
        .into_iter()
        .rev()
        .find_map(|event| match event.kind {
            EventKind::ContinuityJobEnded {
                job_id,
                status,
                error,
                ..
            } => Some((job_id, status, error)),
            _ => None,
        })
        .expect("job ended");
    assert_eq!(ended.0, "job-failed");
    assert_eq!(ended.1, "failed");
    assert!(ended
        .2
        .as_deref()
        .unwrap_or_default()
        .contains("compaction cut point message mismatch"));
}

#[test]
fn ensure_default_errors_when_index_parent_is_file() {
    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_root = dir.path().join("workspace");
    fs::create_dir_all(&workspace_root).expect("workspace");
    fs::create_dir_all(&data_dir).expect("data");
    fs::write(data_dir.join("continuities"), "file").expect("continuities file");

    let event_log = Arc::new(EventLog::new(data_dir.join("events.jsonl")).expect("log"));
    let store = ContinuityStore::new(data_dir.clone(), workspace_root, event_log).expect("store");

    let err = store.ensure_default().expect_err("expected error");
    assert!(err.contains("save continuity index"));
}
