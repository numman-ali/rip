use super::*;

#[tokio::test]
async fn thread_post_message_with_openresponses_emits_context_compiled() {
    use axum::extract::State;
    use axum::http::header::CONTENT_TYPE;
    use axum::routing::post;
    use axum::{response::IntoResponse, Json, Router as AxumRouter};
    use serde_json::Value;
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    #[derive(Clone)]
    struct ProviderState {
        requests: Arc<Mutex<Vec<Value>>>,
    }

    let state = ProviderState {
        requests: Arc::new(Mutex::new(Vec::new())),
    };

    let provider_app = AxumRouter::new()
        .route(
            "/v1/responses",
            post(
                |State(state): State<ProviderState>, Json(body): Json<Value>| async move {
                    state.requests.lock().expect("requests").push(body);
                    let sse =
                        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\n\
data: [DONE]\n\n";
                    ([(CONTENT_TYPE, "text/event-stream")], sse).into_response()
                },
            ),
        )
        .with_state(state.clone());

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, provider_app).await.expect("serve");
    });
    let endpoint = format!("http://{addr}/v1/responses");

    let dir = tempdir().expect("tmp");
    let app = build_test_app_with_openresponses_provider(&dir, endpoint, false);
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

    let mut saw_message = false;
    let mut saw_run_spawned = false;
    let mut saw_context_selection = false;
    let mut saw_context_compiled = false;
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
                    saw_message = true;
                }
                Some("continuity_run_spawned")
                    if value.get("message_id").and_then(|value| value.as_str())
                        == Some(posted.message_id.as_str()) =>
                {
                    saw_run_spawned = true;
                }
                Some("continuity_context_selection_decided")
                    if value.get("run_session_id").and_then(|value| value.as_str())
                        == Some(posted.session_id.as_str()) =>
                {
                    assert_eq!(
                        value.get("message_id").and_then(|value| value.as_str()),
                        Some(posted.message_id.as_str())
                    );
                    assert_eq!(
                        value.get("compiler_id").and_then(|value| value.as_str()),
                        Some("rip.context_compiler.v1")
                    );
                    assert_eq!(
                        value
                            .get("compiler_strategy")
                            .and_then(|value| value.as_str()),
                        Some("recent_messages_v1")
                    );
                    assert_eq!(
                        value
                            .get("limits")
                            .and_then(|v| v.get("recent_messages_v1_limit"))
                            .and_then(|v| v.as_u64()),
                        Some(16)
                    );
                    assert!(value.get("compaction_checkpoint").is_none());
                    assert!(
                        value.get("resets").is_none() || value.get("resets").unwrap().is_array()
                    );
                    assert_eq!(
                        value
                            .get("reason")
                            .and_then(|v| v.get("selected"))
                            .and_then(|v| v.as_str()),
                        Some("recent_messages_v1")
                    );
                    assert_eq!(
                        value.get("actor_id").and_then(|value| value.as_str()),
                        Some("alice")
                    );
                    assert_eq!(
                        value.get("origin").and_then(|value| value.as_str()),
                        Some("team")
                    );
                    saw_context_selection = true;
                }
                Some("continuity_context_compiled")
                    if value.get("run_session_id").and_then(|value| value.as_str())
                        == Some(posted.session_id.as_str()) =>
                {
                    let bundle_artifact_id = value
                        .get("bundle_artifact_id")
                        .and_then(|value| value.as_str())
                        .unwrap_or("");
                    assert!(!bundle_artifact_id.is_empty());
                    assert_eq!(
                        value.get("compiler_id").and_then(|value| value.as_str()),
                        Some("rip.context_compiler.v1")
                    );
                    assert_eq!(
                        value
                            .get("compiler_strategy")
                            .and_then(|value| value.as_str()),
                        Some("recent_messages_v1")
                    );
                    assert_eq!(
                        value
                            .get("from_message_id")
                            .and_then(|value| value.as_str()),
                        Some(posted.message_id.as_str())
                    );
                    assert_eq!(
                        value.get("actor_id").and_then(|value| value.as_str()),
                        Some("alice")
                    );
                    assert_eq!(
                        value.get("origin").and_then(|value| value.as_str()),
                        Some("team")
                    );
                    saw_context_compiled = true;
                }
                Some("continuity_run_ended")
                    if value.get("message_id").and_then(|value| value.as_str())
                        == Some(posted.message_id.as_str()) =>
                {
                    saw_run_ended = true;
                }
                _ => {}
            }

            if saw_message
                && saw_run_spawned
                && saw_context_selection
                && saw_context_compiled
                && saw_run_ended
            {
                break;
            }
        }
    })
    .await
    .expect("timeout");

    assert!(saw_message);
    assert!(saw_run_spawned);
    assert!(saw_context_selection);
    assert!(saw_context_compiled);
    assert!(saw_run_ended);

    let requests = state.requests.lock().expect("requests").clone();
    assert_eq!(requests.len(), 1);
    let first = &requests[0];
    assert!(first.get("previous_response_id").is_none());
    assert!(first.get("input").and_then(|v| v.as_array()).is_some());

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{thread_id}/context-selection-status"))
                .header("content-type", "application/json")
                .body(Body::from("{\"limit\":1}"))
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
    let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        value.get("thread_id").and_then(|v| v.as_str()),
        Some(thread_id.as_str())
    );
    let decisions = value
        .get("decisions")
        .and_then(|v| v.as_array())
        .expect("decisions");
    assert!(!decisions.is_empty());
    assert_eq!(
        decisions[0].get("run_session_id").and_then(|v| v.as_str()),
        Some(posted.session_id.as_str())
    );
}
