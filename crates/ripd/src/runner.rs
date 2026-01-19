use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rip_kernel::{Event, Runtime};
use rip_log::EventLog;
use rip_tools::{register_builtin_tools, BuiltinToolConfig, ToolRegistry, ToolRunner};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::checkpoints::WorkspaceCheckpointHook;
use crate::provider_openresponses::OpenResponsesConfig;
use crate::session::{run_session, SessionContext};

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
}

pub struct SessionEngine {
    runtime: Arc<Runtime>,
    tool_runner: Arc<ToolRunner>,
    http_client: reqwest::Client,
    openresponses: Option<OpenResponsesConfig>,
    event_log: Arc<EventLog>,
    snapshot_dir: Arc<PathBuf>,
}

impl SessionEngine {
    pub fn new(
        data_dir: PathBuf,
        workspace_root: PathBuf,
        openresponses: Option<OpenResponsesConfig>,
    ) -> Result<Self, String> {
        let registry = Arc::new(ToolRegistry::default());
        register_builtin_tools(
            &registry,
            BuiltinToolConfig {
                workspace_root: workspace_root.clone(),
                ..BuiltinToolConfig::default()
            },
        );

        let checkpoint_hook = WorkspaceCheckpointHook::new(workspace_root)
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

        Ok(Self {
            runtime: Arc::new(Runtime::new()),
            tool_runner,
            http_client: reqwest::Client::new(),
            openresponses,
            event_log,
            snapshot_dir,
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

    pub fn spawn_session(&self, handle: SessionHandle, input: String) {
        tokio::spawn(run_session(SessionContext {
            runtime: self.runtime.clone(),
            tool_runner: self.tool_runner.clone(),
            http_client: self.http_client.clone(),
            openresponses: self.openresponses.clone(),
            sender: handle.sender.clone(),
            events: handle.events.clone(),
            event_log: self.event_log.clone(),
            snapshot_dir: self.snapshot_dir.clone(),
            server_session_id: handle.session_id.clone(),
            input,
        }));
    }

    pub fn cancel_session(sessions: &mut HashMap<String, SessionHandle>, session_id: &str) -> bool {
        sessions.remove(session_id).is_some()
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

    #[tokio::test]
    async fn engine_emits_session_lifecycle_events() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");
        let engine = SessionEngine::new(data_dir, workspace_dir, None).expect("engine");

        let handle = engine.create_session();
        let mut receiver = handle.subscribe();
        engine.spawn_session(handle, "hello".to_string());

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
