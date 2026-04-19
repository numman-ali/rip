use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::State,
    http::{header::CONTENT_TYPE, StatusCode},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use crate::provider_openresponses::OpenResponsesConfig;
use crate::runner::{SessionEngine, SessionHandle};
use crate::tasks::TaskHandle;
#[cfg(not(test))]
use crate::AuthorityLockGuard;

#[cfg(not(test))]
use reqwest::Client;
#[cfg(not(test))]
use std::net::SocketAddr;
#[cfg(not(test))]
use tokio::net::TcpListener;

use rip_provider_openresponses::ToolChoiceParam;

mod bootstrap;
mod routes_impl;

#[cfg(not(test))]
pub(crate) use bootstrap::serve;

#[derive(Clone)]
pub(crate) struct AppState {
    sessions: Arc<Mutex<HashMap<String, SessionHandle>>>,
    tasks: Arc<Mutex<HashMap<String, TaskHandle>>>,
    engine: Arc<SessionEngine>,
    openapi_json: Arc<String>,
    allow_pty_tasks: bool,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub(crate) struct SessionCreated {
    pub(crate) session_id: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub(crate) struct ThreadEnsureResponse {
    pub(crate) thread_id: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub(crate) struct ThreadMeta {
    pub(crate) thread_id: String,
    pub(crate) created_at_ms: u64,
    pub(crate) title: Option<String>,
    pub(crate) archived: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ThreadPostMessagePayload {
    pub(crate) content: String,
    pub(crate) actor_id: Option<String>,
    pub(crate) origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) openresponses: Option<ThreadOpenResponsesOverrides>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub(crate) struct ThreadOpenResponsesOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) stateless_history: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parallel_tool_calls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) followup_user_message: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub(crate) struct ThreadPostMessageResponse {
    pub(crate) thread_id: String,
    pub(crate) message_id: String,
    pub(crate) session_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ThreadBranchPayload {
    pub(crate) title: Option<String>,
    pub(crate) from_message_id: Option<String>,
    pub(crate) from_seq: Option<u64>,
    pub(crate) actor_id: Option<String>,
    pub(crate) origin: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub(crate) struct ThreadBranchResponse {
    pub(crate) thread_id: String,
    pub(crate) parent_thread_id: String,
    pub(crate) parent_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parent_message_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ThreadHandoffPayload {
    pub(crate) title: Option<String>,
    pub(crate) summary_markdown: Option<String>,
    pub(crate) summary_artifact_id: Option<String>,
    pub(crate) from_message_id: Option<String>,
    pub(crate) from_seq: Option<u64>,
    pub(crate) actor_id: Option<String>,
    pub(crate) origin: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub(crate) struct ThreadHandoffResponse {
    pub(crate) thread_id: String,
    pub(crate) from_thread_id: String,
    pub(crate) from_seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) from_message_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct ThreadCompactionCheckpointPayload {
    pub(crate) summary_markdown: Option<String>,
    pub(crate) summary_artifact_id: Option<String>,
    pub(crate) to_message_id: Option<String>,
    pub(crate) to_seq: Option<u64>,
    pub(crate) stride_messages: Option<u64>,
    pub(crate) actor_id: Option<String>,
    pub(crate) origin: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub(crate) struct ThreadCompactionCheckpointResponse {
    pub(crate) thread_id: String,
    pub(crate) checkpoint_id: String,
    pub(crate) cut_rule_id: String,
    pub(crate) summary_artifact_id: String,
    pub(crate) to_seq: u64,
    pub(crate) to_message_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct InputPayload {
    pub(crate) input: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ConfigDoctorResponse {
    pub(crate) sources: Vec<ConfigDoctorSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) openresponses: Option<ConfigDoctorOpenResponses>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ConfigDoctorSource {
    pub(crate) path: String,
    pub(crate) status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ConfigDoctorOpenResponses {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) effective_route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) route_source: Option<String>,
    pub(crate) endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) endpoint_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) model_source: Option<String>,
    pub(crate) has_api_key: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) api_key_source: Option<String>,
    pub(crate) headers: Vec<String>,
    pub(crate) stateless_history: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) stateless_history_source: Option<String>,
    pub(crate) parallel_tool_calls: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parallel_tool_calls_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) followup_user_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) followup_user_message_source: Option<String>,
}

#[derive(OpenApi)]
#[openapi(info(
    title = "RIP Agent Server",
    description = "Agent session control plane (HTTP/SSE).",
    version = "0.1.0"
))]
struct ApiDoc;

#[cfg(not(test))]
#[allow(dead_code)]
pub(crate) fn build_app(data_dir: std::path::PathBuf) -> Router {
    build_app_with_workspace_root_and_provider(
        data_dir,
        workspace_root(),
        OpenResponsesConfig::from_env(),
    )
}

