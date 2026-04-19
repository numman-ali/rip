use super::append::rebuild_messages_runs_seek_index_best_effort_v1;
use super::scan::{drain_sidecar_lines, scan_sidecar_backwards, ParseMode};
use super::*;
use rip_kernel::EventKind;
use tempfile::tempdir;

fn continuity_event(continuity_id: &str, seq: u64, kind: EventKind) -> Event {
    Event {
        id: format!("e{seq}"),
        session_id: continuity_id.to_string(),
        timestamp_ms: 0,
        seq,
        kind,
    }
}

#[test]
fn try_read_last_seq_reads_last_sidecar_line() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c1";

    cache.append_best_effort(&continuity_event(
        cid,
        0,
        EventKind::ContinuityCreated {
            workspace: "w".to_string(),
            title: None,
        },
    ));
    cache.append_best_effort(&continuity_event(
        cid,
        1,
        EventKind::ContinuityMessageAppended {
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
            content: "hello".to_string(),
        },
    ));
    cache.append_best_effort(&continuity_event(
        cid,
        2,
        EventKind::ContinuityMessageAppended {
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
            content: "world".to_string(),
        },
    ));

    let last = cache.try_read_last_seq(cid).expect("last seq");
    assert_eq!(last, Some(2));
}

#[test]
fn scan_tail_reports_completeness_and_respects_max_events() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c2";

    for seq in 0..6 {
        cache.append_best_effort(&continuity_event(
            cid,
            seq,
            EventKind::ContinuityMessageAppended {
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
                content: format!("m{seq}"),
            },
        ));
    }

    let tail = cache
        .scan_tail(cid, 2, 64 * 1024)
        .expect("tail")
        .expect("present");
    assert_eq!(tail.events.len(), 2);
    assert_eq!(tail.events[0].seq, 4);
    assert_eq!(tail.events[1].seq, 5);
    assert!(!tail.complete, "expected truncated tail");

    let all = cache
        .scan_tail(cid, 64, 64 * 1024)
        .expect("tail")
        .expect("present");
    assert_eq!(all.events.len(), 6);
    assert_eq!(all.events[0].seq, 0);
    assert_eq!(all.events[5].seq, 5);
    assert!(all.complete, "expected full read");
}

fn continuity_event_with_id(continuity_id: &str, seq: u64, id: &str, kind: EventKind) -> Event {
    Event {
        id: id.to_string(),
        session_id: continuity_id.to_string(),
        timestamp_ms: 0,
        seq,
        kind,
    }
}

fn message_event(continuity_id: &str, seq: u64, id: &str) -> Event {
    continuity_event_with_id(
        continuity_id,
        seq,
        id,
        EventKind::ContinuityMessageAppended {
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
            content: format!("m{seq}"),
        },
    )
}

fn run_ended_event(continuity_id: &str, seq: u64) -> Event {
    continuity_event_with_id(
        continuity_id,
        seq,
        &format!("run-{seq}"),
        EventKind::ContinuityRunEnded {
            run_session_id: format!("run-{seq}"),
            message_id: format!("m{seq}"),
            reason: "done".to_string(),
            actor_id: None,
            origin: None,
        },
    )
}

fn checkpoint_event(
    continuity_id: &str,
    seq: u64,
    checkpoint_id: &str,
    to_seq: u64,
    summary_kind: &str,
) -> Event {
    continuity_event_with_id(
        continuity_id,
        seq,
        checkpoint_id,
        EventKind::ContinuityCompactionCheckpointCreated {
            checkpoint_id: checkpoint_id.to_string(),
            cut_rule_id: "stride_messages_v1".to_string(),
            summary_kind: summary_kind.to_string(),
            summary_artifact_id: format!("artifact-{seq}"),
            from_seq: seq.saturating_sub(1),
            from_message_id: Some(format!("m{}", seq.saturating_sub(1))),
            to_seq,
            to_message_id: Some(format!("m{to_seq}")),
            actor_id: "user".to_string(),
            origin: "cli".to_string(),
        },
    )
}

