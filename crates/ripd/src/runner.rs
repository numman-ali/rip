use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rip_kernel::{Event, Runtime};
use rip_log::EventLog;
use rip_tools::{register_builtin_tools, BuiltinToolConfig, ToolRegistry, ToolRunner};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::checkpoints::WorkspaceCheckpointHook;
use crate::continuities::{ContinuityRunLink, ContinuityStore};
use crate::provider_openresponses::OpenResponsesConfig;
use crate::session::{run_session, SessionContext};
use crate::tasks::{TaskEngine, TaskEngineConfig};
use crate::workspace_lock::WorkspaceLock;

const EVENT_CHANNEL_CAPACITY: usize = 16_384;
const TOOL_MAX_CONCURRENCY: usize = 4;

#[derive(Clone)]
pub struct SessionHandle {
    pub session_id: String,
    sender: broadcast::Sender<Event>,
    events: Arc<Mutex<Vec<Event>>>,
}

impl SessionHandle {
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    pub(crate) async fn events_snapshot(&self) -> Vec<Event> {
        self.events.lock().await.clone()
    }
}

pub struct SessionEngine {
    runtime: Arc<Runtime>,
    tool_runner: Arc<ToolRunner>,
    http_client: reqwest::Client,
    openresponses: Option<OpenResponsesConfig>,
    event_log: Arc<EventLog>,
    snapshot_dir: Arc<PathBuf>,
    task_engine: Arc<TaskEngine>,
    continuity_store: Arc<ContinuityStore>,
    workspace_lock: Arc<WorkspaceLock>,
}

impl SessionEngine {
    pub fn new(
        data_dir: PathBuf,
        workspace_root: PathBuf,
        openresponses: Option<OpenResponsesConfig>,
    ) -> Result<Self, String> {
        let workspace_lock = Arc::new(WorkspaceLock::new());
        let registry = Arc::new(ToolRegistry::default());
        let builtin_config = BuiltinToolConfig {
            workspace_root: workspace_root.clone(),
            ..BuiltinToolConfig::default()
        };
        register_builtin_tools(&registry, builtin_config.clone());

        let checkpoint_hook = WorkspaceCheckpointHook::new(workspace_root.clone())
            .map_err(|err| format!("workspace checkpoint hook init failed: {err}"))?;
        let tool_runner = Arc::new(ToolRunner::with_checkpoint_hook(
            registry,
            TOOL_MAX_CONCURRENCY,
            Arc::new(checkpoint_hook),
        ));

        let event_log = Arc::new(
            EventLog::new(data_dir.join("events.jsonl"))
                .map_err(|err| format!("event log init failed: {err}"))?,
        );
        let snapshot_dir = Arc::new(data_dir.join("snapshots"));
        let task_snapshot_dir = Arc::new(data_dir.join("task_snapshots"));
        let task_engine = Arc::new(TaskEngine::new(
            TaskEngineConfig {
                workspace_root: workspace_root.clone(),
                artifact_max_bytes: builtin_config.artifact_max_bytes,
                max_bytes: builtin_config.max_bytes,
            },
            workspace_lock.clone(),
            event_log.clone(),
            task_snapshot_dir,
        ));
        let continuity_store = Arc::new(ContinuityStore::new(
            data_dir.clone(),
            workspace_root.clone(),
            event_log.clone(),
        )?);

        Ok(Self {
            runtime: Arc::new(Runtime::new()),
            tool_runner,
            http_client: reqwest::Client::new(),
            openresponses,
            event_log,
            snapshot_dir,
            task_engine,
            continuity_store,
            workspace_lock,
        })
    }

    pub fn new_default() -> Result<Self, String> {
        let data_dir = default_data_dir();
        let workspace_root = default_workspace_root();
        let openresponses = openresponses_from_env();
        Self::new(data_dir, workspace_root, openresponses)
    }

    pub fn create_session(&self) -> SessionHandle {
        let session_id = Uuid::new_v4().to_string();
        let (sender, _receiver) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        SessionHandle {
            session_id,
            sender,
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn spawn_session(
        &self,
        handle: SessionHandle,
        input: String,
        continuity: Option<ContinuityRunLink>,
        openresponses_override: Option<OpenResponsesConfig>,
    ) {
        let openresponses = openresponses_override.or_else(|| self.openresponses.clone());
        tokio::spawn(run_session(SessionContext {
            runtime: self.runtime.clone(),
            tool_runner: self.tool_runner.clone(),
            workspace_lock: self.workspace_lock.clone(),
            http_client: self.http_client.clone(),
            openresponses,
            sender: handle.sender.clone(),
            events: handle.events.clone(),
            event_log: self.event_log.clone(),
            snapshot_dir: self.snapshot_dir.clone(),
            continuities: self.continuity_store.clone(),
            continuity_run: continuity,
            server_session_id: handle.session_id.clone(),
            input,
        }));
    }

    pub fn cancel_session(sessions: &mut HashMap<String, SessionHandle>, session_id: &str) -> bool {
        sessions.remove(session_id).is_some()
    }

    pub(crate) fn tasks(&self) -> Arc<TaskEngine> {
        self.task_engine.clone()
    }

    pub fn continuities(&self) -> Arc<ContinuityStore> {
        self.continuity_store.clone()
    }
}

fn default_data_dir() -> PathBuf {
    if let Ok(value) = std::env::var("RIP_DATA_DIR") {
        return PathBuf::from(value);
    }
    PathBuf::from("data")
}

fn default_workspace_root() -> PathBuf {
    if let Ok(value) = std::env::var("RIP_WORKSPACE_ROOT") {
        return PathBuf::from(value);
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn openresponses_from_env() -> Option<OpenResponsesConfig> {
    #[cfg(not(test))]
    {
        OpenResponsesConfig::from_env()
    }
    #[cfg(test)]
    {
        None
    }
}

#[cfg(test)]
mod tests;