#[cfg(test)]
pub(crate) fn build_app_with_workspace_root(
    data_dir: std::path::PathBuf,
    workspace_root: std::path::PathBuf,
) -> Router {
    build_app_with_workspace_root_and_provider(data_dir, workspace_root, None)
}

pub(crate) fn build_app_with_workspace_root_and_provider(
    data_dir: std::path::PathBuf,
    workspace_root: std::path::PathBuf,
    openresponses: Option<OpenResponsesConfig>,
) -> Router {
    build_app_with_workspace_root_and_provider_and_task_policy(
        data_dir,
        workspace_root,
        openresponses,
        allow_pty_tasks_from_env(),
    )
}

pub(crate) fn build_app_with_workspace_root_and_provider_and_task_policy(
    data_dir: std::path::PathBuf,
    workspace_root: std::path::PathBuf,
    openresponses: Option<OpenResponsesConfig>,
    allow_pty_tasks: bool,
) -> Router {
    let (router, openapi_json) = build_openapi_router();

    let engine = Arc::new(
        SessionEngine::new(data_dir, workspace_root, openresponses).expect("session engine"),
    );

    let state = AppState {
        sessions: Arc::new(Mutex::new(HashMap::new())),
        tasks: Arc::new(Mutex::new(HashMap::new())),
        engine,
        openapi_json: Arc::new(openapi_json),
        allow_pty_tasks,
    };

    router
        .route("/openapi.json", get(openapi_spec))
        .with_state(state)
}

pub(crate) fn build_openapi_router() -> (Router<AppState>, String) {
    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(routes_impl::config::config_doctor))
        .routes(routes!(routes_impl::sessions::create_session))
        .routes(routes!(routes_impl::sessions::send_input))
        .routes(routes!(routes_impl::sessions::stream_events))
        .routes(routes!(routes_impl::sessions::cancel_session))
        .routes(routes!(routes_impl::threads::thread_ensure))
        .routes(routes!(routes_impl::threads::thread_list))
        .routes(routes!(routes_impl::threads::thread_get))
        .routes(routes!(routes_impl::threads::thread_post_message))
        .routes(routes!(routes_impl::threads::thread_branch))
        .routes(routes!(routes_impl::threads::thread_handoff))
        .routes(routes!(
            routes_impl::threads_compaction::thread_compaction_checkpoint
        ))
        .routes(routes!(
            routes_impl::threads_compaction::thread_compaction_cut_points
        ))
        .routes(routes!(
            routes_impl::threads_compaction::thread_compaction_status
        ))
        .routes(routes!(routes_impl::threads::thread_provider_cursor_status))
        .routes(routes!(routes_impl::threads::thread_provider_cursor_rotate))
        .routes(routes!(
            routes_impl::threads::thread_context_selection_status
        ))
        .routes(routes!(
            routes_impl::threads_compaction::thread_compaction_auto
        ))
        .routes(routes!(
            routes_impl::threads_compaction::thread_compaction_auto_schedule
        ))
        .routes(routes!(routes_impl::threads::thread_stream_events))
        .routes(routes!(routes_impl::tasks::create_task))
        .routes(routes!(routes_impl::tasks::list_tasks))
        .routes(routes!(routes_impl::tasks::task_status))
        .routes(routes!(routes_impl::tasks::task_output))
        .routes(routes!(routes_impl::tasks::stream_task_events))
        .routes(routes!(routes_impl::tasks::cancel_task))
        .routes(routes!(routes_impl::tasks::task_write_stdin))
        .routes(routes!(routes_impl::tasks::task_resize))
        .routes(routes!(routes_impl::tasks::task_signal))
        .split_for_parts();
    let json = api
        .to_pretty_json()
        .map(|value| format!("{value}\n"))
        .expect("openapi json");
    (router, json)
}

async fn openapi_spec(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    (
        StatusCode::OK,
        [(CONTENT_TYPE, "application/json")],
        state.openapi_json.as_str().to_owned(),
    )
}

#[cfg(not(test))]
pub(crate) fn data_dir() -> std::path::PathBuf {
    if let Ok(value) = std::env::var("RIP_DATA_DIR") {
        return std::path::PathBuf::from(value);
    }
    std::path::PathBuf::from("data")
}

#[cfg_attr(test, inline(never))]
pub(crate) fn workspace_root() -> std::path::PathBuf {
    if let Ok(value) = std::env::var("RIP_WORKSPACE_ROOT") {
        return std::path::PathBuf::from(value);
    }
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

fn allow_pty_tasks_from_env() -> bool {
    let Ok(value) = std::env::var("RIP_TASKS_ALLOW_PTY") else {
        return false;
    };
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(not(test))]
fn server_addr_from_env() -> Option<SocketAddr> {
    let raw = std::env::var("RIP_SERVER_ADDR").ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.parse::<SocketAddr>() {
        Ok(addr) => Some(addr),
        Err(err) => {
            eprintln!("invalid RIP_SERVER_ADDR={raw:?}: {err}; using default");
            None
        }
    }
}
