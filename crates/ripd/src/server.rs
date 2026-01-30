use std::{collections::HashMap, convert::Infallible, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::{header::CONTENT_TYPE, StatusCode},
    response::{sse::Event as SseEvent, IntoResponse, Sse},
    routing::get,
    Json, Router,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_stream::wrappers::BroadcastStream;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use rip_provider_openresponses::ToolChoiceParam;

use crate::provider_openresponses::OpenResponsesConfig;
use crate::runner::{SessionEngine, SessionHandle};
use crate::tasks::{
    TaskCancelPayload, TaskCreated, TaskEngine, TaskHandle, TaskOutputQuery, TaskOutputResponse,
    TaskResizePayload, TaskSignalPayload, TaskSpawnPayload, TaskStatusResponse,
    TaskWriteStdinPayload,
};
#[cfg(not(test))]
use crate::AuthorityLockGuard;

#[cfg(not(test))]
use reqwest::Client;
#[cfg(not(test))]
use std::net::SocketAddr;
#[cfg(not(test))]
use tokio::net::TcpListener;

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
struct InputPayload {
    input: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct ConfigDoctorResponse {
    sources: Vec<ConfigDoctorSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    openresponses: Option<ConfigDoctorOpenResponses>,
}

#[derive(Debug, Serialize, ToSchema)]
struct ConfigDoctorSource {
    path: String,
    status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
struct ConfigDoctorOpenResponses {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    route: Option<String>,
    endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    has_api_key: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_key_source: Option<String>,
    headers: Vec<String>,
    stateless_history: bool,
    parallel_tool_calls: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    followup_user_message: Option<String>,
}

#[derive(OpenApi)]
#[openapi(info(
    title = "RIP Agent Server",
    description = "Agent session control plane (HTTP/SSE).",
    version = "0.1.0"
))]
struct ApiDoc;

#[cfg(not(test))]
pub(crate) async fn serve(data_dir: std::path::PathBuf) {
    let workspace_root = workspace_root();
    let addr = server_addr_from_env().unwrap_or_else(|| "127.0.0.1:7341".parse().expect("addr"));

    let client = Client::builder()
        .timeout(std::time::Duration::from_millis(250))
        .build()
        .expect("reqwest client");

    let lock = acquire_authority_lock_with_recovery(&client, &data_dir, &workspace_root)
        .await
        .unwrap_or_else(|err| panic!("{err}"));

    let app = build_app_with_workspace_root_and_provider(
        data_dir.clone(),
        workspace_root.clone(),
        OpenResponsesConfig::from_env(),
    );

    let listener = TcpListener::bind(addr).await.expect("bind");
    let local_addr = listener.local_addr().expect("local addr");
    let endpoint = format!("http://{local_addr}");
    eprintln!("ripd listening on {endpoint}");

    lock.write_meta(endpoint)
        .unwrap_or_else(|err| panic!("{err}"));

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        let _ = shutdown_rx.await;
    });

    let mut server_task = tokio::spawn(async move { server.await });
    tokio::select! {
        result = &mut server_task => {
            let result = result.expect("server task");
            result.expect("server");
            return;
        }
        _ = shutdown_signal() => {
            let _ = shutdown_tx.send(());
        }
    }

    match tokio::time::timeout(std::time::Duration::from_secs(2), &mut server_task).await {
        Ok(result) => {
            let result = result.expect("server task");
            result.expect("server");
        }
        Err(_) => {
            eprintln!("server shutdown timed out; forcing exit");
            server_task.abort();
            let _ = server_task.await;
        }
    }
}

