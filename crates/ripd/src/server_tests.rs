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
    workspace_root, SessionCreated, ThreadBranchResponse, ThreadCompactionCheckpointResponse,
    ThreadEnsureResponse, ThreadHandoffResponse, ThreadMeta, ThreadPostMessageResponse,
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
            headers: Vec::new(),
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

async fn compaction_checkpoint(
    app: &Router,
    thread_id: &str,
    payload: serde_json::Value,
) -> ThreadCompactionCheckpointResponse {
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
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&body).expect("json")
}

async fn compaction_cut_points(
    app: &Router,
    thread_id: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{thread_id}/compaction-cut-points"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
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
    serde_json::from_slice(&body).expect("json")
}

async fn compaction_status(
    app: &Router,
    thread_id: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{thread_id}/compaction-status"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
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
    serde_json::from_slice(&body).expect("json")
}

async fn compaction_auto(
    app: &Router,
    thread_id: &str,
    payload: serde_json::Value,
) -> (axum::http::StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{thread_id}/compaction-auto"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, serde_json::from_slice(&body).expect("json"))
}

async fn compaction_auto_schedule(
    app: &Router,
    thread_id: &str,
    payload: serde_json::Value,
) -> (axum::http::StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/threads/{thread_id}/compaction-auto-schedule"))
                .header("content-type", "application/json")
                .body(Body::from(payload.to_string()))
                .unwrap(),
        )
        .await
        .expect("response");
    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, serde_json::from_slice(&body).expect("json"))
}

async fn branch_thread(
    app: &Router,
    parent_thread_id: &str,
    payload: serde_json::Value,
) -> ThreadBranchResponse {
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
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    serde_json::from_slice(&body).expect("json")
}

async fn handoff_thread(
    app: &Router,
    from_thread_id: &str,
    payload: serde_json::Value,
) -> ThreadHandoffResponse {
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
    assert_eq!(response.status(), axum::http::StatusCode::CREATED);
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

    // Sanity: the first message isn't a cut point for stride=2, but is still present.
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

    // Verify the thread event stream contains no job/checkpoint frames.
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

    // Observe the emitted continuity frames via the thread event stream.
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

    // Verify the thread event stream contains no schedule/job/checkpoint frames.
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

    // Observe the emitted continuity frames via the thread event stream.
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
                        let has_errors = value
                            .get("errors")
                            .and_then(|value| value.as_array())
                            .map(|errors| !errors.is_empty())
                            .unwrap_or(false);
                        let has_response_errors = value
                            .get("response_errors")
                            .and_then(|value| value.as_array())
                            .map(|errors| !errors.is_empty())
                            .unwrap_or(false);
                        if has_errors || has_response_errors {
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

    assert!(saw_provider_error, "expected provider_event with errors");
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
                        let has_errors = value
                            .get("errors")
                            .and_then(|value| value.as_array())
                            .map(|errors| !errors.is_empty())
                            .unwrap_or(false);
                        let has_response_errors = value
                            .get("response_errors")
                            .and_then(|value| value.as_array())
                            .map(|errors| !errors.is_empty())
                            .unwrap_or(false);
                        if has_errors || has_response_errors {
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

    assert!(saw_provider_error, "expected provider_event with errors");
    assert!(saw_session_ended, "expected session_ended");
}
