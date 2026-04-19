use super::*;

#[tokio::test]
async fn prompt_uses_openresponses_provider_when_configured() {
    use axum::http::header::CONTENT_TYPE;
    use axum::routing::post;
    use axum::{response::IntoResponse, Router as AxumRouter};
    use tokio::net::TcpListener;

    let sse = include_str!("../../../../../fixtures/openresponses/stream_all.sse").to_string();
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
    let session_id = create_session_id(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let send_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sessions/{session_id}/input"))
                .header("content-type", "application/json")
                .body(Body::from("{\"input\":\"hi\"}"))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(send_response.status(), axum::http::StatusCode::ACCEPTED);

    let mut saw_provider_done = false;
    let mut saw_output_delta = false;
    let mut saw_session_ended = false;

    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("provider_event") => {
                        let status = value
                            .get("status")
                            .and_then(|value| value.as_str())
                            .unwrap_or("");
                        if status == "done" {
                            saw_provider_done = true;
                        }
                    }
                    Some("output_text_delta") => saw_output_delta = true,
                    Some("session_ended") => {
                        saw_session_ended = true;
                        break;
                    }
                    _ => {}
                }

                if saw_provider_done && saw_output_delta && saw_session_ended {
                    break;
                }
            }
        }
    })
    .await
    .expect("timeout");

    assert!(saw_provider_done, "expected provider_event done");
    assert!(saw_output_delta, "expected output_text_delta");
    assert!(saw_session_ended, "expected session_ended");
}

