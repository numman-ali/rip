mod commands;
mod hooks;

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub use commands::{Command, CommandContext, CommandHandler, CommandRegistry, CommandResult};
pub use hooks::{Hook, HookContext, HookEngine, HookEventKind, HookHandler, HookOutcome};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub session_id: String,
    pub timestamp_ms: u64,
    pub seq: u64,
    #[serde(flatten)]
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    SessionStarted,
    Output { content: String },
    SessionEnded { reason: String },
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

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn next_event(&mut self) -> Option<Event> {
        let (next_stage, kind) = match self.stage {
            Stage::Start => (Stage::Output, EventKind::SessionStarted),
            Stage::Output => (
                Stage::End,
                EventKind::Output {
                    content: format!("ack: {}", self.input),
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

        let hook_event = match &event.kind {
            EventKind::SessionStarted => HookEventKind::SessionStarted,
            EventKind::Output { .. } => HookEventKind::Output,
            EventKind::SessionEnded { .. } => HookEventKind::SessionEnded,
        };

        let output = match &event.kind {
            EventKind::Output { content } => Some(content.clone()),
            _ => None,
        };

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

        matches!(events[0].kind, EventKind::SessionStarted);
        matches!(events[1].kind, EventKind::Output { .. });
        matches!(events[2].kind, EventKind::SessionEnded { .. });
    }

    #[test]
    fn event_serializes_to_json() {
        let runtime = Runtime::new();
        let mut session = runtime.start_session("test".to_string());
        let event = session.next_event().expect("event");
        let json = serde_json::to_string(&event).expect("json");
        assert!(json.contains("session_started"));
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
        matches!(events[0].kind, EventKind::SessionStarted);
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
}