#[test]
fn rebuild_best_effort_supports_replay_message_counts_and_windows() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-window";
    let id1 = "00000000-0000-0000-0000-000000000001";
    let id2 = "00000000-0000-0000-0000-000000000002";
    let id3 = "00000000-0000-0000-0000-000000000003";

    let events = vec![
        continuity_event(
            cid,
            0,
            EventKind::ContinuityCreated {
                workspace: "w".to_string(),
                title: Some("thread".to_string()),
            },
        ),
        message_event(cid, 1, id1),
        continuity_event_with_id(
            cid,
            2,
            "tool-2",
            EventKind::ContinuityToolSideEffects {
                run_session_id: "run-1".to_string(),
                tool_id: "tool-2".to_string(),
                tool_name: "write".to_string(),
                affected_paths: Some(vec!["a.txt".to_string()]),
                checkpoint_id: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ),
        run_ended_event(cid, 3),
        message_event(cid, 4, id2),
        continuity_event_with_id(
            cid,
            5,
            "tool-5",
            EventKind::ContinuityToolSideEffects {
                run_session_id: "run-2".to_string(),
                tool_id: "tool-5".to_string(),
                tool_name: "edit".to_string(),
                affected_paths: None,
                checkpoint_id: None,
                actor_id: "user".to_string(),
                origin: "cli".to_string(),
            },
        ),
        message_event(cid, 6, id3),
    ];

    cache.rebuild_best_effort(cid, &events);

    let replay = cache.try_replay(cid).expect("replay").expect("sidecar");
    assert_eq!(replay.len(), 7);
    assert_eq!(replay[6].seq, 6);

    assert_eq!(
        cache.message_count_messages_runs_v1(cid).expect("count"),
        Some(3)
    );
    assert_eq!(
        cache
            .message_by_ordinal_messages_runs_v1(cid, 2)
            .expect("ordinal")
            .expect("message"),
        (4, id2.to_string())
    );
    assert!(cache
        .message_by_ordinal_messages_runs_v1(cid, 4)
        .expect("ordinal")
        .is_none());

    let tail = cache
        .scan_tail_messages_runs_v1(cid, 10, 64 * 1024)
        .expect("tail")
        .expect("present");
    assert_eq!(
        tail.events
            .iter()
            .map(|event| event.seq)
            .collect::<Vec<_>>(),
        vec![1, 3, 4, 6]
    );
    assert!(tail.complete);

    let from_seq = cache
        .window_recent_messages_v1_from_seq(cid, 6, 2)
        .expect("window")
        .expect("present");
    assert_eq!(from_seq.from_seq, 6);
    assert_eq!(
        from_seq
            .events
            .iter()
            .map(|event| event.seq)
            .collect::<Vec<_>>(),
        vec![4, 6]
    );

    let from_message = cache
        .window_recent_messages_v1_from_message_id(cid, id2, 2)
        .expect("window")
        .expect("present");
    assert_eq!(from_message.from_seq, 5);
    assert_eq!(from_message.from_message_id.as_deref(), Some(id2));
    assert_eq!(
        from_message
            .events
            .iter()
            .map(|event| event.seq)
            .collect::<Vec<_>>(),
        vec![1, 3, 4]
    );
}

