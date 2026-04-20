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
    build_test_app_with_openresponses_provider_profile(dir, None, endpoint, stateless_history)
}

fn build_test_app_with_openresponses_provider_profile(
    dir: &tempfile::TempDir,
    provider_id: Option<&str>,
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
            provider_id: provider_id.map(str::to_string),
            endpoint,
            api_key: None,
            model: Some("fixture-model".to_string()),
            headers: Vec::new(),
            tool_choice: ToolChoiceParam::auto(),
            reasoning: None,
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

mod config_openapi;
mod openresponses_sessions;
mod openresponses_threads;
mod sessions;
mod tasks;
mod threads_basic;
mod threads_branching;
mod threads_compaction;
mod threads_compaction_auto;