#[cfg(not(test))]
async fn ping_openapi(client: &Client, endpoint: &str) -> bool {
    let url = format!("{endpoint}/openapi.json");
    match client.get(url).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

#[cfg(not(test))]
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        let _ = sigterm.recv().await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

#[cfg(not(test))]
async fn acquire_authority_lock_with_recovery(
    client: &Client,
    data_dir: &std::path::Path,
    workspace_root: &std::path::Path,
) -> Result<AuthorityLockGuard, String> {
    let workspace_root_str = workspace_root.to_string_lossy().to_string();

    let mut lock_invalid_since: Option<std::time::Instant> = None;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);

    loop {
        match AuthorityLockGuard::try_acquire(data_dir, workspace_root) {
            Ok(lock) => return Ok(lock),
            Err(err) => {
                let meta = crate::read_authority_meta(data_dir).unwrap_or(None);
                let endpoint_reachable = match &meta {
                    Some(meta) => ping_openapi(client, &meta.endpoint).await,
                    None => false,
                };
                if endpoint_reachable {
                    let Some(meta) = &meta else {
                        return Err(err);
                    };
                    return Err(format!(
                        "store already has an authority (endpoint={} pid={})",
                        meta.endpoint, meta.pid
                    ));
                }

                match crate::read_authority_lock_record(data_dir) {
                    Ok(Some(lock)) => {
                        lock_invalid_since = None;
                        if lock.workspace_root != workspace_root_str {
                            return Err(format!(
                                "store authority workspace mismatch: authority_root={} current_root={}",
                                lock.workspace_root,
                                workspace_root.display()
                            ));
                        }

                        let pid_liveness = crate::pid_liveness(lock.pid);
                        if matches!(pid_liveness, crate::PidLiveness::Dead) && !endpoint_reachable {
                            let cleaned = crate::try_cleanup_stale_authority_files(
                                data_dir,
                                lock.pid,
                                lock.started_at_ms,
                            )?;
                            if cleaned {
                                continue;
                            }
                        }

                        return Err(err);
                    }
                    Ok(None) => {
                        if std::time::Instant::now() >= deadline {
                            return Err(err);
                        }
                    }
                    Err(lock_err) => {
                        let lock_path = crate::authority_lock_path(data_dir);
                        lock_invalid_since.get_or_insert(std::time::Instant::now());

                        if lock_err.contains("lock json invalid")
                            && lock_invalid_since
                                .map(|since| since.elapsed() > std::time::Duration::from_secs(1))
                                .unwrap_or(false)
                        {
                            let cleaned = crate::try_cleanup_corrupt_lock_file(data_dir)?;
                            if cleaned {
                                lock_invalid_since = None;
                                continue;
                            }
                        }

                        if std::time::Instant::now() >= deadline {
                            return Err(format!(
                                "{err} (lock_path={} read_err={lock_err})",
                                lock_path.display()
                            ));
                        }
                    }
                }

                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        }
    }
}

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
        .routes(routes!(config_doctor))
        .routes(routes!(create_session))
        .routes(routes!(send_input))
        .routes(routes!(stream_events))
        .routes(routes!(cancel_session))
        .routes(routes!(thread_ensure))
        .routes(routes!(thread_list))
        .routes(routes!(thread_get))
        .routes(routes!(thread_post_message))
        .routes(routes!(thread_branch))
        .routes(routes!(thread_handoff))
        .routes(routes!(thread_compaction_checkpoint))
        .routes(routes!(thread_compaction_cut_points))
        .routes(routes!(thread_compaction_status))
        .routes(routes!(thread_provider_cursor_status))
        .routes(routes!(thread_provider_cursor_rotate))
        .routes(routes!(thread_context_selection_status))
        .routes(routes!(thread_compaction_auto))
        .routes(routes!(thread_compaction_auto_schedule))
        .routes(routes!(thread_stream_events))
        .routes(routes!(create_task))
        .routes(routes!(list_tasks))
        .routes(routes!(task_status))
        .routes(routes!(task_output))
        .routes(routes!(stream_task_events))
        .routes(routes!(cancel_task))
        .routes(routes!(task_write_stdin))
        .routes(routes!(task_resize))
        .routes(routes!(task_signal))
        .split_for_parts();
    let json = api
        .to_pretty_json()
        .map(|value| format!("{value}\n"))
        .expect("openapi json");
    (router, json)
}