#[test]
fn missing_messages_runs_sidecar_and_corrupt_indexes_rebuild_from_full_sidecar() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-mr";
    let id1 = "00000000-0000-0000-0000-000000000010";
    let id2 = "00000000-0000-0000-0000-000000000011";

    cache.rebuild_best_effort(
        cid,
        &[
            continuity_event(
                cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: None,
                },
            ),
            message_event(cid, 1, id1),
            run_ended_event(cid, 2),
            message_event(cid, 3, id2),
        ],
    );

    std::fs::remove_file(cache.messages_runs_path_for_v1(cid)).expect("remove mr");
    std::fs::remove_file(cache.messages_runs_seq_index_path_v1(cid)).expect("remove seek");
    std::fs::remove_file(cache.messages_runs_message_index_path_v1(cid)).expect("remove idx");

    assert_eq!(
        cache.message_count_messages_runs_v1(cid).expect("count"),
        Some(2)
    );
    assert!(cache.messages_runs_path_for_v1(cid).exists());
    assert!(cache.messages_runs_seq_index_path_v1(cid).exists());
    assert!(cache.messages_runs_message_index_path_v1(cid).exists());

    std::fs::write(cache.messages_runs_message_index_path_v1(cid), b"bad").expect("corrupt idx");
    assert_eq!(
        cache
            .message_by_ordinal_messages_runs_v1(cid, 2)
            .expect("ordinal")
            .expect("message"),
        (3, id2.to_string())
    );

    std::fs::write(cache.messages_runs_seq_index_path_v1(cid), b"bad").expect("corrupt seek");
    let window = cache
        .window_recent_messages_v1_from_seq(cid, 3, 2)
        .expect("window")
        .expect("present");
    assert_eq!(
        window
            .events
            .iter()
            .map(|event| event.seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn checkpoint_queries_rebuild_sidecars_and_choose_latest_entries() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-comp";
    cache.rebuild_best_effort(
        cid,
        &[
            continuity_event(
                cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: None,
                },
            ),
            checkpoint_event(cid, 1, "ckpt-1", 1, "cumulative_v1"),
            message_event(cid, 2, "00000000-0000-0000-0000-000000000020"),
            checkpoint_event(cid, 3, "ckpt-2", 3, "cumulative_v1"),
            checkpoint_event(cid, 4, "ckpt-3", 3, "cumulative_v1"),
            checkpoint_event(cid, 5, "ckpt-4", 5, "other_v1"),
            checkpoint_event(cid, 6, "ckpt-5", 6, "cumulative_v1"),
        ],
    );

    std::fs::remove_file(cache.compaction_checkpoints_path_for_v1(cid)).expect("remove sidecar");
    std::fs::remove_file(cache.compaction_checkpoints_index_path_for_v1(cid))
        .expect("remove index");

    let latest = cache
        .latest_compaction_checkpoint_before_or_at_seq_v1(cid, 3)
        .expect("latest")
        .expect("checkpoint");
    let EventKind::ContinuityCompactionCheckpointCreated {
        checkpoint_id,
        to_seq,
        ..
    } = latest.kind
    else {
        panic!("expected checkpoint");
    };
    assert_eq!(checkpoint_id, "ckpt-3");
    assert_eq!(to_seq, 3);

    let hierarchy = cache
        .hierarchical_compaction_checkpoints_before_or_at_seq_v1(cid, 6, 3, Some("cumulative_v1"))
        .expect("hierarchy")
        .expect("present");
    assert_eq!(
        hierarchy
            .iter()
            .map(|entry| entry.checkpoint_id.as_str())
            .collect::<Vec<_>>(),
        vec!["ckpt-1", "ckpt-3", "ckpt-5"]
    );
    assert!(cache
        .hierarchical_compaction_checkpoints_before_or_at_seq_v1(cid, 6, 0, None)
        .expect("hierarchy")
        .expect("present")
        .is_empty());
}

#[test]
fn message_validations_surface_missing_sidecars_and_drifted_ordinals() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-ordinal";
    let id1 = "00000000-0000-0000-0000-000000000040";
    let id2 = "00000000-0000-0000-0000-000000000041";
    let bogus_id = "00000000-0000-0000-0000-000000000042";

    cache.rebuild_best_effort(
        cid,
        &[
            continuity_event(
                cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: None,
                },
            ),
            message_event(cid, 1, id1),
            run_ended_event(cid, 2),
            message_event(cid, 3, id2),
        ],
    );

    let ord_path = cache.messages_runs_message_ordinal_index_path_v1(cid);
    append_message_record_best_effort_v1(&ord_path, 99, bogus_id);

    let err = cache
        .message_count_messages_runs_v1(cid)
        .expect_err("drifted ordinal index");
    assert!(err.to_string().contains("out of sync"));

    let err = cache
        .message_by_ordinal_messages_runs_v1(cid, 3)
        .expect_err("missing message");
    assert!(err.to_string().contains("references missing message"));

    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-missing-sidecar";
    cache.rebuild_best_effort(
        cid,
        &[
            continuity_event(
                cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: None,
                },
            ),
            message_event(cid, 1, "00000000-0000-0000-0000-000000000043"),
        ],
    );

    std::fs::remove_file(cache.messages_runs_path_for_v1(cid)).expect("remove mr");
    std::fs::remove_file(cache.path_for(cid)).expect("remove full");

    let err = cache
        .message_count_messages_runs_v1(cid)
        .expect_err("missing mr sidecar");
    assert!(err.to_string().contains("cannot be validated"));
}

