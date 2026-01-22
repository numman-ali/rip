use axum::body::Body;
use axum::http::Request;
use axum::Router;
use http_body_util::BodyExt;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;
use tokio::time::{sleep, timeout, Duration};
use tower::util::ServiceExt;

use rip_provider_openresponses::ToolChoiceParam;

use crate::provider_openresponses::{parse_tool_choice_env, OpenResponsesConfig};
use crate::server::{
    build_app_with_workspace_root, build_app_with_workspace_root_and_provider,
    build_app_with_workspace_root_and_provider_and_task_policy, build_openapi_router,
    workspace_root, SessionCreated, ThreadEnsureResponse, ThreadMeta, ThreadPostMessageResponse,
};

fn build_test_app(dir: &tempfile::TempDir) -> Router {
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace dir");
    build_app_with_workspace_root(data_dir, workspace_dir)
}

fn build_test_app_with_task_policy(dir: &tempfile::TempDir, allow_pty_tasks: bool) -> Router {
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace dir");
    build_app_with_workspace_root_and_provider_and_task_policy(
        data_dir,
        workspace_dir,
        None,
        allow_pty_tasks,
    )
}

fn build_test_app_with_openresponses_provider(
    dir: &tempfile::TempDir,
    endpoint: String,
    stateless_history: bool,
) -> Router {
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace dir");
    build_app_with_workspace_root_and_provider(
        data_dir,
        workspace_dir,
        Some(OpenResponsesConfig {
            endpoint,
            api_key: None,
            model: Some("fixture-model".to_string()),
            tool_choice: ToolChoiceParam::auto(),
            followup_user_message: None,
            stateless_history,
            parallel_tool_calls: false,
        }),
    )
}

async fn create_session_id(app: &Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let payload: SessionCreated = serde_json::from_slice(&body).expect("json");
    payload.session_id
}

async fn create_task_id(app: &Router, command: &str) -> String {
    create_task_id_with_mode(app, command, "pipes").await
}

async fn create_task_id_with_mode(app: &Router, command: &str, execution_mode: &str) -> String {
    let payload = serde_json::json!({
        "tool": "bash",
        "args": {
            "command": command
        },
        "title": "test-task",
        "execution_mode": execution_mode,
    });

    let response = app
        .clone()
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
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
    value
        .get("task_id")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

async fn ensure_thread_id(app: &Router) -> String {
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
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let payload: ThreadEnsureResponse = serde_json::from_slice(&body).expect("json");
    payload.thread_id
}

async fn post_thread_message(
    app: &Router,
    thread_id: &str,
    content: &str,
) -> ThreadPostMessageResponse {
    let payload = serde_json::json!({
        "content": content,
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
    serde_json::from_slice(&body).expect("json")
}

async fn wait_for_task_terminal(app: &Router, task_id: &str) {
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

    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                if value.get("type").and_then(|value| value.as_str()) == Some("tool_task_status") {
                    let status = value.get("status").and_then(|value| value.as_str());
                    if matches!(status, Some("exited") | Some("cancelled") | Some("failed")) {
                        break;
                    }
                }
            }
        }
    })
    .await
    .expect("timeout");
}

#[test]
fn workspace_root_returns_value() {
    let root = workspace_root();
    let func: fn() -> PathBuf = workspace_root;
    let pointer_root = func();
    assert!(!root.as_os_str().is_empty());
    assert!(!pointer_root.as_os_str().is_empty());
}

#[tokio::test]
async fn openapi_spec_served() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(content_type.starts_with("application/json"));
    let body = response.into_body().collect().await.expect("body");
    let bytes = body.to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert!(value
        .get("paths")
        .and_then(|paths| paths.get("/sessions"))
        .is_some());
}

#[test]
fn openapi_snapshot_matches() {
    let json = build_openapi_router().1;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let path = root.join("schemas/ripd/openapi.json");
    if std::env::var("RIPD_UPDATE_OPENAPI").is_ok() {
        std::fs::create_dir_all(path.parent().expect("dir")).expect("mkdir");
        std::fs::write(&path, json).expect("write");
        return;
    }
    let existing = std::fs::read_to_string(&path).expect("snapshot missing");
    assert_eq!(existing, json);
}

#[tokio::test]
async fn create_session_returns_id() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let session_id = create_session_id(&app).await;
    assert!(!session_id.is_empty());
}

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
    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            let Some(value) = extract_data_json(&message) else {
                continue;
            };
            if value.get("type").and_then(|value| value.as_str())
                == Some("continuity_message_appended")
                && value.get("content").and_then(|value| value.as_str()) == Some("hello")
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
                break;
            }
        }
    })
    .await
    .expect("message timeout");
    assert!(saw_appended, "expected continuity_message_appended");
}

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

    let mut saw_stdin = false;
    let mut saw_resize = false;
    let mut saw_signal = false;
    let mut saw_output = false;
    let mut saw_terminal = false;

    timeout(Duration::from_secs(5), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("tool_task_stdin_written") => saw_stdin = true,
                    Some("tool_task_resized") => saw_resize = true,
                    Some("tool_task_signalled") => saw_signal = true,
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

    let output = app
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
    assert_eq!(output.status(), axum::http::StatusCode::OK);
    let body = output.into_body().collect().await.expect("body").to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert!(value
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .contains("hi"));
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

    let output = app
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
    assert_eq!(output.status(), axum::http::StatusCode::OK);
    let body = output.into_body().collect().await.expect("body").to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(
        value
            .get("content")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
        "cd"
    );
    assert_eq!(
        value
            .get("offset_bytes")
            .and_then(|value| value.as_u64())
            .unwrap_or_default(),
        2
    );
}

