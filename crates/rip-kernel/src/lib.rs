mod commands;
mod hooks;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub use commands::{Command, CommandContext, CommandHandler, CommandRegistry, CommandResult};
pub use hooks::{Hook, HookContext, HookEngine, HookEventKind, HookHandler, HookOutcome};

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub id: String,
    pub session_id: String,
    pub timestamp_ms: u64,
    pub seq: u64,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Session,
    Task,
    Continuity,
    Artifact,
}

impl Event {
    pub fn stream_kind(&self) -> StreamKind {
        match &self.kind {
            EventKind::ContinuityCreated { .. }
            | EventKind::ContinuityMessageAppended { .. }
            | EventKind::ContinuityRunSpawned { .. }
            | EventKind::ContinuityContextCompiled { .. }
            | EventKind::ContinuityCompactionCheckpointCreated { .. }
            | EventKind::ContinuityRunEnded { .. }
            | EventKind::ContinuityToolSideEffects { .. }
            | EventKind::ContinuityBranched { .. }
            | EventKind::ContinuityHandoffCreated { .. } => StreamKind::Continuity,
            EventKind::ToolTaskSpawned { .. }
            | EventKind::ToolTaskStatus { .. }
            | EventKind::ToolTaskCancelRequested { .. }
            | EventKind::ToolTaskCancelled { .. }
            | EventKind::ToolTaskOutputDelta { .. }
            | EventKind::ToolTaskStdinWritten { .. }
            | EventKind::ToolTaskResized { .. }
            | EventKind::ToolTaskSignalled { .. } => StreamKind::Task,
            _ => StreamKind::Session,
        }
    }

    pub fn stream_id(&self) -> &str {
        &self.session_id
    }
}

#[derive(Serialize)]
struct EventWire<'a> {
    id: &'a str,
    session_id: &'a str,
    stream_kind: StreamKind,
    stream_id: &'a str,
    timestamp_ms: u64,
    seq: u64,
    #[serde(flatten)]
    kind: &'a EventKind,
}