#[test]
fn window_lookup_paths_rebuild_indexes_and_fall_back_to_available_sidecars() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-full-window";
    let id1 = "00000000-0000-0000-0000-000000000050";
    let id2 = "00000000-0000-0000-0000-000000000051";
    let id3 = "00000000-0000-0000-0000-000000000052";

    cache.rebuild_best_effort(
        cid,
        &[
            continuity_event(
                cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: Some("thread".to_string()),
                },
            ),
            message_event(cid, 1, id1),
            run_ended_event(cid, 2),
            message_event(cid, 3, id2),
            continuity_event_with_id(
                cid,
                4,
                "tool-4",
                EventKind::ContinuityToolSideEffects {
                    run_session_id: "run-2".to_string(),
                    tool_id: "tool-4".to_string(),
                    tool_name: "edit".to_string(),
                    affected_paths: None,
                    checkpoint_id: None,
                    actor_id: "user".to_string(),
                    origin: "cli".to_string(),
                },
            ),
            message_event(cid, 5, id3),
        ],
    );

    std::fs::remove_file(message_index_path(&cache.dir, cid)).expect("remove full message idx");
    let window = cache
        .window_recent_messages_v1_from_message_id_full_sidecar(cid, id2, 2)
        .expect("window")
        .expect("present");
    assert_eq!(window.from_seq, 4);
    assert_eq!(window.from_message_id.as_deref(), Some(id2));
    assert_eq!(
        window
            .events
            .iter()
            .map(|event| event.seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );

    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-mr-head";
    let id1 = "00000000-0000-0000-0000-000000000053";
    let id2 = "00000000-0000-0000-0000-000000000054";
    cache.rebuild_best_effort(
        cid,
        &[
            continuity_event(
                cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: None,
                },
            ),
            message_event(cid, 1, id1),
            run_ended_event(cid, 2),
            message_event(cid, 3, id2),
        ],
    );

    std::fs::remove_file(cache.path_for(cid)).expect("remove full sidecar");
    let window = cache
        .window_recent_messages_v1_from_message_id_messages_runs_v1(cid, id2, 2)
        .expect("window")
        .expect("present");
    assert_eq!(window.from_seq, 3);
    assert_eq!(window.from_message_id.as_deref(), Some(id2));
    assert_eq!(
        window
            .events
            .iter()
            .map(|event| event.seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
}

#[test]
fn sidecar_builders_and_tail_readers_surface_invalid_inputs() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-invalid-build";

    let session_event = Event {
        id: "session-0".to_string(),
        session_id: "run-1".to_string(),
        timestamp_ms: 0,
        seq: 0,
        kind: EventKind::SessionStarted {
            input: "hi".to_string(),
        },
    };
    std::fs::create_dir_all(&cache.dir).expect("dir");
    std::fs::write(
        cache.path_for(cid),
        format!("{}\n", serde_json::to_string(&session_event).expect("json")),
    )
    .expect("write");

    let err = cache
        .rebuild_messages_runs_from_full_sidecar_best_effort_v1(
            cid,
            &cache.path_for(cid),
            &cache.messages_runs_path_for_v1(cid),
        )
        .expect_err("invalid mr rebuild");
    assert!(err.to_string().contains("while building mr sidecar"));

    let err = cache
        .rebuild_compaction_checkpoints_from_full_sidecar_best_effort_v1(
            cid,
            &cache.path_for(cid),
            &cache.compaction_checkpoints_path_for_v1(cid),
        )
        .expect_err("invalid comp rebuild");
    assert!(err
        .to_string()
        .contains("while building compaction sidecar"));

    let empty_sidecar = cache.path_for("c-empty");
    std::fs::write(&empty_sidecar, "").expect("write empty");
    let err = cache
        .try_read_last_seq_for_sidecar_path("c-empty", &empty_sidecar)
        .expect_err("empty sidecar");
    assert!(err.to_string().contains("is empty"));

    let mr_sidecar = cache.messages_runs_path_for_v1("c-mr-index");
    std::fs::write(
        &mr_sidecar,
        format!("{}\n", serde_json::to_string(&session_event).expect("json")),
    )
    .expect("write mr");
    let err = rebuild_messages_runs_seek_index_best_effort_v1(
        &mr_sidecar,
        &cache.messages_runs_seq_index_path_v1("c-mr-index"),
    )
    .expect_err("invalid mr seek");
    assert!(err.to_string().contains("non-continuity event"));
}

