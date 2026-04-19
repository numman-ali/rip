use super::*;

#[tokio::test]
async fn thread_branch_creates_child_and_emits_branch_event() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let parent_thread_id = ensure_thread_id(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/threads/{parent_thread_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let first_message = reader.next_data_message().await.expect("created");
    let first_value = extract_data_json(&first_message).expect("json");
    assert_eq!(
        first_value.get("type").and_then(|value| value.as_str()),
        Some("continuity_created")
    );

    let posted = post_thread_message(&app, &parent_thread_id, "hello").await;
    let from_message_id = posted.message_id.clone();

    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            let Some(value) = extract_data_json(&message) else {
                continue;
            };
            if value.get("type").and_then(|value| value.as_str()) == Some("continuity_run_ended")
                && value.get("message_id").and_then(|value| value.as_str())
                    == Some(from_message_id.as_str())
            {
                break;
            }
        }
    })
    .await
    .expect("run ended timeout");

    let branched = branch_thread(
        &app,
        &parent_thread_id,
        serde_json::json!({
            "title": "child",
            "from_message_id": from_message_id.clone(),
            "actor_id": "alice",
            "origin": "team",
        }),
    )
    .await;

    assert_eq!(branched.parent_thread_id, parent_thread_id);
    assert_eq!(branched.parent_seq, 3);
    assert_eq!(
        branched.parent_message_id.as_deref(),
        Some(from_message_id.as_str())
    );
    assert!(!branched.thread_id.is_empty());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/threads/{}/events", branched.thread_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut child_reader = TestSseReader::new(response.into_body());

    let created_message = child_reader.next_data_message().await.expect("created");
    let created_value = extract_data_json(&created_message).expect("json");
    assert_eq!(
        created_value.get("type").and_then(|value| value.as_str()),
        Some("continuity_created")
    );

    let branch_message = child_reader.next_data_message().await.expect("branched");
    let branch_value = extract_data_json(&branch_message).expect("json");
    assert_eq!(
        branch_value.get("type").and_then(|value| value.as_str()),
        Some("continuity_branched")
    );
    assert_eq!(
        branch_value
            .get("parent_thread_id")
            .and_then(|value| value.as_str()),
        Some(parent_thread_id.as_str())
    );
    assert_eq!(
        branch_value
            .get("parent_seq")
            .and_then(|value| value.as_u64()),
        Some(3)
    );
    assert_eq!(
        branch_value
            .get("actor_id")
            .and_then(|value| value.as_str()),
        Some("alice")
    );
    assert_eq!(
        branch_value.get("origin").and_then(|value| value.as_str()),
        Some("team")
    );
}

#[tokio::test]
async fn thread_branch_unknown_is_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);

    let payload = serde_json::json!({
        "title": "child",
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/threads/missing-thread-id/branch")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn thread_branch_rejects_invalid_from_seq() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let parent_thread_id = ensure_thread_id(&app).await;

    let payload = serde_json::json!({
        "from_seq": 999,
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{parent_thread_id}/branch"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn thread_handoff_creates_child_and_emits_handoff_event() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let from_thread_id = ensure_thread_id(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/threads/{from_thread_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let first_message = reader.next_data_message().await.expect("created");
    let first_value = extract_data_json(&first_message).expect("json");
    assert_eq!(
        first_value.get("type").and_then(|value| value.as_str()),
        Some("continuity_created")
    );

    let posted = post_thread_message(&app, &from_thread_id, "hello").await;
    let from_message_id = posted.message_id.clone();

    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            let Some(value) = extract_data_json(&message) else {
                continue;
            };
            if value.get("type").and_then(|value| value.as_str()) == Some("continuity_run_ended")
                && value.get("message_id").and_then(|value| value.as_str())
                    == Some(from_message_id.as_str())
            {
                break;
            }
        }
    })
    .await
    .expect("run ended timeout");

    let handed = handoff_thread(
        &app,
        &from_thread_id,
        serde_json::json!({
            "title": "handoff",
            "summary_markdown": "# Summary\n\n- hello",
            "from_message_id": from_message_id.clone(),
            "actor_id": "alice",
            "origin": "team",
        }),
    )
    .await;

    assert_eq!(handed.from_thread_id, from_thread_id);
    assert_eq!(handed.from_seq, 3);
    assert_eq!(
        handed.from_message_id.as_deref(),
        Some(from_message_id.as_str())
    );
    assert!(!handed.thread_id.is_empty());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/threads/{}/events", handed.thread_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut child_reader = TestSseReader::new(response.into_body());

    let created_message = child_reader.next_data_message().await.expect("created");
    let created_value = extract_data_json(&created_message).expect("json");
    assert_eq!(
        created_value.get("type").and_then(|value| value.as_str()),
        Some("continuity_created")
    );

    let handoff_message = child_reader.next_data_message().await.expect("handoff");
    let handoff_value = extract_data_json(&handoff_message).expect("json");
    assert_eq!(
        handoff_value.get("type").and_then(|value| value.as_str()),
        Some("continuity_handoff_created")
    );
    assert_eq!(
        handoff_value
            .get("from_thread_id")
            .and_then(|value| value.as_str()),
        Some(from_thread_id.as_str())
    );
    assert_eq!(
        handoff_value
            .get("from_seq")
            .and_then(|value| value.as_u64()),
        Some(3)
    );
    assert_eq!(
        handoff_value
            .get("summary_markdown")
            .and_then(|value| value.as_str()),
        Some("# Summary\n\n- hello")
    );
    assert_eq!(
        handoff_value
            .get("actor_id")
            .and_then(|value| value.as_str()),
        Some("alice")
    );
    assert_eq!(
        handoff_value.get("origin").and_then(|value| value.as_str()),
        Some("team")
    );
}

#[tokio::test]
async fn thread_handoff_unknown_is_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);

    let payload = serde_json::json!({
        "summary_markdown": "summary",
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/threads/missing-thread-id/handoff")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn thread_handoff_rejects_invalid_from_seq() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let from_thread_id = ensure_thread_id(&app).await;

    let payload = serde_json::json!({
        "summary_markdown": "summary",
        "from_seq": 999,
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{from_thread_id}/handoff"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn thread_handoff_rejects_missing_summary() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let from_thread_id = ensure_thread_id(&app).await;

    let payload = serde_json::json!({
        "title": "handoff",
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{from_thread_id}/handoff"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}