#[tokio::test]
async fn task_output_fetches_stderr_stream() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let task_id = create_task_id(&app, "printf 'oops\\n' >&2").await;

    wait_for_task_terminal(&app, &task_id).await;

    let output = app
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
    assert_eq!(output.status(), axum::http::StatusCode::OK);
    let body = output.into_body().collect().await.expect("body").to_bytes();
    let value: serde_json::Value = serde_json::from_slice(&body).expect("json");
    assert!(value
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .contains("oops"));
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

#[tokio::test]
async fn send_input_unknown_session_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions/unknown/input")
                .header("content-type", "application/json")
                .body(Body::from("{\"input\":\"hi\"}"))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn stream_events_unknown_session_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sessions/unknown/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cancel_unknown_session_404() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/sessions/unknown/cancel")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn cancel_existing_session_no_content() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
    let session_id = create_session_id(&app).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sessions/{session_id}/cancel"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), axum::http::StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn send_input_accepts_and_writes_snapshot() {
    let dir = tempdir().expect("tmp");
    let data_dir = dir.path().join("data");
    let workspace_dir = dir.path().join("workspace");
    fs::create_dir_all(&workspace_dir).expect("workspace dir");
    let app = build_app_with_workspace_root(data_dir.clone(), workspace_dir);
    let session_id = create_session_id(&app).await;
    let response = app
        .clone()
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
    assert_eq!(response.status(), axum::http::StatusCode::ACCEPTED);

    let snapshot_path = data_dir
        .join("snapshots")
        .join(format!("{session_id}.json"));
    let log_path = data_dir.join("events.jsonl");
    timeout(Duration::from_secs(1), async {
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
}

#[tokio::test]
async fn stream_events_emits_payload() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
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
    let mut body = response.into_body();

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

    let frame = timeout(Duration::from_secs(1), body.frame())
        .await
        .expect("timeout")
        .expect("frame")
        .expect("frame");
    let payload = frame
        .into_data()
        .map(|data| String::from_utf8_lossy(&data).to_string())
        .unwrap_or_default();
    assert!(payload.contains("\"type\""));
}

#[tokio::test]
async fn stream_events_sse_compliance() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
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
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    assert!(content_type.starts_with("text/event-stream"));
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

    let message = reader.next_data_message().await.expect("message");
    let data_line = message
        .lines()
        .find(|line| line.starts_with("data:"))
        .expect("data line");
    let json = data_line.trim_start_matches("data:").trim();
    let value: serde_json::Value = serde_json::from_str(json).expect("json");
    assert!(value.get("type").is_some());

    for line in message.lines() {
        assert!(line.starts_with("data:") || line.starts_with("event:"));
    }
}

#[tokio::test]
async fn stream_events_preserves_order() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
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

    let first = reader.next_data_message().await.expect("first");
    let second = reader.next_data_message().await.expect("second");
    let first_value = extract_data_json(&first).expect("json");
    let second_value = extract_data_json(&second).expect("json");
    let first_seq = first_value
        .get("seq")
        .and_then(|value| value.as_u64())
        .expect("seq");
    let second_seq = second_value
        .get("seq")
        .and_then(|value| value.as_u64())
        .expect("seq");
    assert!(second_seq > first_seq);
}

#[tokio::test]
async fn tool_input_emits_checkpoint_events() {
    let dir = tempdir().expect("tmp");
    let app = build_test_app(&dir);
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

    let payload = serde_json::json!({
        "tool": "write",
        "args": {"path": "a.txt", "content": "hello"}
    })
    .to_string();

    let body = serde_json::json!({ "input": payload }).to_string();
    let send_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/sessions/{session_id}/input"))
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .expect("response");
    assert_eq!(send_response.status(), axum::http::StatusCode::ACCEPTED);

    let mut saw_checkpoint = false;
    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                if value.get("type").and_then(|value| value.as_str()) == Some("checkpoint_created")
                {
                    saw_checkpoint = true;
                    break;
                }
            }
        }
    })
    .await
    .expect("checkpoint timeout");

    assert!(saw_checkpoint, "expected checkpoint_created event");
}

struct TestSseReader {
    body: Body,
    buffer: String,
}

impl TestSseReader {
    fn new(body: Body) -> Self {
        Self {
            body,
            buffer: String::new(),
        }
    }

    async fn next_data_message(&mut self) -> Option<String> {
        loop {
            if let Some((message, remainder)) = split_sse_message(&self.buffer) {
                self.buffer = remainder;
                if message.lines().any(|line| line.starts_with("data:")) {
                    return Some(message);
                }
            }

            let frame = self.body.frame().await?.ok()?;
            let data = frame.into_data().ok()?;
            let text = String::from_utf8_lossy(&data);
            self.buffer.push_str(&text);
        }
    }
}

