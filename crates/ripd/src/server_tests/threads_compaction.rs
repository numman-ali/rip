use super::*;

#[tokio::test]
async fn thread_compaction_checkpoint_unknown_is_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);

    let payload = serde_json::json!({
        "summary_markdown": "summary"
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/threads/nope/compaction-checkpoint")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn thread_compaction_checkpoint_rejects_missing_summary() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let thread_id = ensure_thread_id(&app).await;
    let posted = post_thread_message(&app, &thread_id, "hello").await;

    let payload = serde_json::json!({
        "to_message_id": posted.message_id,
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{thread_id}/compaction-checkpoint"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn thread_compaction_checkpoint_creates_and_emits_checkpoint_event() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let thread_id = ensure_thread_id(&app).await;
    let posted = post_thread_message(&app, &thread_id, "hello").await;

    let created = compaction_checkpoint(
        &app,
        &thread_id,
        serde_json::json!({
            "summary_markdown": "summary",
            "to_message_id": posted.message_id,
        }),
    )
    .await;
    assert_eq!(created.thread_id, thread_id);
    assert!(!created.checkpoint_id.is_empty());
    assert!(!created.summary_artifact_id.is_empty());

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
    let found = timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            let Some(value) = extract_data_json(&message) else {
                continue;
            };
            if value.get("type").and_then(|value| value.as_str())
                != Some("continuity_compaction_checkpoint_created")
            {
                continue;
            }
            return Some(value);
        }
        None
    })
    .await
    .expect("timeout");
    let found = found.expect("checkpoint event");

    assert_eq!(
        found.get("checkpoint_id").and_then(|value| value.as_str()),
        Some(created.checkpoint_id.as_str())
    );
    assert_eq!(
        found
            .get("summary_artifact_id")
            .and_then(|value| value.as_str()),
        Some(created.summary_artifact_id.as_str())
    );
    assert_eq!(
        found.get("to_seq").and_then(|value| value.as_u64()),
        Some(created.to_seq)
    );
    assert_eq!(
        found.get("to_message_id").and_then(|value| value.as_str()),
        Some(created.to_message_id.as_str())
    );
}