#[utoipa::path(
    post,
    path = "/sessions",
    responses(
        (status = 201, description = "Session created", body = SessionCreated)
    )
)]
async fn create_session(State(state): State<AppState>) -> impl IntoResponse {
    let handle = state.engine.create_session();
    let session_id = handle.session_id.clone();

    let mut sessions = state.sessions.lock().await;
    sessions.insert(session_id.clone(), handle);

    (StatusCode::CREATED, Json(SessionCreated { session_id }))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/input",
    params(
        ("id" = String, Path, description = "Session id")
    ),
    request_body = InputPayload,
    responses(
        (status = 202, description = "Input accepted"),
        (status = 404, description = "Session not found")
    )
)]
async fn send_input(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<InputPayload>,
) -> impl IntoResponse {
    let handle = {
        let sessions = state.sessions.lock().await;
        match sessions.get(&session_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    state
        .engine
        .spawn_session(handle, payload.input, None, None);

    StatusCode::ACCEPTED.into_response()
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/events",
    params(
        ("id" = String, Path, description = "Session id")
    ),
    responses(
        (status = 200, description = "SSE stream of event frames"),
        (status = 404, description = "Session not found")
    )
)]
async fn stream_events(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let handle = {
        let sessions = state.sessions.lock().await;
        match sessions.get(&session_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let receiver = handle.subscribe();
    let past = handle.events_snapshot().await;

    let last_seq = past.last().map(|event| event.seq);
    let past_stream = tokio_stream::iter(past).filter_map(|event| async move {
        let json = serde_json::to_string(&event).ok()?;
        Some(Ok::<SseEvent, Infallible>(SseEvent::default().data(json)))
    });

    let last_seq_live = last_seq;
    let live_stream = BroadcastStream::new(receiver).filter_map(move |result| {
        let last_seq = last_seq_live;
        async move {
            match result {
                Ok(event) => {
                    if last_seq.map(|last| event.seq <= last).unwrap_or(false) {
                        return None;
                    }
                    let json = serde_json::to_string(&event).ok()?;
                    Some(Ok::<SseEvent, Infallible>(SseEvent::default().data(json)))
                }
                Err(_) => None,
            }
        }
    });

    let stream = past_stream.chain(live_stream);

    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().text("ping"))
        .into_response()
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/cancel",
    params(
        ("id" = String, Path, description = "Session id")
    ),
    responses(
        (status = 204, description = "Session canceled"),
        (status = 404, description = "Session not found")
    )
)]
async fn cancel_session(
    Path(session_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let mut sessions = state.sessions.lock().await;
    if sessions.remove(&session_id).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

#[utoipa::path(
    post,
    path = "/threads/ensure",
    responses(
        (status = 200, description = "Default thread ensured", body = ThreadEnsureResponse)
    )
)]
async fn thread_ensure(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.engine.continuities();
    match store.ensure_default() {
        Ok(thread_id) => Json(ThreadEnsureResponse { thread_id }).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/threads",
    responses(
        (status = 200, description = "List threads", body = [ThreadMeta])
    )
)]
async fn thread_list(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.engine.continuities();
    let mut out = Vec::new();
    for meta in store.list() {
        out.push(ThreadMeta {
            thread_id: meta.continuity_id,
            created_at_ms: meta.created_at_ms,
            title: meta.title,
            archived: meta.archived,
        });
    }
    Json(out).into_response()
}

#[utoipa::path(
    get,
    path = "/threads/{id}",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    responses(
        (status = 200, description = "Thread metadata", body = ThreadMeta),
        (status = 404, description = "Thread not found")
    )
)]
async fn thread_get(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let store = state.engine.continuities();
    match store.get(&thread_id) {
        Some(meta) => Json(ThreadMeta {
            thread_id,
            created_at_ms: meta.created_at_ms,
            title: meta.title,
            archived: meta.archived,
        })
        .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/messages",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = ThreadPostMessagePayload,
    responses(
        (status = 202, description = "Message accepted and run started", body = ThreadPostMessageResponse),
        (status = 404, description = "Thread not found")
    )
)]
async fn thread_post_message(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<ThreadPostMessagePayload>,
) -> impl IntoResponse {
    let ThreadPostMessagePayload {
        content,
        actor_id,
        origin,
        openresponses,
    } = payload;

    let actor_id = actor_id.unwrap_or_else(|| "user".to_string());
    let origin = origin.unwrap_or_else(|| "server".to_string());

    let store = state.engine.continuities();
    let (resolved_openresponses, _loaded) = crate::config::resolve_openresponses_config(
        store.workspace_root(),
        crate::config::OpenResponsesOverrideInput {
            endpoint: openresponses.as_ref().and_then(|cfg| cfg.endpoint.clone()),
            model: openresponses.as_ref().and_then(|cfg| cfg.model.clone()),
            stateless_history: openresponses.as_ref().and_then(|cfg| cfg.stateless_history),
            parallel_tool_calls: openresponses
                .as_ref()
                .and_then(|cfg| cfg.parallel_tool_calls),
            followup_user_message: openresponses
                .as_ref()
                .and_then(|cfg| cfg.followup_user_message.clone()),
        },
    );
    let openresponses_override = resolved_openresponses.map(|cfg| OpenResponsesConfig {
        endpoint: cfg.endpoint,
        api_key: cfg.api_key,
        model: cfg.model,
        headers: cfg.headers,
        tool_choice: ToolChoiceParam::auto(),
        followup_user_message: cfg.followup_user_message,
        stateless_history: cfg.stateless_history,
        parallel_tool_calls: cfg.parallel_tool_calls,
    });
    let message_id = match store.append_message(
        &thread_id,
        actor_id.clone(),
        origin.clone(),
        content.clone(),
    ) {
        Ok(id) => id,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let handle = state.engine.create_session();
    let session_id = handle.session_id.clone();
    {
        let mut sessions = state.sessions.lock().await;
        sessions.insert(session_id.clone(), handle.clone());
    }

    let run_link = crate::continuities::ContinuityRunLink {
        continuity_id: thread_id.clone(),
        message_id: message_id.clone(),
        actor_id: actor_id.clone(),
        origin: origin.clone(),
    };
    if store
        .append_run_spawned(&thread_id, &message_id, &session_id, actor_id, origin)
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    state
        .engine
        .spawn_session(handle, content, Some(run_link), openresponses_override);

    (
        StatusCode::ACCEPTED,
        Json(ThreadPostMessageResponse {
            thread_id,
            message_id,
            session_id,
        }),
    )
        .into_response()
}

#[utoipa::path(
    post,
    path = "/threads/{id}/branch",
    params(
        ("id" = String, Path, description = "Parent thread id")
    ),
    request_body = ThreadBranchPayload,
    responses(
        (status = 201, description = "Branch created", body = ThreadBranchResponse),
        (status = 400, description = "Invalid branch request"),
        (status = 404, description = "Thread or branch point not found")
    )
)]
async fn thread_branch(
    Path(parent_thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<ThreadBranchPayload>,
) -> impl IntoResponse {
    let actor_id = payload.actor_id.unwrap_or_else(|| "user".to_string());
    let origin = payload.origin.unwrap_or_else(|| "server".to_string());

    let store = state.engine.continuities();
    match store.branch(
        &parent_thread_id,
        payload.title,
        payload.from_message_id,
        payload.from_seq,
        actor_id,
        origin,
    ) {
        Ok((thread_id, parent_seq, parent_message_id)) => (
            StatusCode::CREATED,
            Json(ThreadBranchResponse {
                thread_id,
                parent_thread_id,
                parent_seq,
                parent_message_id,
            }),
        )
            .into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("out of range") || err_lower.contains("requires only one of") {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("does not exist") || err_lower.contains("not found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/handoff",
    params(
        ("id" = String, Path, description = "Source thread id")
    ),
    request_body = ThreadHandoffPayload,
    responses(
        (status = 201, description = "Handoff thread created", body = ThreadHandoffResponse),
        (status = 400, description = "Invalid handoff request"),
        (status = 404, description = "Thread or handoff point not found")
    )
)]
async fn thread_handoff(
    Path(from_thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<ThreadHandoffPayload>,
) -> impl IntoResponse {
    let actor_id = payload.actor_id.unwrap_or_else(|| "user".to_string());
    let origin = payload.origin.unwrap_or_else(|| "server".to_string());

    let store = state.engine.continuities();
    match store.handoff(
        &from_thread_id,
        payload.title,
        (payload.summary_markdown, payload.summary_artifact_id),
        payload.from_message_id,
        payload.from_seq,
        (actor_id, origin),
    ) {
        Ok((thread_id, from_seq, from_message_id)) => (
            StatusCode::CREATED,
            Json(ThreadHandoffResponse {
                thread_id,
                from_thread_id,
                from_seq,
                from_message_id,
            }),
        )
            .into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("out of range")
                || err_lower.contains("requires only one of")
                || err_lower.contains("requires summary")
            {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("does not exist") || err_lower.contains("not found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/compaction-checkpoint",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = ThreadCompactionCheckpointPayload,
    responses(
        (status = 201, description = "Compaction checkpoint created", body = ThreadCompactionCheckpointResponse),
        (status = 400, description = "Invalid checkpoint request"),
        (status = 404, description = "Thread or cut point not found")
    )
)]
async fn thread_compaction_checkpoint(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<ThreadCompactionCheckpointPayload>,
) -> impl IntoResponse {
    let actor_id = payload.actor_id.unwrap_or_else(|| "user".to_string());
    let origin = payload.origin.unwrap_or_else(|| "server".to_string());

    let store = state.engine.continuities();
    match store.compaction_checkpoint_cumulative_v1(
        &thread_id,
        crate::CompactionCheckpointCumulativeV1Request {
            summary_markdown: payload.summary_markdown,
            summary_artifact_id: payload.summary_artifact_id,
            to_message_id: payload.to_message_id,
            to_seq: payload.to_seq,
            stride_messages: payload.stride_messages,
            actor_id,
            origin,
        },
    ) {
        Ok((checkpoint_id, summary_artifact_id, to_seq, to_message_id, cut_rule_id)) => (
            StatusCode::CREATED,
            Json(ThreadCompactionCheckpointResponse {
                thread_id,
                checkpoint_id,
                cut_rule_id,
                summary_artifact_id,
                to_seq,
                to_message_id,
            }),
        )
            .into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("out of range")
                || err_lower.contains("requires")
                || err_lower.contains("must")
                || err_lower.contains("mismatch")
                || err_lower.contains("stride")
            {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("does not exist") || err_lower.contains("not found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/compaction-cut-points",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::CompactionCutPointsV1Request,
    responses(
        (status = 200, description = "Computed cut points", body = crate::CompactionCutPointsV1Response),
        (status = 400, description = "Invalid cut point request"),
        (status = 404, description = "Thread not found")
    )
)]
async fn thread_compaction_cut_points(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<crate::CompactionCutPointsV1Request>,
) -> impl IntoResponse {
    let store = state.engine.continuities();
    match store.compaction_cut_points_v1(&thread_id, payload) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("invalid_stride") {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/compaction-status",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::CompactionStatusV1Request,
    responses(
        (status = 200, description = "Compaction status projection", body = crate::CompactionStatusV1Response),
        (status = 400, description = "Invalid status request"),
        (status = 404, description = "Thread not found")
    )
)]
async fn thread_compaction_status(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<crate::CompactionStatusV1Request>,
) -> impl IntoResponse {
    let store = state.engine.continuities();
    match store.compaction_status_v1(&thread_id, payload) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("invalid_stride") {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/provider-cursor-status",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::ProviderCursorStatusV1Request,
    responses(
        (status = 200, description = "Provider cursor status projection", body = crate::ProviderCursorStatusV1Response),
        (status = 404, description = "Thread not found")
    )
)]
async fn thread_provider_cursor_status(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<crate::ProviderCursorStatusV1Request>,
) -> impl IntoResponse {
    let store = state.engine.continuities();
    match store.provider_cursor_status_v1(&thread_id, payload) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/context-selection-status",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::ContextSelectionStatusV1Request,
    responses(
        (status = 200, description = "Context selection strategy decisions (truth-derived)", body = crate::ContextSelectionStatusV1Response),
        (status = 404, description = "Thread not found")
    )
)]
async fn thread_context_selection_status(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<crate::ContextSelectionStatusV1Request>,
) -> impl IntoResponse {
    let store = state.engine.continuities();
    match store.context_selection_status_v1(&thread_id, payload) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/provider-cursor-rotate",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::ProviderCursorRotateV1Request,
    responses(
        (status = 200, description = "Provider cursor rotation logged", body = crate::ProviderCursorRotateV1Response),
        (status = 404, description = "Thread not found")
    )
)]
async fn thread_provider_cursor_rotate(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(mut payload): Json<crate::ProviderCursorRotateV1Request>,
) -> impl IntoResponse {
    if payload.actor_id.trim().is_empty() {
        payload.actor_id = "user".to_string();
    }
    if payload.origin.trim().is_empty() {
        payload.origin = "server".to_string();
    }

    let store = state.engine.continuities();
    match store.provider_cursor_rotate_v1(&thread_id, payload) {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[utoipa::path(
    post,
    path = "/threads/{id}/compaction-auto",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::CompactionAutoV1Request,
    responses(
        (status = 200, description = "Auto-compaction no-op / dry-run result", body = crate::CompactionAutoV1Response),
        (status = 202, description = "Auto-compaction job spawned", body = crate::CompactionAutoV1Response),
        (status = 400, description = "Invalid auto-compaction request"),
        (status = 404, description = "Thread not found")
    )
)]
async fn thread_compaction_auto(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(mut payload): Json<crate::CompactionAutoV1Request>,
) -> impl IntoResponse {
    if payload.actor_id.trim().is_empty() {
        payload.actor_id = "user".to_string();
    }
    if payload.origin.trim().is_empty() {
        payload.origin = "server".to_string();
    }

    let store = state.engine.continuities();
    let actor_id = payload.actor_id.clone();
    let origin = payload.origin.clone();

    let response = match store.compaction_auto_spawn_job_v1(&thread_id, payload) {
        Ok(response) => response,
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("invalid_stride") {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if response.status == "spawned" {
        let Some(job_id) = response.job_id.clone() else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        let planned = response.planned.clone();
        let stride_messages = response.stride_messages;
        let cut_rule_id = response.cut_rule_id.clone();
        let store = store.clone();
        let thread_id = thread_id.clone();
        tokio::spawn(async move {
            let _ = tokio::task::spawn_blocking(move || {
                let _ = store.compaction_auto_run_spawned_job_v1(
                    &thread_id,
                    &job_id,
                    stride_messages,
                    &cut_rule_id,
                    &planned,
                    (actor_id.as_str(), origin.as_str()),
                );
            })
            .await;
        });

        return (StatusCode::ACCEPTED, Json(response)).into_response();
    }

    (StatusCode::OK, Json(response)).into_response()
}

#[utoipa::path(
    post,
    path = "/threads/{id}/compaction-auto-schedule",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    request_body = crate::CompactionAutoScheduleV1Request,
    responses(
        (status = 200, description = "Scheduler no-op / dry-run / skipped result", body = crate::CompactionAutoScheduleV1Response),
        (status = 202, description = "Scheduler started a job", body = crate::CompactionAutoScheduleV1Response),
        (status = 400, description = "Invalid schedule request"),
        (status = 404, description = "Thread not found")
    )
)]
async fn thread_compaction_auto_schedule(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
    Json(mut payload): Json<crate::CompactionAutoScheduleV1Request>,
) -> impl IntoResponse {
    if payload.actor_id.trim().is_empty() {
        payload.actor_id = "user".to_string();
    }
    if payload.origin.trim().is_empty() {
        payload.origin = "server".to_string();
    }

    let store = state.engine.continuities();
    let actor_id = payload.actor_id.clone();
    let origin = payload.origin.clone();

    let response = match store.compaction_auto_schedule_spawn_job_v1(&thread_id, payload) {
        Ok(response) => response,
        Err(err) => {
            let err_lower = err.to_ascii_lowercase();
            if err_lower.contains("invalid_stride") {
                return StatusCode::BAD_REQUEST.into_response();
            }
            if err_lower.contains("not_found") {
                return StatusCode::NOT_FOUND.into_response();
            }
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    if response.decision == "scheduled" {
        if response.execute {
            let Some(job_id) = response.job_id.clone() else {
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            };
            let planned = response.planned.clone();
            let stride_messages = response.stride_messages;
            let cut_rule_id = response.cut_rule_id.clone();
            let store = store.clone();
            let thread_id = thread_id.clone();
            tokio::spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = store.compaction_auto_run_spawned_job_v1(
                        &thread_id,
                        &job_id,
                        stride_messages,
                        &cut_rule_id,
                        &planned,
                        (actor_id.as_str(), origin.as_str()),
                    );
                })
                .await;
            });
        }

        return (StatusCode::ACCEPTED, Json(response)).into_response();
    }

    (StatusCode::OK, Json(response)).into_response()
}

#[utoipa::path(
    get,
    path = "/threads/{id}/events",
    params(
        ("id" = String, Path, description = "Thread id")
    ),
    responses(
        (status = 200, description = "SSE stream of thread event frames"),
        (status = 404, description = "Thread not found")
    )
)]
async fn thread_stream_events(
    Path(thread_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let store = state.engine.continuities();
    let receiver = store.subscribe();

    let past = match store.replay_events(&thread_id) {
        Ok(events) => events,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    // If there are no frames in the stream, treat the thread id as unknown.
    // (The truth of thread existence is its continuity event stream.)
    if past.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let last_seq = past.last().map(|event| event.seq);
    let past_stream = tokio_stream::iter(past).filter_map(|event| async move {
        let json = serde_json::to_string(&event).ok()?;
        Some(Ok::<SseEvent, Infallible>(SseEvent::default().data(json)))
    });

    let thread_id_live = thread_id.clone();
    let last_seq_live = last_seq;
    let live_stream = BroadcastStream::new(receiver).filter_map(move |result| {
        let last_seq = last_seq_live;
        let thread_id = thread_id_live.clone();
        async move {
            match result {
                Ok(event) => {
                    if event.session_id != thread_id {
                        return None;
                    }
                    if last_seq.map(|last| event.seq <= last).unwrap_or(false) {
                        return None;
                    }
                    let json = serde_json::to_string(&event).ok()?;
                    Some(Ok::<SseEvent, Infallible>(SseEvent::default().data(json)))
                }
                Err(_) => None,
            }
        }
    });

    let stream = past_stream.chain(live_stream);
    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().text("ping"))
        .into_response()
}

#[utoipa::path(
    post,
    path = "/tasks",
    request_body = TaskSpawnPayload,
    responses(
        (status = 201, description = "Task created", body = TaskCreated),
        (status = 400, description = "Invalid task request")
    )
)]
async fn create_task(
    State(state): State<AppState>,
    Json(payload): Json<TaskSpawnPayload>,
) -> impl IntoResponse {
    let mode = payload
        .execution_mode
        .unwrap_or(crate::tasks::ApiToolTaskExecutionMode::Pipes);
    if mode == crate::tasks::ApiToolTaskExecutionMode::Pty && !state.allow_pty_tasks {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if payload.tool != "bash" && payload.tool != "shell" {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let engine: Arc<TaskEngine> = state.engine.tasks();
    let handle = engine.create_task(&payload);
    let task_id = handle.task_id.clone();
    {
        let mut tasks = state.tasks.lock().await;
        tasks.insert(task_id.clone(), handle.clone());
    }

    engine.spawn_task(handle, payload);

    (StatusCode::CREATED, Json(TaskCreated { task_id })).into_response()
}

#[utoipa::path(
    get,
    path = "/tasks",
    responses(
        (status = 200, description = "List tasks", body = [TaskStatusResponse])
    )
)]
async fn list_tasks(State(state): State<AppState>) -> impl IntoResponse {
    let handles = {
        let tasks = state.tasks.lock().await;
        tasks.values().cloned().collect::<Vec<_>>()
    };
    let mut out = Vec::with_capacity(handles.len());
    for handle in handles {
        out.push(handle.status().await);
    }
    Json(out).into_response()
}

#[utoipa::path(
    get,
    path = "/tasks/{id}",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    responses(
        (status = 200, description = "Task status", body = TaskStatusResponse),
        (status = 404, description = "Task not found")
    )
)]
async fn task_status(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };
    Json(handle.status().await).into_response()
}

