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
    ) {
        tokio::spawn(run_session(SessionContext {
            runtime: self.runtime.clone(),
            tool_runner: self.tool_runner.clone(),
            workspace_lock: self.workspace_lock.clone(),
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
    use rip_log::{verify_snapshot, write_snapshot, EventLog};
    use serde_json::json;
    use std::path::PathBuf;
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

    async fn wait_for_event<F>(receiver: &mut broadcast::Receiver<Event>, predicate: F) -> Event
    where
        F: Fn(&EventKind) -> bool,
    {
        timeout(Duration::from_secs(3), async {
            loop {
                let event = receiver.recv().await.expect("event");
                if predicate(&event.kind) {
                    return event;
                }
            }
        })
        .await
        .expect("timeout")
    }

    async fn wait_for_snapshot(path: PathBuf) {
        timeout(Duration::from_secs(3), async {
            loop {
                if path.exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("snapshot timeout");
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn workspace_mutations_are_serialized_across_sessions() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");
        let engine = SessionEngine::new(data_dir.clone(), workspace_dir, None).expect("engine");

        let handle_one = engine.create_session();
        let mut rx_one = handle_one.subscribe();
        let handle_two = engine.create_session();
        let mut rx_two = handle_two.subscribe();

        engine.spawn_session(
            handle_one.clone(),
            r#"{"tool":"bash","args":{"command":"sleep 0.2"}}"#.to_string(),
            None,
        );

        let _ = wait_for_event(&mut rx_one, |kind| {
            matches!(kind, EventKind::ToolStarted { .. })
        })
        .await;

        engine.spawn_session(
            handle_two.clone(),
            r#"{"tool":"bash","args":{"command":"printf 'ok'"}}"#.to_string(),
            None,
        );

        let _ = wait_for_event(&mut rx_one, |kind| {
            matches!(kind, EventKind::ToolEnded { .. })
        })
        .await;
        let _ = wait_for_event(&mut rx_two, |kind| {
            matches!(kind, EventKind::ToolStarted { .. })
        })
        .await;
        let _ = wait_for_event(&mut rx_two, |kind| {
            matches!(kind, EventKind::ToolEnded { .. })
        })
        .await;
        let _ = wait_for_event(&mut rx_one, |kind| {
            matches!(kind, EventKind::SessionEnded { .. })
        })
        .await;
        let _ = wait_for_event(&mut rx_two, |kind| {
            matches!(kind, EventKind::SessionEnded { .. })
        })
        .await;

        let log = EventLog::new(data_dir.join("events.jsonl")).expect("log");
        let events = log.replay().expect("replay");

        let mut first: Option<String> = None;
        let mut switched = false;
        let mut saw_second = false;

        for event in events.iter() {
            let is_tool_event = matches!(
                event.kind,
                EventKind::ToolStarted { .. }
                    | EventKind::ToolStdout { .. }
                    | EventKind::ToolStderr { .. }
                    | EventKind::ToolEnded { .. }
                    | EventKind::ToolFailed { .. }
            );
            if !is_tool_event {
                continue;
            }

            let sid = event.session_id.clone();
            match first.as_ref() {
                None => first = Some(sid),
                Some(primary) if sid == *primary => {
                    if switched {
                        panic!("tool events interleaved across sessions");
                    }
                }
                Some(_) => {
                    switched = true;
                    saw_second = true;
                }
            }
        }

        assert!(saw_second, "expected tool events from both sessions");
        let snapshot_one = data_dir
            .join("snapshots")
            .join(format!("{}.json", handle_one.session_id));
        let snapshot_two = data_dir
            .join("snapshots")
            .join(format!("{}.json", handle_two.session_id));
        wait_for_snapshot(snapshot_one.clone()).await;
        wait_for_snapshot(snapshot_two.clone()).await;
        verify_snapshot(&log, snapshot_one).expect("snapshot one");
        verify_snapshot(&log, snapshot_two).expect("snapshot two");
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn workspace_mutations_are_serialized_with_tasks() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");
        let engine = SessionEngine::new(data_dir.clone(), workspace_dir, None).expect("engine");

        let payload = crate::tasks::TaskSpawnPayload {
            tool: "bash".to_string(),
            args: json!({"command":"sleep 0.2"}),
            title: None,
            execution_mode: None,
            origin_session_id: None,
        };
        let task_handle = engine.tasks().create_task(&payload);
        let mut task_rx = task_handle.subscribe();
        engine.tasks().spawn_task(task_handle.clone(), payload);

        let _ = wait_for_event(&mut task_rx, |kind| {
            matches!(kind, EventKind::ToolTaskStatus { status, .. } if *status == rip_kernel::ToolTaskStatus::Running)
        })
        .await;

        let session_handle = engine.create_session();
        let mut session_rx = session_handle.subscribe();
        engine.spawn_session(
            session_handle.clone(),
            r#"{"tool":"bash","args":{"command":"printf 'ok'"}}"#.to_string(),
            None,
        );

        let _ = wait_for_event(&mut task_rx, |kind| {
            matches!(kind, EventKind::ToolTaskStatus { status, .. } if *status == rip_kernel::ToolTaskStatus::Exited || *status == rip_kernel::ToolTaskStatus::Cancelled || *status == rip_kernel::ToolTaskStatus::Failed)
        })
        .await;
        let _ = wait_for_event(&mut session_rx, |kind| {
            matches!(kind, EventKind::ToolStarted { .. })
        })
        .await;
        let _ = wait_for_event(&mut session_rx, |kind| {
            matches!(kind, EventKind::ToolEnded { .. })
        })
        .await;
        let _ = wait_for_event(&mut session_rx, |kind| {
            matches!(kind, EventKind::SessionEnded { .. })
        })
        .await;

        let log = EventLog::new(data_dir.join("events.jsonl")).expect("log");
        let events = log.replay().expect("replay");
        let session_tool_start = events.iter().position(|event| {
            event.session_id == session_handle.session_id
                && matches!(event.kind, EventKind::ToolStarted { .. })
        });
        let session_tool_start = session_tool_start.expect("session tool start");

        let mut task_after_session = false;
        for event in events.iter().skip(session_tool_start) {
            if matches!(
                event.kind,
                EventKind::ToolTaskSpawned { .. }
                    | EventKind::ToolTaskStatus { .. }
                    | EventKind::ToolTaskCancelRequested { .. }
                    | EventKind::ToolTaskCancelled { .. }
                    | EventKind::ToolTaskOutputDelta { .. }
                    | EventKind::ToolTaskStdinWritten { .. }
                    | EventKind::ToolTaskResized { .. }
                    | EventKind::ToolTaskSignalled { .. }
            ) {
                task_after_session = true;
                break;
            }
        }
        assert!(
            !task_after_session,
            "task events interleaved after session tool start"
        );

        let session_snapshot = data_dir
            .join("snapshots")
            .join(format!("{}.json", session_handle.session_id));
        let task_snapshot = data_dir
            .join("task_snapshots")
            .join(format!("{}.json", task_handle.task_id));
        wait_for_snapshot(session_snapshot.clone()).await;
        wait_for_snapshot(task_snapshot.clone()).await;
        verify_snapshot(&log, session_snapshot).expect("session snapshot");
        verify_snapshot(&log, task_snapshot).expect("task snapshot");
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn continuity_logs_workspace_tool_side_effects_across_parallel_runs() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");
        let engine = SessionEngine::new(data_dir.clone(), workspace_dir, None).expect("engine");

        let store = engine.continuities();
        let thread_id = store.ensure_default().expect("thread");
        let actor_id = "alice".to_string();
        let origin = "cli".to_string();

        let handle_one = engine.create_session();
        let mut rx_one = handle_one.subscribe();
        let input_one = r#"{"tool":"bash","args":{"command":"sleep 0.2"}}"#.to_string();
        let message_one = store
            .append_message(
                &thread_id,
                actor_id.clone(),
                origin.clone(),
                input_one.clone(),
            )
            .expect("append message");
        store
            .append_run_spawned(
                &thread_id,
                &message_one,
                &handle_one.session_id,
                actor_id.clone(),
                origin.clone(),
            )
            .expect("run spawned");
        engine.spawn_session(
            handle_one.clone(),
            input_one,
            Some(ContinuityRunLink {
                continuity_id: thread_id.clone(),
                message_id: message_one.clone(),
                actor_id: actor_id.clone(),
                origin: origin.clone(),
            }),
        );

        let _ = wait_for_event(&mut rx_one, |kind| {
            matches!(kind, EventKind::ToolStarted { .. })
        })
        .await;

        let handle_two = engine.create_session();
        let mut rx_two = handle_two.subscribe();
        let input_two = r#"{"tool":"write","args":{"path":"note.txt","content":"hi"}}"#.to_string();
        let message_two = store
            .append_message(
                &thread_id,
                actor_id.clone(),
                origin.clone(),
                input_two.clone(),
            )
            .expect("append message");
        store
            .append_run_spawned(
                &thread_id,
                &message_two,
                &handle_two.session_id,
                actor_id.clone(),
                origin.clone(),
            )
            .expect("run spawned");
        engine.spawn_session(
            handle_two.clone(),
            input_two,
            Some(ContinuityRunLink {
                continuity_id: thread_id.clone(),
                message_id: message_two.clone(),
                actor_id: actor_id.clone(),
                origin: origin.clone(),
            }),
        );

        let _ = wait_for_event(&mut rx_one, |kind| {
            matches!(kind, EventKind::ToolEnded { .. })
        })
        .await;
        let _ = wait_for_event(&mut rx_two, |kind| {
            matches!(kind, EventKind::ToolStarted { .. })
        })
        .await;
        let _ = wait_for_event(&mut rx_two, |kind| {
            matches!(kind, EventKind::ToolEnded { .. })
        })
        .await;
        let _ = wait_for_event(&mut rx_one, |kind| {
            matches!(kind, EventKind::SessionEnded { .. })
        })
        .await;
        let _ = wait_for_event(&mut rx_two, |kind| {
            matches!(kind, EventKind::SessionEnded { .. })
        })
        .await;

        let log = EventLog::new(data_dir.join("events.jsonl")).expect("log");
        let session_one_tool_id = log
            .replay_session(&handle_one.session_id)
            .expect("replay session one")
            .iter()
            .find_map(|event| match &event.kind {
                EventKind::ToolStarted { tool_id, .. } => Some(tool_id.clone()),
                _ => None,
            })
            .expect("session one tool id");
        let session_two_tool_id = log
            .replay_session(&handle_two.session_id)
            .expect("replay session two")
            .iter()
            .find_map(|event| match &event.kind {
                EventKind::ToolStarted { tool_id, .. } => Some(tool_id.clone()),
                _ => None,
            })
            .expect("session two tool id");

        let thread_events = store.replay_events(&thread_id).expect("replay thread");
        let mut tool_frames = thread_events
            .iter()
            .filter_map(|event| match &event.kind {
                EventKind::ContinuityToolSideEffects {
                    run_session_id,
                    tool_id,
                    tool_name,
                    affected_paths,
                    checkpoint_id,
                    actor_id,
                    origin,
                } => Some((
                    event.seq,
                    run_session_id.as_str(),
                    tool_id.as_str(),
                    tool_name.as_str(),
                    affected_paths.as_ref(),
                    checkpoint_id.as_deref(),
                    actor_id.as_str(),
                    origin.as_str(),
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        tool_frames.sort_by_key(|(seq, ..)| *seq);
        assert_eq!(tool_frames.len(), 2);

        let (
            seq_one,
            run_one,
            tool_one,
            name_one,
            affected_one,
            checkpoint_one,
            actor_one,
            origin_one,
        ) = tool_frames[0];
        assert_eq!(run_one, handle_one.session_id.as_str());
        assert_eq!(tool_one, session_one_tool_id.as_str());
        assert_eq!(name_one, "bash");
        assert!(affected_one.is_none(), "bash affected_paths should be null");
        assert!(
            checkpoint_one.is_none(),
            "bash checkpoint_id should be null"
        );
        assert_eq!(actor_one, "alice");
        assert_eq!(origin_one, "cli");

        let (
            seq_two,
            run_two,
            tool_two,
            name_two,
            affected_two,
            checkpoint_two,
            actor_two,
            origin_two,
        ) = tool_frames[1];
        assert!(
            seq_one < seq_two,
            "expected sequential tool side effects ordering"
        );
        assert_eq!(run_two, handle_two.session_id.as_str());
        assert_eq!(tool_two, session_two_tool_id.as_str());
        assert_eq!(name_two, "write");
        let expected_paths = vec!["note.txt".to_string()];
        assert_eq!(affected_two, Some(&expected_paths));
        assert!(
            checkpoint_two.is_some(),
            "write should record an auto-checkpoint id"
        );
        assert_eq!(actor_two, "alice");
        assert_eq!(origin_two, "cli");

        let snapshot_path = write_snapshot(
            data_dir.join("continuity_snapshots"),
            &thread_id,
            &thread_events,
        )
        .expect("continuity snapshot");
        verify_snapshot(&log, snapshot_path).expect("continuity snapshot verify");
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn continuity_logs_workspace_tool_side_effects_with_parallel_tasks() {
        let dir = tempdir().expect("tmp");
        let data_dir = dir.path().join("data");
        let workspace_dir = dir.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("workspace");
        let engine = SessionEngine::new(data_dir.clone(), workspace_dir, None).expect("engine");

        let store = engine.continuities();
        let thread_id = store.ensure_default().expect("thread");

        let payload = crate::tasks::TaskSpawnPayload {
            tool: "bash".to_string(),
            args: json!({"command":"sleep 0.2"}),
            title: None,
            execution_mode: None,
            origin_session_id: None,
        };
        let task_handle = engine.tasks().create_task(&payload);
        let mut task_rx = task_handle.subscribe();
        engine.tasks().spawn_task(task_handle.clone(), payload);
        let _ = wait_for_event(&mut task_rx, |kind| {
            matches!(kind, EventKind::ToolTaskStatus { status, .. } if *status == rip_kernel::ToolTaskStatus::Running)
        })
        .await;

        let actor_id = "bob".to_string();
        let origin = "cli".to_string();
        let session_handle = engine.create_session();
        let mut session_rx = session_handle.subscribe();
        let input =
            r#"{"tool":"write","args":{"path":"task_note.txt","content":"hi"}}"#.to_string();
        let message_id = store
            .append_message(&thread_id, actor_id.clone(), origin.clone(), input.clone())
            .expect("append message");
        store
            .append_run_spawned(
                &thread_id,
                &message_id,
                &session_handle.session_id,
                actor_id.clone(),
                origin.clone(),
            )
            .expect("run spawned");
        engine.spawn_session(
            session_handle.clone(),
            input,
            Some(ContinuityRunLink {
                continuity_id: thread_id.clone(),
                message_id: message_id.clone(),
                actor_id: actor_id.clone(),
                origin: origin.clone(),
            }),
        );

        let _ = wait_for_event(&mut task_rx, |kind| {
            matches!(kind, EventKind::ToolTaskStatus { status, .. } if *status == rip_kernel::ToolTaskStatus::Exited || *status == rip_kernel::ToolTaskStatus::Cancelled || *status == rip_kernel::ToolTaskStatus::Failed)
        })
        .await;
        let _ = wait_for_event(&mut session_rx, |kind| {
            matches!(kind, EventKind::ToolStarted { .. })
        })
        .await;
        let _ = wait_for_event(&mut session_rx, |kind| {
            matches!(kind, EventKind::ToolEnded { .. })
        })
        .await;
        let _ = wait_for_event(&mut session_rx, |kind| {
            matches!(kind, EventKind::SessionEnded { .. })
        })
        .await;

        let thread_events = store.replay_events(&thread_id).expect("replay thread");
        assert!(
            thread_events
                .iter()
                .any(|event| matches!(&event.kind, EventKind::ContinuityToolSideEffects { .. })),
            "expected continuity_tool_side_effects frame"
        );

        let log = EventLog::new(data_dir.join("events.jsonl")).expect("log");
        let snapshot_path = write_snapshot(
            data_dir.join("continuity_snapshots"),
            &thread_id,
            &thread_events,
        )
        .expect("continuity snapshot");
        verify_snapshot(&log, snapshot_path).expect("continuity snapshot verify");
    }
}