impl Serialize for Event {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        EventWire {
            id: &self.id,
            session_id: &self.session_id,
            stream_kind: self.stream_kind(),
            stream_id: self.stream_id(),
            timestamp_ms: self.timestamp_ms,
            seq: self.seq,
            kind: &self.kind,
        }
        .serialize(serializer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderEventStatus {
    Event,
    Done,
    InvalidJson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTaskExecutionMode {
    Pipes,
    Pty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTaskStatus {
    Queued,
    Running,
    Exited,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTaskStream {
    Stdout,
    Stderr,
    Pty,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointAction {
    Create,
    Rewind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    SessionStarted {
        #[serde(default, alias = "prompt")]
        input: String,
    },
    #[serde(alias = "output")]
    OutputTextDelta {
        #[serde(alias = "content")]
        delta: String,
    },
    SessionEnded {
        reason: String,
    },
    ContinuityCreated {
        /// Stable workspace identifier (currently the workspace root path as a string).
        workspace: String,
        title: Option<String>,
    },
    ContinuityMessageAppended {
        actor_id: String,
        origin: String,
        content: String,
    },
    ContinuityRunSpawned {
        run_session_id: String,
        message_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<String>,
    },
    ContinuityContextCompiled {
        run_session_id: String,
        bundle_artifact_id: String,
        compiler_id: String,
        compiler_strategy: String,
        from_seq: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_message_id: Option<String>,
        actor_id: String,
        origin: String,
    },
    ContinuityCompactionCheckpointCreated {
        checkpoint_id: String,
        cut_rule_id: String,
        summary_kind: String,
        summary_artifact_id: String,
        from_seq: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_message_id: Option<String>,
        to_seq: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to_message_id: Option<String>,
        actor_id: String,
        origin: String,
    },
    ContinuityRunEnded {
        run_session_id: String,
        message_id: String,
        reason: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<String>,
    },
    ContinuityToolSideEffects {
        run_session_id: String,
        tool_id: String,
        tool_name: String,
        affected_paths: Option<Vec<String>>,
        checkpoint_id: Option<String>,
        actor_id: String,
        origin: String,
    },
    ContinuityBranched {
        parent_thread_id: String,
        parent_seq: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent_message_id: Option<String>,
        actor_id: String,
        origin: String,
    },
    ContinuityHandoffCreated {
        from_thread_id: String,
        from_seq: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from_message_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary_artifact_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary_markdown: Option<String>,
        actor_id: String,
        origin: String,
    },
    ToolStarted {
        tool_id: String,
        name: String,
        args: Value,
        timeout_ms: Option<u64>,
    },
    ToolStdout {
        tool_id: String,
        chunk: String,
    },
    ToolStderr {
        tool_id: String,
        chunk: String,
    },
    ToolEnded {
        tool_id: String,
        exit_code: i32,
        duration_ms: u64,
        artifacts: Option<Value>,
    },
    ToolFailed {
        tool_id: String,
        error: String,
    },
    ProviderEvent {
        provider: String,
        status: ProviderEventStatus,
        event_name: Option<String>,
        data: Option<Value>,
        raw: Option<String>,
        errors: Vec<String>,
        response_errors: Vec<String>,
    },
    CheckpointCreated {
        checkpoint_id: String,
        label: String,
        created_at_ms: u64,
        files: Vec<String>,
        auto: bool,
        tool_name: Option<String>,
    },
    CheckpointRewound {
        checkpoint_id: String,
        label: String,
        files: Vec<String>,
    },
    CheckpointFailed {
        action: CheckpointAction,
        error: String,
    },
    ToolTaskSpawned {
        task_id: String,
        tool_name: String,
        args: Value,
        cwd: Option<String>,
        title: Option<String>,
        execution_mode: ToolTaskExecutionMode,
        origin_session_id: Option<String>,
        artifacts: Option<Value>,
    },
    ToolTaskStatus {
        task_id: String,
        status: ToolTaskStatus,
        exit_code: Option<i32>,
        started_at_ms: Option<u64>,
        ended_at_ms: Option<u64>,
        artifacts: Option<Value>,
        error: Option<String>,
    },
    ToolTaskCancelRequested {
        task_id: String,
        reason: String,
    },
    ToolTaskCancelled {
        task_id: String,
        reason: String,
        wall_time_ms: Option<u64>,
    },
    ToolTaskOutputDelta {
        task_id: String,
        stream: ToolTaskStream,
        chunk: String,
        artifacts: Option<Value>,
    },
    ToolTaskStdinWritten {
        task_id: String,
        chunk_b64: String,
    },
    ToolTaskResized {
        task_id: String,
        rows: u16,
        cols: u16,
    },
    ToolTaskSignalled {
        task_id: String,
        signal: String,
    },
}

#[derive(Clone)]
pub struct Runtime {
    hooks: Arc<HookEngine>,
    commands: Arc<CommandRegistry>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            hooks: Arc::new(HookEngine::new()),
            commands: Arc::new(CommandRegistry::new()),
        }
    }

    pub fn start_session(&self, input: String) -> Session {
        Session::new(input, self.hooks.clone())
    }

    pub fn start_session_with_id(&self, session_id: impl Into<String>, input: String) -> Session {
        Session::with_id(session_id.into(), input, self.hooks.clone())
    }

    pub fn register_hook<F>(&self, name: impl Into<String>, event: HookEventKind, handler: F)
    where
        F: Fn(&HookContext) -> HookOutcome + Send + Sync + 'static,
    {
        let hook = Hook::new(name, event, Arc::new(handler));
        self.hooks.register(hook);
    }

    pub fn register_command<F>(
        &self,
        name: impl Into<String>,
        description: impl Into<String>,
        handler: F,
    ) -> Result<(), String>
    where
        F: Fn(CommandContext) -> CommandResult + Send + Sync + 'static,
    {
        let command = Command::new(name, description, Arc::new(handler));
        self.commands.register(command)
    }

    pub fn hooks(&self) -> Arc<HookEngine> {
        self.hooks.clone()
    }

    pub fn commands(&self) -> Arc<CommandRegistry> {
        self.commands.clone()
    }
}

pub struct Session {
    id: String,
    input: String,
    seq: u64,
    stage: Stage,
    hooks: Arc<HookEngine>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Stage {
    Start,
    Output,
    End,
    Done,
}

impl Session {
    pub fn new(input: String, hooks: Arc<HookEngine>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            input,
            seq: 0,
            stage: Stage::Start,
            hooks,
        }
    }

    pub fn with_id(id: String, input: String, hooks: Arc<HookEngine>) -> Self {
        Self {
            id,
            input,
            seq: 0,
            stage: Stage::Start,
            hooks,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn set_seq(&mut self, seq: u64) {
        self.seq = seq;
    }

    pub fn next_event(&mut self) -> Option<Event> {
        let (next_stage, kind) = match self.stage {
            Stage::Start => (
                Stage::Output,
                EventKind::SessionStarted {
                    input: self.input.clone(),
                },
            ),
            Stage::Output => (
                Stage::End,
                EventKind::OutputTextDelta {
                    delta: format!("ack: {}", self.input),
                },
            ),
            Stage::End => (
                Stage::Done,
                EventKind::SessionEnded {
                    reason: "completed".to_string(),
                },
            ),
            Stage::Done => return None,
        };

        self.stage = next_stage;

        let timestamp_ms = now_ms();
        let event = Event {
            id: Uuid::new_v4().to_string(),
            session_id: self.id.clone(),
            timestamp_ms,
            seq: self.seq,
            kind,
        };

        let (hook_event, output) = match &event.kind {
            EventKind::SessionStarted { .. } => (Some(HookEventKind::SessionStarted), None),
            EventKind::OutputTextDelta { delta } => {
                (Some(HookEventKind::Output), Some(delta.clone()))
            }
            EventKind::SessionEnded { .. } => (Some(HookEventKind::SessionEnded), None),
            EventKind::ContinuityCreated { .. }
            | EventKind::ContinuityMessageAppended { .. }
            | EventKind::ContinuityRunSpawned { .. }
            | EventKind::ContinuityContextCompiled { .. }
            | EventKind::ContinuityCompactionCheckpointCreated { .. }
            | EventKind::ContinuityRunEnded { .. }
            | EventKind::ContinuityToolSideEffects { .. }
            | EventKind::ContinuityBranched { .. }
            | EventKind::ContinuityHandoffCreated { .. } => (None, None),
            EventKind::ProviderEvent { .. }
            | EventKind::ToolStarted { .. }
            | EventKind::ToolStdout { .. }
            | EventKind::ToolStderr { .. }
            | EventKind::ToolEnded { .. }
            | EventKind::ToolFailed { .. }
            | EventKind::CheckpointCreated { .. }
            | EventKind::CheckpointRewound { .. }
            | EventKind::CheckpointFailed { .. }
            | EventKind::ToolTaskSpawned { .. }
            | EventKind::ToolTaskStatus { .. }
            | EventKind::ToolTaskCancelRequested { .. }
            | EventKind::ToolTaskCancelled { .. }
            | EventKind::ToolTaskOutputDelta { .. }
            | EventKind::ToolTaskStdinWritten { .. }
            | EventKind::ToolTaskResized { .. }
            | EventKind::ToolTaskSignalled { .. } => (None, None),
        };

        if let Some(hook_event) = hook_event {
            let ctx = HookContext {
                session_id: self.id.clone(),
                seq: self.seq,
                timestamp_ms,
                event: hook_event,
                output,
            };

            match self.hooks.run(&ctx) {
                HookOutcome::Continue => {
                    self.seq += 1;
                    Some(event)
                }
                HookOutcome::Abort { reason } => {
                    self.stage = Stage::Done;
                    let abort_event = Event {
                        id: Uuid::new_v4().to_string(),
                        session_id: self.id.clone(),
                        timestamp_ms: now_ms(),
                        seq: self.seq,
                        kind: EventKind::SessionEnded { reason },
                    };
                    self.seq += 1;
                    Some(abort_event)
                }
            }
        } else {
            self.seq += 1;
            Some(event)
        }
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_emits_three_events_in_order() {
        let runtime = Runtime::new();
        let mut session = runtime.start_session("hello".to_string());

        let mut events = Vec::new();
        while let Some(event) = session.next_event() {
            events.push(event);
        }

        assert_eq!(events.len(), 3);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
        assert_eq!(events[2].seq, 2);

        matches!(events[0].kind, EventKind::SessionStarted { .. });
        matches!(events[1].kind, EventKind::OutputTextDelta { .. });
        matches!(events[2].kind, EventKind::SessionEnded { .. });
    }

    #[test]
    fn event_serializes_to_json() {
        let runtime = Runtime::new();
        let mut session = runtime.start_session("test".to_string());
        let event = session.next_event().expect("event");
        let json = serde_json::to_string(&event).expect("json");
        assert!(json.contains("session_started"));
        assert!(json.contains("input"));
    }

    #[test]
    fn legacy_event_formats_deserialize() {
        let legacy_session_started =
            r#"{"id":"e1","session_id":"s1","timestamp_ms":0,"seq":0,"type":"session_started"}"#;
        let event: Event = serde_json::from_str(legacy_session_started).expect("deserialize");
        assert!(matches!(
            event.kind,
            EventKind::SessionStarted { input } if input.is_empty()
        ));

        let legacy_session_started_with_prompt = r#"{"id":"e2","session_id":"s1","timestamp_ms":0,"seq":0,"type":"session_started","prompt":"hello"}"#;
        let event: Event =
            serde_json::from_str(legacy_session_started_with_prompt).expect("deserialize");
        assert!(matches!(
            event.kind,
            EventKind::SessionStarted { input } if input == "hello"
        ));

        let legacy_output = r#"{"id":"e3","session_id":"s1","timestamp_ms":0,"seq":1,"type":"output","content":"ack: hello"}"#;
        let event: Event = serde_json::from_str(legacy_output).expect("deserialize");
        assert!(matches!(
            event.kind,
            EventKind::OutputTextDelta { delta } if delta == "ack: hello"
        ));
    }

    #[test]
    fn session_started_includes_input() {
        let runtime = Runtime::new();
        let mut session = runtime.start_session("hello".to_string());
        let event = session.next_event().expect("event");
        match event.kind {
            EventKind::SessionStarted { input } => assert_eq!(input, "hello"),
            _ => panic!("expected session_started"),
        }
    }

    #[test]
    fn session_seq_can_be_overridden() {
        let runtime = Runtime::new();
        let mut session = runtime.start_session("hello".to_string());
        assert_eq!(session.seq(), 0);
        session.set_seq(42);
        assert_eq!(session.seq(), 42);
        let event = session.next_event().expect("event");
        assert_eq!(event.seq, 42);
    }

    #[test]
    fn start_session_with_id_sets_id() {
        let runtime = Runtime::new();
        let session = runtime.start_session_with_id("custom", "hello".to_string());
        assert_eq!(session.id(), "custom");
    }

    #[test]
    fn hook_abort_ends_session_early() {
        let runtime = Runtime::new();
        runtime.register_hook("abort-on-output", HookEventKind::Output, |_| {
            HookOutcome::Abort {
                reason: "stop".to_string(),
            }
        });

        let mut session = runtime.start_session("hello".to_string());
        let mut events = Vec::new();
        while let Some(event) = session.next_event() {
            events.push(event);
        }

        assert_eq!(events.len(), 2);
        matches!(events[0].kind, EventKind::SessionStarted { .. });
        matches!(events[1].kind, EventKind::SessionEnded { .. });
    }

    #[test]
    fn command_registry_executes() {
        let runtime = Runtime::new();
        runtime
            .register_command("ping", "test command", |_ctx| Ok("pong".to_string()))
            .expect("register");

        let registry = runtime.commands();
        let result = registry.execute(
            "ping",
            CommandContext {
                session_id: None,
                args: Vec::new(),
                raw: "ping".to_string(),
            },
        );

        assert_eq!(result.expect("command"), "pong");
    }

    #[test]
    fn command_registry_rejects_duplicates() {
        let runtime = Runtime::new();
        runtime
            .register_command("dup", "first", |_ctx| Ok("ok".to_string()))
            .expect("register");
        let result = runtime.commands().execute(
            "dup",
            CommandContext {
                session_id: None,
                args: Vec::new(),
                raw: "dup".to_string(),
            },
        );
        assert_eq!(result.expect("execute"), "ok");
        let err = runtime
            .register_command("dup", "second", |_ctx| Ok("ok".to_string()))
            .expect_err("error");
        assert!(err.contains("already registered"));
    }

    #[test]
    fn command_registry_lists_commands() {
        let runtime = Runtime::new();
        runtime
            .register_command("a", "first", |_ctx| Ok("a".to_string()))
            .expect("register");
        runtime
            .register_command("b", "second", |_ctx| Ok("b".to_string()))
            .expect("register");

        let mut names: Vec<String> = runtime
            .commands()
            .list()
            .into_iter()
            .map(|cmd| cmd.name)
            .collect();
        names.sort();
        assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
        let result = runtime.commands().execute(
            "a",
            CommandContext {
                session_id: None,
                args: Vec::new(),
                raw: "a".to_string(),
            },
        );
        assert_eq!(result.expect("execute"), "a");
        let result = runtime.commands().execute(
            "b",
            CommandContext {
                session_id: None,
                args: Vec::new(),
                raw: "b".to_string(),
            },
        );
        assert_eq!(result.expect("execute"), "b");
    }

    #[test]
    fn command_registry_unknown_command_errors() {
        let runtime = Runtime::new();
        let result = runtime.commands().execute(
            "missing",
            CommandContext {
                session_id: None,
                args: Vec::new(),
                raw: "missing".to_string(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn hooks_run_in_order() {
        let runtime = Runtime::new();
        let order: Arc<std::sync::Mutex<Vec<&'static str>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let first = order.clone();
        let second = order.clone();

        runtime.register_hook("first", HookEventKind::SessionStarted, move |_| {
            first.lock().expect("lock").push("first");
            HookOutcome::Continue
        });
        runtime.register_hook("second", HookEventKind::SessionStarted, move |_| {
            second.lock().expect("lock").push("second");
            HookOutcome::Continue
        });

        let mut session = runtime.start_session("hello".to_string());
        session.next_event();

        let recorded = order.lock().expect("lock").clone();
        assert_eq!(recorded, vec!["first", "second"]);
    }

    #[test]
    fn runtime_default_exposes_ids_and_hooks() {
        let runtime = Runtime::default();
        let session = runtime.start_session("hello".to_string());
        assert!(!session.id().is_empty());

        let hooks = runtime.hooks();
        let ctx = HookContext {
            session_id: session.id().to_string(),
            seq: 0,
            timestamp_ms: 0,
            event: HookEventKind::SessionStarted,
            output: None,
        };
        assert_eq!(hooks.run(&ctx), HookOutcome::Continue);
    }

    #[test]
    fn runtime_exposes_commands_registry() {
        let runtime = Runtime::new();
        let registry = runtime.commands();
        registry
            .register(Command::new(
                "noop",
                "no-op",
                std::sync::Arc::new(|_ctx| Ok("ok".to_string())),
            ))
            .expect("register");
        let result = registry.execute(
            "noop",
            CommandContext {
                session_id: None,
                args: Vec::new(),
                raw: String::new(),
            },
        );
        assert_eq!(result.expect("execute"), "ok");
    }

    #[test]
    fn runtime_registers_command_and_hook() {
        let runtime = Runtime::new();
        runtime
            .register_command("echo", "echo", |ctx| Ok(ctx.raw))
            .expect("register command");
        let result = runtime.commands().execute(
            "echo",
            CommandContext {
                session_id: None,
                args: vec!["hi".to_string()],
                raw: "hi".to_string(),
            },
        );
        assert_eq!(result.expect("execute"), "hi");

        runtime.register_hook("noop", HookEventKind::SessionStarted, |_ctx| {
            HookOutcome::Continue
        });
        let ctx = HookContext {
            session_id: "s1".to_string(),
            seq: 0,
            timestamp_ms: 0,
            event: HookEventKind::SessionStarted,
            output: None,
        };
        assert_eq!(runtime.hooks().run(&ctx), HookOutcome::Continue);
    }
}
