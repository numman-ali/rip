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
}

impl SessionEngine {
    pub fn new(
        data_dir: PathBuf,
        workspace_root: PathBuf,
        openresponses: Option<OpenResponsesConfig>,
    ) -> Result<Self, String> {
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
    ) {
        tokio::spawn(run_session(SessionContext {
            runtime: self.runtime.clone(),
            tool_runner: self.tool_runner.clone(),
            http_client: self.http_client.clone(),
            openresponses: self.openresponses.clone(),
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
mod tests {
    use super::*;
    use rip_kernel::EventKind;
    use tempfile::tempdir;
    use tokio::time::{timeout, Duration};

    struct EnvGuard {
        key: &'static str,
        value: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: impl Into<std::ffi::OsString>) -> Self {
            let value = value.into();
            let prev = std::env::var_os(key);
            std::env::set_var(key, &value);
            Self { key, value: prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.value {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    #[test]
    fn openresponses_env_is_disabled_in_tests() {
        assert!(openresponses_from_env().is_none());
    }

    #[test]
    fn defaults_resolve_to_non_empty_paths() {
        let data_dir = default_data_dir();
        let workspace_root = default_workspace_root();
        assert!(!data_dir.as_os_str().is_empty());
        assert!(!workspace_root.as_os_str().is_empty());
    }

    #[test]
    fn cancel_session_removes_handle() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");
        let engine = SessionEngine::new(data_dir, workspace_dir, None).expect("engine");
        let handle = engine.create_session();
        let session_id = handle.session_id.clone();

        let mut sessions = HashMap::new();
        sessions.insert(session_id.clone(), handle);
        assert!(SessionEngine::cancel_session(&mut sessions, &session_id));
        assert!(!SessionEngine::cancel_session(&mut sessions, &session_id));
    }

    #[test]
    fn new_default_uses_env_paths() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");

        let _data_guard = EnvGuard::set("RIP_DATA_DIR", data_dir.to_string_lossy().to_string());
        let _workspace_guard = EnvGuard::set(
            "RIP_WORKSPACE_ROOT",
            workspace_dir.to_string_lossy().to_string(),
        );

        let engine = SessionEngine::new_default().expect("engine");
        let handle = engine.create_session();
        assert!(!handle.session_id.is_empty());
    }

    #[tokio::test]
    async fn engine_emits_session_lifecycle_events() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");
        let engine = SessionEngine::new(data_dir, workspace_dir, None).expect("engine");

        let handle = engine.create_session();
        let mut receiver = handle.subscribe();
        engine.spawn_session(handle, "hello".to_string(), None);

        let mut saw_started = false;
        let mut saw_ended = false;

        timeout(Duration::from_secs(2), async {
            loop {
                let event = receiver.recv().await.expect("event");
                match event.kind {
                    EventKind::SessionStarted { .. } => saw_started = true,
                    EventKind::SessionEnded { .. } => {
                        saw_ended = true;
                        break;
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("timeout");

        assert!(saw_started);
        assert!(saw_ended);
    }
}