#[utoipa::path(
    get,
    path = "/tasks/{id}/output",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    responses(
        (status = 200, description = "Fetch task output (range)", body = TaskOutputResponse),
        (status = 404, description = "Task not found")
    )
)]
async fn task_output(
    Path(task_id): Path<String>,
    Query(query): Query<TaskOutputQuery>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let engine: Arc<TaskEngine> = state.engine.tasks();
    let offset = query.offset_bytes.unwrap_or(0);
    let max_bytes = query.max_bytes.unwrap_or(engine.config().max_bytes);

    match handle
        .output(engine.config(), query.stream, offset, max_bytes)
        .await
    {
        Ok(output) => Json(output).into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/tasks/{id}/events",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    responses(
        (status = 200, description = "SSE stream of task event frames"),
        (status = 404, description = "Task not found")
    )
)]
async fn stream_task_events(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let receiver = handle.subscribe();
    let past = handle.events_snapshot().await;

    let last_seq = past.last().map(|event| event.seq);
    let past_stream = tokio_stream::iter(past).filter_map(|event| async move {
        let json = serde_json::to_string(&event).ok()?;
        Some(Ok::<SseEvent, Infallible>(SseEvent::default().data(json)))
    });

    let last_seq_live = last_seq;
    let live_stream = BroadcastStream::new(receiver).filter_map(move |result| {
        let last_seq = last_seq_live;
        async move {
            match result {
                Ok(event) => {
                    if last_seq.map(|last| event.seq <= last).unwrap_or(false) {
                        return None;
                    }
                    let json = serde_json::to_string(&event).ok()?;
                    Some(Ok::<SseEvent, Infallible>(SseEvent::default().data(json)))
                }
                Err(_) => None,
            }
        }
    });

    let stream = past_stream.chain(live_stream);
    Sse::new(stream)
        .keep_alive(axum::response::sse::KeepAlive::new().text("ping"))
        .into_response()
}

