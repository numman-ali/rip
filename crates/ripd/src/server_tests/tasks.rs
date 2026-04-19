use super::*;

#[tokio::test]
async fn create_task_returns_id() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let task_id = create_task_id(&app, "printf 'hi\\n'").await;
    assert!(!task_id.is_empty());
}

#[tokio::test]
async fn create_task_rejects_unsupported_tool() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let payload = serde_json::json!({
        "tool": "ls",
        "args": {},
        "execution_mode": "pipes",
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/tasks")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_task_rejects_pty_mode() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let payload = serde_json::json!({
        "tool": "bash",
        "args": {"command": "printf 'hi\\n'"},
        "execution_mode": "pty",
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/tasks")
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[cfg(not(windows))]
#[tokio::test]
async fn pty_task_supports_control_operations() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app_with_task_policy(&dir, true);
    let task_id = create_task_id_with_mode(&app, "stty -echo; cat", "pty").await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/tasks/{task_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    timeout(Duration::from_secs(5), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                if value.get("type").and_then(|value| value.as_str()) == Some("tool_task_status")
                    && value.get("status").and_then(|value| value.as_str()) == Some("running")
                {
                    break;
                }
            }
        }
    })
    .await
    .expect("timeout");

    let mut saw_stdin = false;
    let mut saw_resize = false;
    let mut saw_signal = false;
    let mut saw_output = false;
    let mut saw_terminal = false;

    let write = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/tasks/{task_id}/stdin"))
                .header("content-type", "application/json")
                .body(Body::from("{\"chunk_b64\":\"aGkK\"}"))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(write.status(), axum::http::StatusCode::ACCEPTED);

    timeout(Duration::from_secs(5), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("tool_task_stdin_written") => saw_stdin = true,
                    Some("tool_task_output_delta") => {
                        if value
                            .get("chunk")
                            .and_then(|value| value.as_str())
                            .unwrap_or("")
                            .contains("hi")
                        {
                            saw_output = true;
                        }
                    }
                    Some("tool_task_status") => {
                        let status = value.get("status").and_then(|value| value.as_str());
                        if matches!(status, Some("exited") | Some("cancelled") | Some("failed")) {
                            saw_terminal = true;
                            break;
                        }
                    }
                    _ => {}
                }
                if saw_stdin && saw_output {
                    break;
                }
            }
        }
    })
    .await
    .expect("timeout");

    let resize = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/tasks/{task_id}/resize"))
                .header("content-type", "application/json")
                .body(Body::from("{\"rows\":30,\"cols\":100}"))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(resize.status(), axum::http::StatusCode::ACCEPTED);

    timeout(Duration::from_secs(5), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("tool_task_resized") => {
                        saw_resize = true;
                        break;
                    }
                    Some("tool_task_status") => {
                        let status = value.get("status").and_then(|value| value.as_str());
                        if matches!(status, Some("exited") | Some("cancelled") | Some("failed")) {
                            saw_terminal = true;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("timeout");

    let signal = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/tasks/{task_id}/signal"))
                .header("content-type", "application/json")
                .body(Body::from("{\"signal\":\"SIGTERM\"}"))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(signal.status(), axum::http::StatusCode::ACCEPTED);

    timeout(Duration::from_secs(5), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("tool_task_signalled") => saw_signal = true,
                    Some("tool_task_status") => {
                        let status = value.get("status").and_then(|value| value.as_str());
                        if matches!(status, Some("exited") | Some("cancelled") | Some("failed")) {
                            saw_terminal = true;
                        }
                    }
                    _ => {}
                }
            }
            if saw_signal && saw_terminal {
                break;
            }
        }
    })
    .await
    .expect("timeout");

    assert!(saw_stdin, "expected tool_task_stdin_written");
    assert!(saw_resize, "expected tool_task_resized");
    assert!(saw_signal, "expected tool_task_signalled");
    assert!(saw_output, "expected tool_task_output_delta");
    assert!(saw_terminal, "expected terminal tool_task_status");

    timeout(Duration::from_secs(2), async {
        loop {
            let output = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!(
                            "/tasks/{task_id}/output?stream=pty&offset_bytes=0&max_bytes=64"
                        ))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .expect("response");
            if output.status() != axum::http::StatusCode::OK {
                sleep(Duration::from_millis(10)).await;
                continue;
            }
            let body = output.into_body().collect().await.expect("body").to_bytes();
            let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
            if value
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .contains("hi")
            {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("pty output timeout");
}

#[tokio::test]
async fn list_tasks_includes_created_task() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let task_id = create_task_id(&app, "printf 'list\\n'").await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/tasks")
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
    let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let array = value.as_array().expect("array");
    assert!(array.iter().any(|entry| {
        entry
            .get("task_id")
            .and_then(|value| value.as_str())
            .map(|value| value == task_id)
            .unwrap_or(false)
    }));
}

#[tokio::test]
async fn task_status_returns_value() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let task_id = create_task_id(&app, "printf 'status\\n'").await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/tasks/{task_id}"))
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
    let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        value
            .get("task_id")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
        task_id
    );
    assert_eq!(
        value
            .get("tool")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
        "bash"
    );
}

