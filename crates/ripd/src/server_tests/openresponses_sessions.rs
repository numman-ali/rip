use super::*;

mod errors;
mod smoke;
mod tool_loop;

async fn run_openresponses_tool_loop_fixture(first_sse: &'static str, stateless_history: bool) {
    use axum::extract::State;
    use axum::http::header::CONTENT_TYPE;
    use axum::routing::post;
    use axum::{response::IntoResponse, Json, Router as AxumRouter};
    use serde_json::Value;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    let second_sse = include_str!("../../../../fixtures/openresponses/tool_loop_followup.sse");

    #[derive(Clone)]
    struct ProviderState {
        requests: Arc<Mutex<Vec<Value>>>,
        call_count: Arc<AtomicUsize>,
        first_sse: &'static str,
        second_sse: &'static str,
    }

    let state = ProviderState {
        requests: Arc::new(Mutex::new(Vec::new())),
        call_count: Arc::new(AtomicUsize::new(0)),
        first_sse,
        second_sse,
    };

    let provider_app = AxumRouter::new()
        .route(
            "/v1/responses",
            post(
                |State(state): State<ProviderState>, Json(body): Json<Value>| async move {
                    state.requests.lock().expect("requests").push(body);
                    let idx = state.call_count.fetch_add(1, Ordering::SeqCst);
                    let sse_body = if idx == 0 {
                        state.first_sse
                    } else {
                        state.second_sse
                    };
                    ([(CONTENT_TYPE, "text/event-stream")], sse_body).into_response()
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
    let app = build_test_app_with_openresponses_provider(&dir, endpoint, stateless_history);
    let session_id = create_session_id(&app).await;
    let workspace_root = dir.path().join("workspace");
    std::fs::write(workspace_root.join("a.txt"), "one\ntwo\n").expect("seed workspace");

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

    let mut saw_tool_started = false;
    let mut saw_tool_ended = false;
    let mut saw_session_ended = false;

    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("tool_started") => saw_tool_started = true,
                    Some("tool_ended") => saw_tool_ended = true,
                    Some("session_ended") => {
                        saw_session_ended = true;
                        break;
                    }
                    _ => {}
                }
                if saw_tool_started && saw_tool_ended && saw_session_ended {
                    break;
                }
            }
        }
    })
    .await
    .expect("timeout");

    assert!(saw_tool_started, "expected tool_started");
    assert!(saw_tool_ended, "expected tool_ended");
    assert!(saw_session_ended, "expected session_ended");

    let written = dir.path().join("workspace").join("a.txt");
    assert_eq!(
        std::fs::read_to_string(&written).expect("file"),
        "ONE\ntwo\n"
    );

    let requests = state.requests.lock().expect("requests").clone();
    assert_eq!(requests.len(), 2, "expected two provider requests");
    if stateless_history {
        let first_input_items = requests[0]
            .get("input")
            .and_then(|value| value.as_array())
            .expect("first input array");
        assert_eq!(
            first_input_items
                .first()
                .and_then(|value| value.get("type"))
                .and_then(|value| value.as_str()),
            Some("message")
        );
        assert_eq!(
            first_input_items
                .first()
                .and_then(|value| value.get("role"))
                .and_then(|value| value.as_str()),
            Some("user")
        );
    } else {
        assert_eq!(
            requests[0].get("input").and_then(|value| value.as_str()),
            Some("hi")
        );
    }

    let input_items = requests[1]
        .get("input")
        .and_then(|value| value.as_array())
        .expect("followup input array");

    if stateless_history {
        assert!(requests[1].get("previous_response_id").is_none());
        assert_eq!(
            input_items
                .first()
                .and_then(|value| value.get("type"))
                .and_then(|value| value.as_str()),
            Some("message")
        );
        assert_eq!(
            input_items
                .first()
                .and_then(|value| value.get("role"))
                .and_then(|value| value.as_str()),
            Some("user")
        );

        let function_call_item = input_items.get(1).expect("function_call item");
        assert_eq!(
            function_call_item
                .get("type")
                .and_then(|value| value.as_str()),
            Some("function_call")
        );
        assert_eq!(
            function_call_item
                .get("call_id")
                .and_then(|value| value.as_str()),
            Some("call_1")
        );

        let tool_output_item = input_items.get(2).expect("tool output item");
        assert_eq!(
            tool_output_item
                .get("type")
                .and_then(|value| value.as_str()),
            Some("function_call_output")
        );
        assert_eq!(
            tool_output_item
                .get("call_id")
                .and_then(|value| value.as_str()),
            Some("call_1")
        );
        assert_eq!(
            tool_output_item.get("id").and_then(|value| value.as_str()),
            Some("output_call_1")
        );

        let output = tool_output_item
            .get("output")
            .and_then(|value| value.as_str())
            .expect("output");
        let parsed: serde_json::Value = serde_json::from_str(output).expect("output json");
        assert_eq!(
            parsed.get("tool").and_then(|value| value.as_str()),
            Some("apply_patch")
        );
        assert_eq!(
            parsed.get("ok").and_then(|value| value.as_bool()),
            Some(true)
        );
    } else {
        assert_eq!(
            requests[1]
                .get("previous_response_id")
                .and_then(|value| value.as_str()),
            Some("resp_1")
        );
        let tool_output_item = input_items.first().expect("tool output item");
        assert_eq!(
            tool_output_item
                .get("type")
                .and_then(|value| value.as_str()),
            Some("function_call_output")
        );
        assert_eq!(
            tool_output_item
                .get("call_id")
                .and_then(|value| value.as_str()),
            Some("call_1")
        );
        let output = tool_output_item
            .get("output")
            .and_then(|value| value.as_str())
            .expect("output");
        let parsed: serde_json::Value = serde_json::from_str(output).expect("output json");
        assert_eq!(
            parsed.get("tool").and_then(|value| value.as_str()),
            Some("apply_patch")
        );
        assert_eq!(
            parsed.get("ok").and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    let data_dir = dir.path().join("data");
    let snapshot_path = data_dir
        .join("snapshots")
        .join(format!("{session_id}.json"));
    let log_path = data_dir.join("events.jsonl");
    timeout(Duration::from_secs(2), async {
        loop {
            let snapshot_ready = snapshot_path.exists();
            let log_ready = log_path
                .metadata()
                .map(|meta| meta.len() > 0)
                .unwrap_or(false);
            if snapshot_ready && log_ready {
                break;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("snapshot timeout");
    let log = rip_log::EventLog::new(log_path).expect("event log");
    rip_log::verify_snapshot(&log, snapshot_path).expect("verify snapshot");
}