fn split_sse_message(buffer: &str) -> Option<(String, String)> {
    let marker = "\n\n";
    let idx = buffer.find(marker)?;
    let message = buffer[..idx].to_string();
    let remainder = buffer[idx + marker.len()..].to_string();
    Some((message, remainder))
}

fn extract_data_json(message: &str) -> Option<serde_json::Value> {
    let data_line = message.lines().find(|line| line.starts_with("data:"))?;
    let json = data_line.trim_start_matches("data:").trim();
    serde_json::from_str(json).ok()
}

#[tokio::test]
async fn prompt_uses_openresponses_provider_when_configured() {
    use axum::http::header::CONTENT_TYPE;
    use axum::routing::post;
    use axum::{response::IntoResponse, Router as AxumRouter};
    use tokio::net::TcpListener;

    let sse = include_str!("../../../fixtures/openresponses/stream_all.sse").to_string();
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
async fn prompt_openresponses_executes_function_tools_and_sends_followup() {
    run_openresponses_tool_loop_fixture(
        include_str!("../../../fixtures/openresponses/tool_loop_apply_patch_first.sse"),
        false,
    )
    .await;
}

#[tokio::test]
async fn prompt_openresponses_executes_function_tools_with_argument_deltas() {
    run_openresponses_tool_loop_fixture(
        include_str!("../../../fixtures/openresponses/tool_loop_apply_patch_args_delta.sse"),
        false,
    )
    .await;
}

#[tokio::test]
async fn prompt_openresponses_executes_function_tools_stateless_history() {
    run_openresponses_tool_loop_fixture(
        include_str!("../../../fixtures/openresponses/tool_loop_apply_patch_first.sse"),
        true,
    )
    .await;
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

async fn run_openresponses_tool_loop_fixture(first_sse: &'static str, stateless_history: bool) {
    use axum::extract::State;
    use axum::http::header::CONTENT_TYPE;
    use axum::routing::post;
    use axum::{response::IntoResponse, Json, Router as AxumRouter};
    use rip_log::{verify_snapshot, EventLog};
    use serde_json::Value;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;

    let second_sse = include_str!("../../../fixtures/openresponses/tool_loop_followup.sse");

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
        let parsed: Value = serde_json::from_str(output).expect("output json");
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
        let parsed: Value = serde_json::from_str(output).expect("output json");
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
    let log = EventLog::new(log_path).expect("event log");
    verify_snapshot(&log, snapshot_path).expect("verify snapshot");
}

#[tokio::test]
async fn prompt_openresponses_without_done_still_ends_session() {
    use axum::http::header::CONTENT_TYPE;
    use axum::routing::post;
    use axum::{response::IntoResponse, Router as AxumRouter};
    use tokio::net::TcpListener;

    let sse_full = include_str!("../../../fixtures/openresponses/stream_all.sse").to_string();
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

#[tokio::test]
async fn prompt_openresponses_http_error_emits_provider_error() {
    use axum::http::header::CONTENT_TYPE;
    use axum::http::StatusCode;
    use axum::routing::post;
    use axum::{response::IntoResponse, Router as AxumRouter};
    use tokio::net::TcpListener;

    let provider_app = AxumRouter::new().route(
        "/v1/responses",
        post(|| async move {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(CONTENT_TYPE, "text/plain")],
                "fail",
            )
                .into_response()
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

    let mut saw_provider_error = false;
    let mut saw_session_ended = false;

    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("provider_event") => {
                        if value.get("status").and_then(|value| value.as_str())
                            == Some("invalid_json")
                        {
                            saw_provider_error = true;
                        }
                    }
                    Some("session_ended") => {
                        saw_session_ended = true;
                        assert_eq!(
                            value.get("reason").and_then(|value| value.as_str()),
                            Some("provider_error")
                        );
                        break;
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("timeout");

    assert!(saw_provider_error, "expected provider_event invalid_json");
    assert!(saw_session_ended, "expected session_ended");
}

#[tokio::test]
async fn prompt_openresponses_connection_error_emits_provider_error() {
    let endpoint = "http://127.0.0.1:1/v1/responses".to_string();

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

    let mut saw_provider_error = false;
    let mut saw_session_ended = false;

    timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next_data_message().await {
            if let Some(value) = extract_data_json(&message) {
                match value.get("type").and_then(|value| value.as_str()) {
                    Some("provider_event") => {
                        if value.get("status").and_then(|value| value.as_str())
                            == Some("invalid_json")
                        {
                            saw_provider_error = true;
                        }
                    }
                    Some("session_ended") => {
                        saw_session_ended = true;
                        assert_eq!(
                            value.get("reason").and_then(|value| value.as_str()),
                            Some("provider_error")
                        );
                        break;
                    }
                    _ => {}
                }
            }
        }
    })
    .await
    .expect("timeout");

    assert!(saw_provider_error, "expected provider_event invalid_json");
    assert!(saw_session_ended, "expected session_ended");
}