#[test]
fn compaction_index_rebuilds_from_corruption_and_absence_is_explicit() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-comp-rebuild";
    cache.rebuild_best_effort(
        cid,
        &[
            continuity_event(
                cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: None,
                },
            ),
            checkpoint_event(cid, 1, "ckpt-1", 1, "cumulative_v1"),
            checkpoint_event(cid, 2, "ckpt-2", 2, "cumulative_v1"),
            checkpoint_event(cid, 3, "ckpt-3", 2, "cumulative_v1"),
        ],
    );

    std::fs::write(cache.compaction_checkpoints_index_path_for_v1(cid), b"bad")
        .expect("corrupt index");
    let entries = cache
        .hierarchical_compaction_checkpoints_before_or_at_seq_v1(cid, 2, 2, Some("cumulative_v1"))
        .expect("hierarchy")
        .expect("present");
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.checkpoint_id.as_str())
            .collect::<Vec<_>>(),
        vec!["ckpt-1", "ckpt-3"]
    );

    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-no-checkpoints";
    cache.rebuild_best_effort(
        cid,
        &[
            continuity_event(
                cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: None,
                },
            ),
            message_event(cid, 1, "00000000-0000-0000-0000-000000000060"),
        ],
    );

    assert!(cache
        .ensure_compaction_checkpoints_sidecar_best_effort_v1(cid)
        .expect("sidecar")
        .is_none());
    assert!(cache
        .ensure_compaction_checkpoints_index_best_effort_v1(cid)
        .expect("index")
        .is_none());
}

#[test]
fn invalid_sidecars_surface_replay_and_tail_errors() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-bad";

    std::fs::create_dir_all(cache.dir.clone()).expect("dir");
    std::fs::write(cache.path_for(cid), "\n\n").expect("write");
    let err = cache.try_replay(cid).expect_err("empty sidecar");
    assert!(err.to_string().contains("is empty"));

    let session_event = Event {
        id: "s0".to_string(),
        session_id: "run-1".to_string(),
        timestamp_ms: 0,
        seq: 0,
        kind: EventKind::SessionStarted {
            input: "hi".to_string(),
        },
    };
    std::fs::write(
        cache.path_for(cid),
        format!("{}\n", serde_json::to_string(&session_event).expect("json")),
    )
    .expect("write");
    let err = cache.try_replay(cid).expect_err("non continuity");
    assert!(err.to_string().contains("non-continuity"));

    std::fs::write(
        cache.path_for(cid),
        format!(
            "{}\n{}\n",
            serde_json::to_string(&message_event(
                cid,
                0,
                "00000000-0000-0000-0000-000000000030",
            ))
            .expect("json"),
            serde_json::to_string(&message_event(
                cid,
                2,
                "00000000-0000-0000-0000-000000000031",
            ))
            .expect("json"),
        ),
    )
    .expect("write");
    let err = cache.scan_tail(cid, 10, 64 * 1024).expect_err("gap");
    assert!(err.to_string().contains("non-contiguous"));
}