#[tokio::test]
async fn task_status_unknown_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/tasks/unknown")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stream_task_events_unknown_task_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/tasks/unknown/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn task_events_stream_emits_output_and_termination() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let task_id = create_task_id(&app, "printf 'hello-task\\n'").await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/tasks/{task_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let mut saw_spawned = false;
    let mut saw_output = false;
    let mut saw_terminal = false;

    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("tool_task_spawned") => saw_spawned = true,
                    Some("tool_task_output_delta") => {
                        if value
                            .get("chunk")
                            .and_then(|value| value.as_str())
                            .unwrap_or("")
                            .contains("hello-task")
                        {
                            saw_output = true;
                        }
                    }
                    Some("tool_task_status") => {
                        let status = value.get("status").and_then(|value| value.as_str());
                        if matches!(status, Some("exited") | Some("cancelled") | Some("failed")) {
                            saw_terminal = true;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("timeout");

    assert!(saw_spawned, "expected tool_task_spawned");
    assert!(saw_output, "expected tool_task_output_delta");
    assert!(saw_terminal, "expected terminal tool_task_status");

    let snapshot_path = dir
        .path()
        .join("data")
        .join("task_snapshots")
        .join(format!("{task_id}.json"));
    assert!(snapshot_path.exists(), "expected task snapshot file");

    timeout(Duration::from_secs(2), async {
        loop {
            let output = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!(
                            "/tasks/{task_id}/output?stream=stdout&offset_bytes=0&max_bytes=128"
                        ))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .expect("response");
            assert_eq!(output.status(), axum::http::StatusCode::OK);
            let body = output.into_body().collect().await.expect("body").to_bytes();
            let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
            if value
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .contains("hello-task")
            {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("output");
}

#[tokio::test]
async fn task_output_supports_offset_range_reads_from_offset() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let task_id = create_task_id(&app, "printf 'abcdef'").await;

    wait_for_task_terminal(&app, &task_id).await;

    timeout(Duration::from_secs(2), async {
        loop {
            let output = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!(
                            "/tasks/{task_id}/output?stream=stdout&offset_bytes=2&max_bytes=2"
                        ))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .expect("response");
            if output.status() != axum::http::StatusCode::OK {
                sleep(Duration::from_millis(10)).await;
                continue;
            }
            let body = output.into_body().collect().await.expect("body").to_bytes();
            let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
            if value.get("content").and_then(|value| value.as_str()) == Some("cd") {
                assert_eq!(
                    value.get("offset_bytes").and_then(|value| value.as_u64()),
                    Some(2)
                );
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("stdout output timeout");
}

#[tokio::test]
async fn task_output_fetches_stderr_stream() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let task_id = create_task_id(&app, "printf 'oops\\n' >&2").await;

    wait_for_task_terminal(&app, &task_id).await;

    timeout(Duration::from_secs(2), async {
        loop {
            let output = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri(format!(
                            "/tasks/{task_id}/output?stream=stderr&offset_bytes=0&max_bytes=64"
                        ))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .expect("response");
            if output.status() != axum::http::StatusCode::OK {
                sleep(Duration::from_millis(10)).await;
                continue;
            }
            let body = output.into_body().collect().await.expect("body").to_bytes();
            let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
            if value
                .get("content")
                .and_then(|value| value.as_str())
                .unwrap_or("")
                .contains("oops")
            {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("stderr output timeout");
}

#[tokio::test]
async fn task_output_returns_bad_request_when_artifact_missing() {
    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace dir");
    let app = build_app_with_workspace_root(data_dir, workspace_dir.clone());

    let task_id = create_task_id(&app, "printf 'missing\\n'").await;
    wait_for_task_terminal(&app, &task_id).await;

    let status = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/tasks/{task_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(status.status(), axum::http::StatusCode::OK);
    let body = status.into_body().collect().await.expect("body").to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let stdout_id = value
        .get("artifacts")
        .and_then(|value| value.get("logs").or(Some(value)))
        .and_then(|value| value.get("stdout"))
        .and_then(|value| value.get("id"))
        .and_then(|value| value.as_str())
        .unwrap_or("");
    assert!(!stdout_id.is_empty(), "expected stdout artifact id");

    let artifact_path = workspace_dir
        .join(".rip")
        .join("artifacts")
        .join("blobs")
        .join(stdout_id);
    assert!(artifact_path.exists(), "expected artifact file");
    fs::remove_file(&artifact_path).expect("remove artifact");

    let output = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/tasks/{task_id}/output?stream=stdout&offset_bytes=0&max_bytes=64"
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(output.status(), axum::http::StatusCode::BAD_REQUEST);
}

#[cfg(not(windows))]
#[tokio::test]
async fn cancel_task_emits_cancelled_status() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let task_id = create_task_id(&app, "sleep 5").await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/tasks/{task_id}/events"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let mut reader = TestSseReader::new(response.into_body());

    let mut saw_running = false;
    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                if value.get("type").and_then(|value| value.as_str()) == Some("tool_task_status")
                    && value.get("status").and_then(|value| value.as_str()) == Some("running")
                {
                    saw_running = true;
                    break;
                }
            }
        }
    })
    .await
    .expect("timeout");
    assert!(saw_running, "expected running status");

    let cancel = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/tasks/{task_id}/cancel"))
                .header("content-type", "application/json")
                .body(Body::from("{\"reason\":\"stop\"}"))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(cancel.status(), axum::http::StatusCode::ACCEPTED);

    let mut saw_cancel_requested = false;
    let mut saw_cancelled = false;
    let mut saw_terminal = false;

    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("tool_task_cancel_requested") => saw_cancel_requested = true,
                    Some("tool_task_cancelled") => saw_cancelled = true,
                    Some("tool_task_status") => {
                        if value.get("status").and_then(|value| value.as_str()) == Some("cancelled")
                        {
                            saw_terminal = true;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("timeout");

    assert!(saw_cancel_requested, "expected tool_task_cancel_requested");
    assert!(saw_cancelled, "expected tool_task_cancelled");
    assert!(saw_terminal, "expected cancelled status");

    let snapshot_path = dir
        .path()
        .join("data")
        .join("task_snapshots")
        .join(format!("{task_id}.json"));
    assert!(snapshot_path.exists(), "expected task snapshot file");
}