#[tokio::test]
async fn thread_provider_cursor_status_and_rotate_are_auditable() {
    use axum::http::header::CONTENT_TYPE;
    use axum::routing::post;
    use axum::{response::IntoResponse, Router as AxumRouter};
    use tokio::net::TcpListener;

    let sse = "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_thread_1\"}}\n\n\
data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n\
data: [DONE]\n\n"
        .to_string();
    let provider_app = AxumRouter::new().route(
        "/v1/responses",
        post(move || {
            let body = sse.clone();
            async move { ([(CONTENT_TYPE, "text/event-stream")], body).into_response() }
        }),
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, provider_app).await.expect("serve");
    });
    let endpoint = format!("http://{addr}/v1/responses");

    let dir = tempdir().expect("tmp");
    let app = build_test_app_with_openresponses_provider(&dir, endpoint, false);
    let thread_id = ensure_thread_id(&app).await;
    let posted = post_thread_message(&app, &thread_id, "hello").await;

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

    let cursor_set = timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            let Some(value) = extract_data_json(&message) else {
                continue;
            };
            if value.get("type").and_then(|value| value.as_str())
                != Some("continuity_provider_cursor_updated")
            {
                continue;
            }
            if value.get("action").and_then(|value| value.as_str()) != Some("set") {
                continue;
            }
            return Some(value);
        }
        None
    })
    .await
    .expect("timeout")
    .expect("cursor set event");

    assert_eq!(
        cursor_set.get("provider").and_then(|value| value.as_str()),
        Some("openresponses")
    );
    assert_eq!(
        cursor_set
            .get("run_session_id")
            .and_then(|value| value.as_str()),
        Some(posted.session_id.as_str())
    );
    assert_eq!(
        cursor_set
            .get("cursor")
            .and_then(|value| value.get("previous_response_id"))
            .and_then(|value| value.as_str()),
        Some("resp_thread_1")
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{thread_id}/provider-cursor-status"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let status_body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let status_json: serde_json::Value = serde_json::from_slice(&status_body).expect("json");
    assert_eq!(
        status_json
            .get("active")
            .and_then(|value| value.get("action"))
            .and_then(|value| value.as_str()),
        Some("set")
    );

    let rotate_payload = serde_json::json!({
        "provider": null,
        "endpoint": null,
        "model": null,
        "reason": "test",
        "actor_id": "user",
        "origin": "server"
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{thread_id}/provider-cursor-rotate"))
                .header("content-type", "application/json")
                .body(Body::from(rotate_payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let rotate_body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let rotate_json: serde_json::Value = serde_json::from_slice(&rotate_body).expect("json");
    assert_eq!(
        rotate_json.get("rotated").and_then(|value| value.as_bool()),
        Some(true)
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{thread_id}/provider-cursor-status"))
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let status_body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let status_json: serde_json::Value = serde_json::from_slice(&status_body).expect("json");
    assert_eq!(
        status_json
            .get("active")
            .and_then(|value| value.get("action"))
            .and_then(|value| value.as_str()),
        Some("rotated")
    );
    assert!(
        status_json
            .get("active")
            .and_then(|value| value.get("cursor"))
            .is_none()
            || status_json
                .get("active")
                .and_then(|value| value.get("cursor"))
                .map(|cursor| cursor.is_null())
                .unwrap_or(false)
    );
}

#[tokio::test]
async fn thread_compaction_cut_points_returns_latest_first_and_respects_limit() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let thread_id = ensure_thread_id(&app).await;

    let m1 = post_thread_message(&app, &thread_id, "m1").await.message_id;
    let m2 = post_thread_message(&app, &thread_id, "m2").await.message_id;
    let _m3 = post_thread_message(&app, &thread_id, "m3").await.message_id;
    let m4 = post_thread_message(&app, &thread_id, "m4").await.message_id;

    let value = compaction_cut_points(
        &app,
        &thread_id,
        serde_json::json!({
            "stride_messages": 2,
            "limit": 3,
        }),
    )
    .await;

    assert_eq!(
        value.get("thread_id").and_then(|value| value.as_str()),
        Some(thread_id.as_str())
    );
    assert_eq!(
        value.get("stride_messages").and_then(|v| v.as_u64()),
        Some(2)
    );
    assert_eq!(value.get("message_count").and_then(|v| v.as_u64()), Some(4));
    assert_eq!(
        value.get("cut_rule_id").and_then(|v| v.as_str()),
        Some("stride_messages_v1/2")
    );

    let cut_points = value
        .get("cut_points")
        .and_then(|value| value.as_array())
        .expect("cut_points array");
    assert_eq!(cut_points.len(), 2, "expected ordinals 4 and 2");
    assert_eq!(
        cut_points[0]
            .get("target_message_ordinal")
            .and_then(|v| v.as_u64()),
        Some(4)
    );
    assert_eq!(
        cut_points[0].get("to_message_id").and_then(|v| v.as_str()),
        Some(m4.as_str())
    );
    assert_eq!(
        cut_points[0]
            .get("already_checkpointed")
            .and_then(|v| v.as_bool()),
        Some(false)
    );
    assert!(cut_points[0]
        .get("latest_checkpoint_id")
        .and_then(|v| v.as_str())
        .is_none());

    assert_eq!(
        cut_points[1]
            .get("target_message_ordinal")
            .and_then(|v| v.as_u64()),
        Some(2)
    );
    assert_eq!(
        cut_points[1].get("to_message_id").and_then(|v| v.as_str()),
        Some(m2.as_str())
    );
    assert_eq!(
        cut_points[1]
            .get("already_checkpointed")
            .and_then(|v| v.as_bool()),
        Some(false)
    );
    assert!(cut_points[1]
        .get("latest_checkpoint_id")
        .and_then(|v| v.as_str())
        .is_none());

    assert!(
        cut_points[0]
            .get("to_seq")
            .and_then(|v| v.as_u64())
            .unwrap_or_default()
            > cut_points[1]
                .get("to_seq")
                .and_then(|v| v.as_u64())
                .unwrap_or_default()
    );

    assert_eq!(m1.len(), 36);
}

#[tokio::test]
async fn thread_compaction_cut_points_marks_already_checkpointed() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let thread_id = ensure_thread_id(&app).await;

    let _m1 = post_thread_message(&app, &thread_id, "m1").await.message_id;
    let m2 = post_thread_message(&app, &thread_id, "m2").await.message_id;
    let created = compaction_checkpoint(
        &app,
        &thread_id,
        serde_json::json!({
            "summary_markdown": "summary",
            "to_message_id": m2,
        }),
    )
    .await;

    let value = compaction_cut_points(
        &app,
        &thread_id,
        serde_json::json!({
            "stride_messages": 2,
            "limit": 1,
        }),
    )
    .await;

    let cut_points = value
        .get("cut_points")
        .and_then(|value| value.as_array())
        .expect("cut_points array");
    assert_eq!(cut_points.len(), 1);
    assert_eq!(
        cut_points[0]
            .get("already_checkpointed")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        cut_points[0]
            .get("latest_checkpoint_id")
            .and_then(|v| v.as_str()),
        Some(created.checkpoint_id.as_str())
    );
}

#[tokio::test]
async fn thread_compaction_status_reports_next_cut_point_and_last_decision() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let thread_id = ensure_thread_id(&app).await;

    let _m1 = post_thread_message(&app, &thread_id, "m1").await.message_id;
    let m2 = post_thread_message(&app, &thread_id, "m2").await.message_id;

    let status = compaction_status(
        &app,
        &thread_id,
        serde_json::json!({
            "stride_messages": 1,
        }),
    )
    .await;
    assert_eq!(
        status.get("message_count").and_then(|v| v.as_u64()),
        Some(2)
    );
    assert!(
        status.get("latest_checkpoint").is_none()
            || status.get("latest_checkpoint").unwrap().is_null()
    );
    assert_eq!(
        status
            .get("next_cut_point")
            .and_then(|v| v.get("to_message_id"))
            .and_then(|v| v.as_str()),
        Some(m2.as_str())
    );
    assert!(
        status.get("last_schedule_decision").is_none()
            || status.get("last_schedule_decision").unwrap().is_null()
    );

    let (_status_code, scheduled) = compaction_auto_schedule(
        &app,
        &thread_id,
        serde_json::json!({
            "stride_messages": 1,
            "max_new_checkpoints": 1,
            "block_on_inflight": true,
            "execute": false,
            "dry_run": false,
            "actor_id": "alice",
            "origin": "cli",
        }),
    )
    .await;
    assert_eq!(
        scheduled.get("decision").and_then(|v| v.as_str()),
        Some("scheduled")
    );

    let after = compaction_status(
        &app,
        &thread_id,
        serde_json::json!({
            "stride_messages": 1,
        }),
    )
    .await;
    assert_eq!(
        after
            .get("last_schedule_decision")
            .and_then(|v| v.get("decision"))
            .and_then(|v| v.as_str()),
        Some("scheduled")
    );
    assert!(
        after
            .get("inflight_job_id")
            .and_then(|v| v.as_str())
            .is_some(),
        "expected inflight_job_id"
    );
}