#[test]
fn rebuild_helpers_leave_no_derived_files_when_no_cacheable_events_exist() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-empty-derived";

    cache.rebuild_best_effort(
        cid,
        &[continuity_event(
            cid,
            0,
            EventKind::ContinuityCreated {
                workspace: "w".to_string(),
                title: None,
            },
        )],
    );

    assert!(
        cache.path_for(cid).exists(),
        "full sidecar should still exist"
    );
    assert!(!cache.messages_runs_path_for_v1(cid).exists());
    assert!(!cache.messages_runs_seq_index_path_v1(cid).exists());
    assert!(!cache.messages_runs_message_index_path_v1(cid).exists());
    assert!(!cache
        .messages_runs_message_ordinal_index_path_v1(cid)
        .exists());
    assert!(!cache.compaction_checkpoints_path_for_v1(cid).exists());
    assert!(!cache.compaction_checkpoints_index_path_for_v1(cid).exists());
    assert!(cache
        .ensure_messages_runs_sidecar_best_effort_v1(cid)
        .expect("ensure mr")
        .is_none());
    assert!(cache
        .ensure_compaction_checkpoints_sidecar_best_effort_v1(cid)
        .expect("ensure comp")
        .is_none());
    assert!(cache
        .ensure_compaction_checkpoints_index_best_effort_v1(cid)
        .expect("ensure comp idx")
        .is_none());
    assert!(cache
        .scan_tail_messages_runs_v1(cid, 8, 64 * 1024)
        .expect("scan mr tail")
        .is_none());

    fs::create_dir_all(&cache.dir).expect("cache dir");
    let runs_only_cid = "c-runs-only";
    let runs_only_sidecar = cache.messages_runs_path_for_v1(runs_only_cid);
    fs::write(
        &runs_only_sidecar,
        format!(
            "{}\n",
            serde_json::to_string(&run_ended_event(runs_only_cid, 1)).expect("json")
        ),
    )
    .expect("write mr sidecar");
    let runs_only_index = cache.messages_runs_seq_index_path_v1(runs_only_cid);
    rebuild_messages_runs_seek_index_best_effort_v1(&runs_only_sidecar, &runs_only_index)
        .expect("rebuild mr seek");
    assert!(
        !runs_only_index.exists(),
        "run-only sidecars should not emit message seeks"
    );
}