#[tokio::test]
#[ignore]
async fn live_openresponses_smoke() {
    let endpoint = match std::env::var("RIP_OPENRESPONSES_ENDPOINT") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("skipping live test: RIP_OPENRESPONSES_ENDPOINT not set");
            return;
        }
    };
    let api_key = std::env::var("RIP_OPENRESPONSES_API_KEY").ok();
    let model = std::env::var("RIP_OPENRESPONSES_MODEL").ok();

    if api_key.is_none() {
        eprintln!("note: RIP_OPENRESPONSES_API_KEY not set (provider may reject)");
    }
    if model.is_none() {
        eprintln!("note: RIP_OPENRESPONSES_MODEL not set (provider may require a model)");
    }

    let tool_choice = match std::env::var("RIP_OPENRESPONSES_TOOL_CHOICE") {
        Ok(value) => match parse_tool_choice_env(&value) {
            Ok(choice) => choice,
            Err(err) => {
                eprintln!(
                    "invalid RIP_OPENRESPONSES_TOOL_CHOICE={value:?}: {err}; defaulting to auto"
                );
                ToolChoiceParam::auto()
            }
        },
        Err(_) => ToolChoiceParam::required(),
    };

    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace dir");

    let app = build_app_with_workspace_root_and_provider(
        data_dir,
        workspace_dir,
        Some(OpenResponsesConfig {
            endpoint,
            api_key,
            model,
            headers: Vec::new(),
            tool_choice,
            followup_user_message: None,
            stateless_history: false,
            parallel_tool_calls: false,
        }),
    );

    let session_id = create_session_id(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let prompt = "RIP live test: you MUST call tool bash with {\"command\":\"echo RIP_LIVE_TEST_OK\"} exactly once, then respond with the text: done";
    let send_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sessions/{session_id}/input"))
                .header("content-type", "application/json")
                .body(Body::from(format!(
                    "{{\"input\":{}}}",
                    serde_json::to_string(prompt).unwrap()
                )))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(send_response.status(), axum::http::StatusCode::ACCEPTED);

    let mut saw_provider_done = false;
    let mut saw_tool_started = false;
    let mut saw_tool_stdout_marker = false;
    let mut saw_tool_ended = false;
    let mut saw_session_ended = false;
    let mut last_provider_status: Option<String> = None;
    let mut last_provider_raw: Option<String> = None;
    let mut last_provider_errors: Option<Vec<String>> = None;
    let mut seen_openresponses_event_types = std::collections::BTreeSet::<String>::new();
    let mut sample_output_item_added: Option<serde_json::Value> = None;
    let mut sample_output_item_done: Option<serde_json::Value> = None;
    let mut sample_arguments_done: Option<serde_json::Value> = None;

    timeout(Duration::from_secs(60), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("provider_event") => {
                        let status = value
                            .get("status")
                            .and_then(|value| value.as_str())
                            .unwrap_or("");
                        last_provider_status = Some(status.to_string());
                        last_provider_raw = value
                            .get("raw")
                            .and_then(|value| value.as_str())
                            .map(|value| value.to_string());
                        last_provider_errors = value.get("errors").and_then(|value| {
                            value.as_array().map(|arr| {
                                arr.iter()
                                    .filter_map(|item| item.as_str().map(|s| s.to_string()))
                                    .collect::<Vec<_>>()
                            })
                        });
                        if let Some(event_type) = value
                            .get("data")
                            .and_then(|data| data.get("type"))
                            .and_then(|value| value.as_str())
                        {
                            seen_openresponses_event_types.insert(event_type.to_string());
                            if sample_output_item_added.is_none()
                                && event_type == "response.output_item.added"
                            {
                                sample_output_item_added = value.get("data").cloned();
                            }
                            if sample_output_item_done.is_none()
                                && event_type == "response.output_item.done"
                            {
                                sample_output_item_done = value.get("data").cloned();
                            }
                            if sample_arguments_done.is_none()
                                && event_type == "response.function_call_arguments.done"
                            {
                                sample_arguments_done = value.get("data").cloned();
                            }
                        }
                        if status == "done" {
                            saw_provider_done = true;
                        }
                    }
                    Some("tool_started") => saw_tool_started = true,
                    Some("tool_stdout") => {
                        if value
                            .get("chunk")
                            .and_then(|chunk| chunk.as_str())
                            .unwrap_or("")
                            .contains("RIP_LIVE_TEST_OK")
                        {
                            saw_tool_stdout_marker = true;
                        }
                    }
                    Some("tool_ended") => saw_tool_ended = true,
                    Some("session_ended") => {
                        saw_session_ended = true;
                        break;
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("timeout");

    assert!(
        saw_provider_done,
        "expected provider_event done; last provider_event status={last_provider_status:?} raw={last_provider_raw:?} errors={last_provider_errors:?}"
    );
    assert!(
        saw_tool_started && saw_tool_ended,
        "expected at least one tool execution (tool_started/tool_ended); seen openresponses event types={seen_openresponses_event_types:?}; sample output_item.added={sample_output_item_added:?}; sample output_item.done={sample_output_item_done:?}; sample arguments.done={sample_arguments_done:?}"
    );
    assert!(
        saw_tool_stdout_marker,
        "expected bash stdout marker; ensure provider/model executed bash tool"
    );
    assert!(saw_session_ended, "expected session_ended");
}

#[tokio::test]
async fn prompt_openresponses_without_done_still_ends_session() {
    use axum::http::header::CONTENT_TYPE;
    use axum::routing::post;
    use axum::{response::IntoResponse, Router as AxumRouter};
    use tokio::net::TcpListener;

    let sse_full = include_str!("../../../../../fixtures/openresponses/stream_all.sse").to_string();
    let sse = sse_full.replace("data: [DONE]\n\n", "");
    let sse = sse.trim_end_matches("\n\n").to_string();
    assert!(!sse.is_empty());

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
    let session_id = create_session_id(&app).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/sessions/{session_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let send_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sessions/{session_id}/input"))
                .header("content-type", "application/json")
                .body(Body::from("{\"input\":\"hi\"}"))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(send_response.status(), axum::http::StatusCode::ACCEPTED);

    let mut saw_provider_event = false;
    let mut saw_output_delta = false;
    let mut saw_session_ended = false;

    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("provider_event") => saw_provider_event = true,
                    Some("output_text_delta") => saw_output_delta = true,
                    Some("session_ended") => {
                        saw_session_ended = true;
                        break;
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("timeout");

    assert!(saw_provider_event, "expected provider_event");
    assert!(saw_output_delta, "expected output_text_delta");
    assert!(saw_session_ended, "expected session_ended");
}