#[utoipa::path(
    post,
    path = "/tasks/{id}/cancel",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    request_body = TaskCancelPayload,
    responses(
        (status = 202, description = "Cancel requested"),
        (status = 404, description = "Task not found")
    )
)]
async fn cancel_task(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<TaskCancelPayload>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    let reason = payload.reason.unwrap_or_else(|| "cancel".to_string());
    handle.cancel(reason);
    StatusCode::ACCEPTED.into_response()
}

#[utoipa::path(
    post,
    path = "/tasks/{id}/stdin",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    request_body = TaskWriteStdinPayload,
    responses(
        (status = 202, description = "Stdin accepted"),
        (status = 400, description = "Invalid stdin request"),
        (status = 404, description = "Task not found")
    )
)]
async fn task_write_stdin(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<TaskWriteStdinPayload>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    match handle.write_stdin(payload).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/tasks/{id}/resize",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    request_body = TaskResizePayload,
    responses(
        (status = 202, description = "Resize accepted"),
        (status = 400, description = "Invalid resize request"),
        (status = 404, description = "Task not found")
    )
)]
async fn task_resize(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<TaskResizePayload>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    match handle.resize(payload).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

#[utoipa::path(
    post,
    path = "/tasks/{id}/signal",
    params(
        ("id" = String, Path, description = "Task id")
    ),
    request_body = TaskSignalPayload,
    responses(
        (status = 202, description = "Signal accepted"),
        (status = 400, description = "Invalid signal request"),
        (status = 404, description = "Task not found")
    )
)]
async fn task_signal(
    Path(task_id): Path<String>,
    State(state): State<AppState>,
    Json(payload): Json<TaskSignalPayload>,
) -> impl IntoResponse {
    let handle = {
        let tasks = state.tasks.lock().await;
        match tasks.get(&task_id) {
            Some(handle) => handle.clone(),
            None => return StatusCode::NOT_FOUND.into_response(),
        }
    };

    match handle.signal(payload).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

#[utoipa::path(
    get,
    path = "/config/doctor",
    responses(
        (status = 200, description = "Resolved configuration summary", body = ConfigDoctorResponse)
    )
)]
async fn config_doctor(State(state): State<AppState>) -> impl IntoResponse {
    let store = state.engine.continuities();
    let (resolved_openresponses, loaded) = crate::config::resolve_openresponses_config(
        store.workspace_root(),
        crate::config::OpenResponsesOverrideInput::default(),
    );

    let sources = loaded
        .sources
        .into_iter()
        .map(|source| ConfigDoctorSource {
            path: source.path,
            status: source.status,
            error: source.error,
        })
        .collect();

    let openresponses = resolved_openresponses.map(|cfg| ConfigDoctorOpenResponses {
        provider_id: cfg.provider_id,
        route: cfg.route,
        endpoint: cfg.endpoint,
        model: cfg.model,
        has_api_key: cfg
            .api_key
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false),
        api_key_source: cfg.api_key_source,
        headers: cfg.headers.into_iter().map(|(name, _)| name).collect(),
        stateless_history: cfg.stateless_history,
        parallel_tool_calls: cfg.parallel_tool_calls,
        followup_user_message: cfg.followup_user_message,
    });

    (
        StatusCode::OK,
        Json(ConfigDoctorResponse {
            sources,
            openresponses,
        }),
    )
        .into_response()
}

async fn openapi_spec(State(state): State<AppState>) -> impl IntoResponse {
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