#[test]
fn backward_scan_helpers_cover_partial_lines_and_non_continuity_inputs() {
    let dir = tempdir().expect("tmp");
    let path = dir.path().join("scan.jsonl");
    let continuity_id = "c-scan";

    fs::write(&path, "").expect("write empty");
    let mut empty = File::open(&path).expect("open empty");
    let empty_scan = scan_sidecar_backwards(
        &mut empty,
        continuity_id,
        8,
        64 * 1024,
        ParseMode::Header,
        Some(0),
    )
    .expect("empty scan");
    assert!(empty_scan.complete);
    assert!(empty_scan.headers.is_empty());

    let message_json = serde_json::to_string(&message_event(
        continuity_id,
        0,
        "00000000-0000-0000-0000-000000000070",
    ))
    .expect("json");
    fs::write(&path, &message_json).expect("write partial line");
    let mut partial = File::open(&path).expect("open partial");
    let header_scan = scan_sidecar_backwards(
        &mut partial,
        continuity_id,
        8,
        64 * 1024,
        ParseMode::Header,
        None,
    )
    .expect("header scan");
    assert_eq!(header_scan.headers.len(), 1);
    assert!(header_scan.complete);

    let mut partial = File::open(&path).expect("open partial");
    let event_scan = scan_sidecar_backwards(
        &mut partial,
        continuity_id,
        8,
        64 * 1024,
        ParseMode::Event,
        None,
    )
    .expect("event scan");
    assert_eq!(event_scan.events.len(), 1);
    assert!(event_scan.complete);

    let session_event = Event {
        id: "session-0".to_string(),
        session_id: "run-1".to_string(),
        timestamp_ms: 0,
        seq: 1,
        kind: EventKind::SessionStarted {
            input: "hi".to_string(),
        },
    };
    let session_json = serde_json::to_string(&session_event).expect("json");
    fs::write(&path, &session_json).expect("write session");
    let mut invalid = File::open(&path).expect("open invalid");
    let err = scan_sidecar_backwards(
        &mut invalid,
        continuity_id,
        8,
        64 * 1024,
        ParseMode::Event,
        None,
    )
    .expect_err("event mismatch");
    assert!(err.to_string().contains("non-continuity"));

    let mut invalid = File::open(&path).expect("open invalid");
    let err = scan_sidecar_backwards(
        &mut invalid,
        continuity_id,
        8,
        64 * 1024,
        ParseMode::Header,
        None,
    )
    .expect_err("header mismatch");
    assert!(err.to_string().contains("non-continuity"));

    let mut pending = format!("{}\n{session_json}\n", message_json).into_bytes();
    let err = drain_sidecar_lines(
        &mut pending,
        continuity_id,
        8,
        ParseMode::Event,
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .expect_err("drain event");
    assert!(err.to_string().contains("non-continuity"));

    let mut pending = format!("{}\n{session_json}\n", message_json).into_bytes();
    let err = drain_sidecar_lines(
        &mut pending,
        continuity_id,
        8,
        ParseMode::Header,
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .expect_err("drain header");
    assert!(err.to_string().contains("non-continuity"));
}

#[test]
fn full_sidecar_helpers_rebuild_missing_indexes_and_surface_edge_cases() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-full-helpers";
    let id1 = "00000000-0000-0000-0000-000000000071";
    let id2 = "00000000-0000-0000-0000-000000000072";

    cache.rebuild_best_effort(
        cid,
        &[
            continuity_event(
                cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: None,
                },
            ),
            message_event(cid, 1, id1),
            run_ended_event(cid, 2),
            message_event(cid, 3, id2),
        ],
    );

    let full_seek = seq_index_path(&cache.dir, cid);
    fs::remove_file(&full_seek).expect("remove full seek");
    let rebuilt = cache
        .window_recent_messages_v1_from_seq(cid, 3, 2)
        .expect("window")
        .expect("present");
    assert_eq!(
        rebuilt
            .events
            .iter()
            .map(|event| event.seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );

    fs::write(&full_seek, b"bad").expect("corrupt full seek");
    let rebuilt = cache
        .window_recent_messages_v1_from_seq(cid, 3, 2)
        .expect("window")
        .expect("present");
    assert_eq!(
        rebuilt
            .events
            .iter()
            .map(|event| event.seq)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );

    let full_sidecar = cache.path_for(cid);
    let seq_entries = cache
        .ensure_seq_index_v1(cid, &full_sidecar)
        .expect("ensure seq");
    let boundary = cache
        .boundary_pos_for_seq_v1(cid, &full_sidecar, &seq_entries, 3)
        .expect("boundary");
    assert_eq!(boundary, fs::metadata(&full_sidecar).expect("meta").len());

    assert!(cache
        .window_recent_messages_v1_from_message_id_full_sidecar(
            cid,
            "00000000-0000-0000-0000-000000000073",
            2,
        )
        .expect("missing anchor")
        .is_none());

    let replay_gap_cid = "c-replay-gap";
    fs::write(
        cache.path_for(replay_gap_cid),
        format!(
            "{}\n{}\n",
            serde_json::to_string(&message_event(
                replay_gap_cid,
                0,
                "00000000-0000-0000-0000-000000000074",
            ))
            .expect("json"),
            serde_json::to_string(&message_event(
                replay_gap_cid,
                2,
                "00000000-0000-0000-0000-000000000075",
            ))
            .expect("json"),
        ),
    )
    .expect("write replay gap");
    let err = cache.try_replay(replay_gap_cid).expect_err("replay gap");
    assert!(err.to_string().contains("seq mismatch"));

    fs::remove_file(cache.path_for(cid)).expect("remove full sidecar");
    assert!(cache
        .window_recent_messages_v1_from_seq(cid, 3, 2)
        .expect("missing full sidecar")
        .is_none());
}

#[test]
fn anchor_window_helpers_reject_non_continuity_sidecars_in_full_and_mr_modes() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());

    let full_cid = "c-full-anchor-invalid";
    let full_id1 = "00000000-0000-0000-0000-000000000080";
    let full_id2 = "00000000-0000-0000-0000-000000000081";
    fs::create_dir_all(&cache.dir).expect("cache dir");
    fs::write(
        cache.path_for(full_cid),
        format!(
            "{}\n{}\n{}\n{}\n",
            serde_json::to_string(&continuity_event(
                full_cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: None,
                },
            ))
            .expect("json"),
            serde_json::to_string(&message_event(full_cid, 1, full_id1)).expect("json"),
            serde_json::to_string(&Event {
                id: "session-1".to_string(),
                session_id: "run-1".to_string(),
                timestamp_ms: 0,
                seq: 2,
                kind: EventKind::SessionStarted {
                    input: "hi".to_string(),
                },
            })
            .expect("json"),
            serde_json::to_string(&message_event(full_cid, 3, full_id2)).expect("json"),
        ),
    )
    .expect("write full sidecar");
    rebuild_message_index_from_sidecar_v1(
        &cache.path_for(full_cid),
        &message_index_path(&cache.dir, full_cid),
    )
    .expect("rebuild full index");
    let err = cache
        .window_recent_messages_v1_from_message_id_full_sidecar(full_cid, full_id1, 2)
        .expect_err("invalid full sidecar");
    assert!(err.to_string().contains("non-continuity"));

    let mr_cid = "c-mr-anchor-invalid";
    let mr_id1 = "00000000-0000-0000-0000-000000000082";
    let mr_id2 = "00000000-0000-0000-0000-000000000083";
    let mr_sidecar = cache.messages_runs_path_for_v1(mr_cid);
    fs::write(
        &mr_sidecar,
        format!(
            "{}\n{}\n{}\n",
            serde_json::to_string(&message_event(mr_cid, 1, mr_id1)).expect("json"),
            serde_json::to_string(&Event {
                id: "session-2".to_string(),
                session_id: "run-2".to_string(),
                timestamp_ms: 0,
                seq: 2,
                kind: EventKind::SessionStarted {
                    input: "hello".to_string(),
                },
            })
            .expect("json"),
            serde_json::to_string(&message_event(mr_cid, 3, mr_id2)).expect("json"),
        ),
    )
    .expect("write mr sidecar");
    rebuild_message_index_from_sidecar_v1(
        &mr_sidecar,
        &cache.messages_runs_message_index_path_v1(mr_cid),
    )
    .expect("rebuild mr index");
    let err = cache
        .window_recent_messages_v1_from_message_id_messages_runs_v1(mr_cid, mr_id1, 2)
        .expect_err("invalid mr sidecar");
    assert!(err.to_string().contains("non-continuity"));
}

#[test]
fn message_count_messages_runs_reports_header_only_indexes_with_messages() {
    let dir = tempdir().expect("tmp");
    let cache = ContinuityStreamCache::new(dir.path());
    let cid = "c-header-only";

    cache.rebuild_best_effort(
        cid,
        &[
            continuity_event(
                cid,
                0,
                EventKind::ContinuityCreated {
                    workspace: "w".to_string(),
                    title: None,
                },
            ),
            message_event(cid, 1, "00000000-0000-0000-0000-000000000090"),
        ],
    );

    let mut header = [0u8; 32];
    header[0..8].copy_from_slice(b"RIPMORD1");
    header[8..12].copy_from_slice(&1u32.to_le_bytes());
    header[12..16].copy_from_slice(&(24u32).to_le_bytes());
    fs::write(
        cache.messages_runs_message_ordinal_index_path_v1(cid),
        header,
    )
    .expect("write header only");

    let err = cache
        .message_count_messages_runs_v1(cid)
        .expect_err("header only ordinal index");
    assert!(err
        .to_string()
        .contains("empty but messages+runs sidecar contains messages"));
}
