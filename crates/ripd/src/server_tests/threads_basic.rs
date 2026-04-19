use super::*;

#[tokio::test]
async fn thread_list_empty_initially() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/threads")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let threads: Vec<ThreadMeta> = serde_json::from_slice(&body).expect("json");
    assert!(threads.is_empty());
}

#[tokio::test]
async fn thread_ensure_is_idempotent_and_listed() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);

    let first = ensure_thread_id(&app).await;
    let second = ensure_thread_id(&app).await;
    assert_eq!(first, second);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/threads")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let threads: Vec<ThreadMeta> = serde_json::from_slice(&body).expect("json");
    assert!(threads.iter().any(|meta| meta.thread_id == first));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/threads/{first}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let meta: ThreadMeta = serde_json::from_slice(&body).expect("json");
    assert_eq!(meta.thread_id, first);
}

#[tokio::test]
async fn thread_get_unknown_is_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/threads/missing-thread-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn thread_post_message_unknown_is_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);

    let payload = serde_json::json!({
        "content": "hello",
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/threads/missing-thread-id/messages")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn thread_stream_events_unknown_is_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/threads/missing-thread-id/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn thread_ensure_returns_500_when_index_parent_is_file() {
    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace dir");
    let app = build_app_with_workspace_root(data_dir.clone(), workspace_dir);

    fs::create_dir_all(&data_dir).expect("data dir");
    fs::write(data_dir.join("continuities"), "not a directory").expect("continuities file");

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/threads/ensure")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(
        response.status(),
        axum::http::StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[tokio::test]
async fn thread_stream_events_emits_created_and_message() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let thread_id = ensure_thread_id(&app).await;

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

    let first_message = reader.next_data_message().await.expect("created");
    let first_value = extract_data_json(&first_message).expect("json");
    assert_eq!(
        first_value.get("type").and_then(|value| value.as_str()),
        Some("continuity_created")
    );

    let _posted = post_thread_message(&app, &thread_id, "hello").await;

    let mut saw_appended = false;
    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            let Some(value) = extract_data_json(&message) else {
                continue;
            };
            if value.get("type").and_then(|value| value.as_str())
                == Some("continuity_message_appended")
                && value.get("content").and_then(|value| value.as_str()) == Some("hello")
            {
                saw_appended = true;
                break;
            }
        }
    })
    .await
    .expect("message timeout");
    assert!(saw_appended, "expected continuity_message_appended");
}

#[tokio::test]
async fn thread_post_message_preserves_actor_and_origin() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let thread_id = ensure_thread_id(&app).await;

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

    let first_message = reader.next_data_message().await.expect("created");
    let first_value = extract_data_json(&first_message).expect("json");
    assert_eq!(
        first_value.get("type").and_then(|value| value.as_str()),
        Some("continuity_created")
    );

    let payload = serde_json::json!({
        "content": "hello",
        "actor_id": "alice",
        "origin": "team",
    });
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{thread_id}/messages"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::ACCEPTED);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let posted: ThreadPostMessageResponse = serde_json::from_slice(&body).expect("json");
    assert_eq!(posted.thread_id, thread_id);
    assert!(!posted.message_id.is_empty());
    assert!(!posted.session_id.is_empty());

    let mut saw_appended = false;
    let mut saw_run_spawned = false;
    let mut saw_run_ended = false;
    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            let Some(value) = extract_data_json(&message) else {
                continue;
            };
            match value.get("type").and_then(|value| value.as_str()) {
                Some("continuity_message_appended")
                    if value.get("content").and_then(|value| value.as_str()) == Some("hello") =>
                {
                    assert_eq!(
                        value.get("actor_id").and_then(|value| value.as_str()),
                        Some("alice")
                    );
                    assert_eq!(
                        value.get("origin").and_then(|value| value.as_str()),
                        Some("team")
                    );
                    saw_appended = true;
                }
                Some("continuity_run_spawned")
                    if value.get("message_id").and_then(|value| value.as_str())
                        == Some(posted.message_id.as_str()) =>
                {
                    assert_eq!(
                        value.get("run_session_id").and_then(|value| value.as_str()),
                        Some(posted.session_id.as_str())
                    );
                    assert_eq!(
                        value.get("actor_id").and_then(|value| value.as_str()),
                        Some("alice")
                    );
                    assert_eq!(
                        value.get("origin").and_then(|value| value.as_str()),
                        Some("team")
                    );
                    saw_run_spawned = true;
                }
                Some("continuity_run_ended")
                    if value.get("message_id").and_then(|value| value.as_str())
                        == Some(posted.message_id.as_str()) =>
                {
                    assert_eq!(
                        value.get("run_session_id").and_then(|value| value.as_str()),
                        Some(posted.session_id.as_str())
                    );
                    assert_eq!(
                        value.get("actor_id").and_then(|value| value.as_str()),
                        Some("alice")
                    );
                    assert_eq!(
                        value.get("origin").and_then(|value| value.as_str()),
                        Some("team")
                    );
                    assert_eq!(
                        value.get("reason").and_then(|value| value.as_str()),
                        Some("completed")
                    );
                    saw_run_ended = true;
                }
                _ => {}
            }

            if saw_appended && saw_run_spawned && saw_run_ended {
                break;
            }
        }
    })
    .await
    .expect("message timeout");
    assert!(saw_appended, "expected continuity_message_appended");
    assert!(saw_run_spawned, "expected continuity_run_spawned");
    assert!(saw_run_ended, "expected continuity_run_ended");
}
