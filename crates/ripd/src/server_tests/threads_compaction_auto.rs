use super::*;

#[tokio::test]
async fn thread_compaction_auto_dry_run_emits_no_job_frames() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let thread_id = ensure_thread_id(&app).await;

    let _m1 = post_thread_message(&app, &thread_id, "m1").await.message_id;
    let _m2 = post_thread_message(&app, &thread_id, "m2").await.message_id;

    let (status, value) = compaction_auto(
        &app,
        &thread_id,
        serde_json::json!({
            "stride_messages": 2,
            "max_new_checkpoints": 1,
            "dry_run": true,
            "actor_id": "alice",
            "origin": "cli",
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(
        value.get("status").and_then(|value| value.as_str()),
        Some("noop")
    );
    assert!(value.get("job_id").is_none() || value.get("job_id").unwrap().is_null());

    let planned = value
        .get("planned")
        .and_then(|value| value.as_array())
        .expect("planned");
    assert_eq!(planned.len(), 1);
    assert_eq!(
        planned[0]
            .get("target_message_ordinal")
            .and_then(|v| v.as_u64()),
        Some(2)
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/threads/{thread_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let mut saw_job_spawned = false;
    let mut saw_checkpoint = false;
    for _ in 0..256 {
        let message = match timeout(Duration::from_millis(50), reader.next_data_message()).await {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(_) => break,
        };
        if let Some(value) = extract_data_json(&message) {
            match value.get("type").and_then(|value| value.as_str()) {
                Some("continuity_job_spawned") => {
                    saw_job_spawned = true;
                    break;
                }
                Some("continuity_compaction_checkpoint_created") => {
                    saw_checkpoint = true;
                    break;
                }
                _ => {}
            }
        }
    }

    assert!(
        !saw_job_spawned,
        "dry_run must not emit continuity_job_spawned"
    );
    assert!(
        !saw_checkpoint,
        "dry_run must not emit continuity_compaction_checkpoint_created"
    );
}

#[tokio::test]
async fn thread_compaction_auto_spawns_job_and_emits_checkpoint_and_job_ended() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let thread_id = ensure_thread_id(&app).await;

    let _m1 = post_thread_message(&app, &thread_id, "m1").await.message_id;
    let m2 = post_thread_message(&app, &thread_id, "m2").await.message_id;

    let (status, value) = compaction_auto(
        &app,
        &thread_id,
        serde_json::json!({
            "stride_messages": 2,
            "max_new_checkpoints": 1,
            "dry_run": false,
            "actor_id": "alice",
            "origin": "cli",
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::ACCEPTED);
    let job_id = value
        .get("job_id")
        .and_then(|value| value.as_str())
        .expect("job_id");
    assert_eq!(
        value.get("job_kind").and_then(|value| value.as_str()),
        Some("compaction_summarizer_v1")
    );
    assert_eq!(
        value.get("status").and_then(|value| value.as_str()),
        Some("spawned")
    );
    let planned = value
        .get("planned")
        .and_then(|value| value.as_array())
        .expect("planned");
    assert_eq!(planned.len(), 1);
    assert_eq!(
        planned[0].get("to_message_id").and_then(|v| v.as_str()),
        Some(m2.as_str())
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/threads/{thread_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let mut saw_spawned = false;
    let mut saw_checkpoint = false;
    let mut saw_job_ended = false;

    timeout(Duration::from_secs(3), async {
        while let Some(message) = reader.next_data_message().await {
            let Some(value) = extract_data_json(&message) else {
                continue;
            };
            match value.get("type").and_then(|value| value.as_str()) {
                Some("continuity_job_spawned") => {
                    if value.get("job_id").and_then(|v| v.as_str()) == Some(job_id) {
                        assert_eq!(
                            value.get("actor_id").and_then(|v| v.as_str()),
                            Some("alice")
                        );
                        assert_eq!(value.get("origin").and_then(|v| v.as_str()), Some("cli"));
                        saw_spawned = true;
                    }
                }
                Some("continuity_compaction_checkpoint_created") => {
                    if value.get("to_message_id").and_then(|v| v.as_str()) == Some(m2.as_str()) {
                        saw_checkpoint = true;
                    }
                }
                Some("continuity_job_ended") => {
                    if value.get("job_id").and_then(|v| v.as_str()) == Some(job_id) {
                        assert_eq!(
                            value.get("status").and_then(|v| v.as_str()),
                            Some("completed")
                        );
                        saw_job_ended = true;
                        break;
                    }
                }
                _ => {}
            }

            if saw_spawned && saw_checkpoint && saw_job_ended {
                break;
            }
        }
    })
    .await
    .expect("timeout");

    assert!(saw_spawned, "expected continuity_job_spawned");
    assert!(
        saw_checkpoint,
        "expected continuity_compaction_checkpoint_created for to_message_id={m2}"
    );
    assert!(saw_job_ended, "expected continuity_job_ended");
}

#[tokio::test]
async fn thread_compaction_auto_schedule_dry_run_emits_no_frames() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let thread_id = ensure_thread_id(&app).await;

    let _m1 = post_thread_message(&app, &thread_id, "m1").await.message_id;
    let _m2 = post_thread_message(&app, &thread_id, "m2").await.message_id;

    let (status, value) = compaction_auto_schedule(
        &app,
        &thread_id,
        serde_json::json!({
            "stride_messages": 2,
            "max_new_checkpoints": 1,
            "block_on_inflight": true,
            "execute": true,
            "dry_run": true,
            "actor_id": "alice",
            "origin": "cli",
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(
        value.get("decision").and_then(|value| value.as_str()),
        Some("dry_run")
    );

    let planned = value
        .get("planned")
        .and_then(|value| value.as_array())
        .expect("planned");
    assert_eq!(planned.len(), 1);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/threads/{thread_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let mut saw_decision = false;
    let mut saw_job_spawned = false;
    let mut saw_checkpoint = false;

    for _ in 0..256 {
        let message = match timeout(Duration::from_millis(50), reader.next_data_message()).await {
            Ok(Some(message)) => message,
            Ok(None) => break,
            Err(_) => break,
        };
        if let Some(value) = extract_data_json(&message) {
            match value.get("type").and_then(|value| value.as_str()) {
                Some("continuity_compaction_auto_schedule_decided") => {
                    saw_decision = true;
                    break;
                }
                Some("continuity_job_spawned") => {
                    saw_job_spawned = true;
                    break;
                }
                Some("continuity_compaction_checkpoint_created") => {
                    saw_checkpoint = true;
                    break;
                }
                _ => {}
            }
        }
    }

    assert!(
        !saw_decision,
        "dry_run must not emit continuity_compaction_auto_schedule_decided"
    );
    assert!(
        !saw_job_spawned,
        "dry_run must not emit continuity_job_spawned"
    );
    assert!(
        !saw_checkpoint,
        "dry_run must not emit continuity_compaction_checkpoint_created"
    );
}

#[tokio::test]
async fn thread_compaction_auto_schedule_spawns_job_emits_decision_and_completes() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let thread_id = ensure_thread_id(&app).await;

    let _m1 = post_thread_message(&app, &thread_id, "m1").await.message_id;
    let m2 = post_thread_message(&app, &thread_id, "m2").await.message_id;

    let (status, value) = compaction_auto_schedule(
        &app,
        &thread_id,
        serde_json::json!({
            "stride_messages": 2,
            "max_new_checkpoints": 1,
            "block_on_inflight": true,
            "execute": true,
            "dry_run": false,
            "actor_id": "alice",
            "origin": "cli",
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::ACCEPTED);
    assert_eq!(
        value.get("decision").and_then(|value| value.as_str()),
        Some("scheduled")
    );
    let decision_id = value
        .get("decision_id")
        .and_then(|value| value.as_str())
        .expect("decision_id");
    let job_id = value
        .get("job_id")
        .and_then(|value| value.as_str())
        .expect("job_id");
    assert_eq!(
        value.get("job_kind").and_then(|value| value.as_str()),
        Some("compaction_summarizer_v1")
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/threads/{thread_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let mut saw_decision = false;
    let mut saw_spawned = false;
    let mut saw_checkpoint = false;
    let mut saw_job_ended = false;

    timeout(Duration::from_secs(3), async {
        while let Some(message) = reader.next_data_message().await {
            let Some(value) = extract_data_json(&message) else {
                continue;
            };
            match value.get("type").and_then(|value| value.as_str()) {
                Some("continuity_compaction_auto_schedule_decided") => {
                    if value.get("decision_id").and_then(|v| v.as_str()) == Some(decision_id) {
                        assert_eq!(
                            value.get("actor_id").and_then(|v| v.as_str()),
                            Some("alice")
                        );
                        assert_eq!(value.get("origin").and_then(|v| v.as_str()), Some("cli"));
                        assert_eq!(
                            value.get("decision").and_then(|v| v.as_str()),
                            Some("scheduled")
                        );
                        saw_decision = true;
                    }
                }
                Some("continuity_job_spawned") => {
                    if value.get("job_id").and_then(|v| v.as_str()) == Some(job_id) {
                        saw_spawned = true;
                    }
                }
                Some("continuity_compaction_checkpoint_created") => {
                    if value.get("to_message_id").and_then(|v| v.as_str()) == Some(m2.as_str()) {
                        saw_checkpoint = true;
                    }
                }
                Some("continuity_job_ended") => {
                    if value.get("job_id").and_then(|v| v.as_str()) == Some(job_id) {
                        assert_eq!(
                            value.get("status").and_then(|v| v.as_str()),
                            Some("completed")
                        );
                        saw_job_ended = true;
                        break;
                    }
                }
                _ => {}
            }

            if saw_decision && saw_spawned && saw_checkpoint && saw_job_ended {
                break;
            }
        }
    })
    .await
    .expect("timeout");

    assert!(
        saw_decision,
        "expected continuity_compaction_auto_schedule_decided"
    );
    assert!(saw_spawned, "expected continuity_job_spawned");
    assert!(
        saw_checkpoint,
        "expected continuity_compaction_checkpoint_created for to_message_id={m2}"
    );
    assert!(saw_job_ended, "expected continuity_job_ended");
}

#[tokio::test]
async fn thread_compaction_auto_schedule_skips_when_inflight_and_execute_is_false() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let thread_id = ensure_thread_id(&app).await;

    let _m1 = post_thread_message(&app, &thread_id, "m1").await.message_id;
    let _m2 = post_thread_message(&app, &thread_id, "m2").await.message_id;

    let (status, first) = compaction_auto_schedule(
        &app,
        &thread_id,
        serde_json::json!({
            "stride_messages": 2,
            "max_new_checkpoints": 1,
            "block_on_inflight": true,
            "execute": false,
            "dry_run": false,
            "actor_id": "alice",
            "origin": "cli",
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::ACCEPTED);
    assert_eq!(
        first.get("decision").and_then(|value| value.as_str()),
        Some("scheduled")
    );

    let (status, second) = compaction_auto_schedule(
        &app,
        &thread_id,
        serde_json::json!({
            "stride_messages": 2,
            "max_new_checkpoints": 1,
            "block_on_inflight": true,
            "execute": false,
            "dry_run": false,
            "actor_id": "alice",
            "origin": "cli",
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(
        second.get("decision").and_then(|value| value.as_str()),
        Some("skipped_inflight")
    );
}
