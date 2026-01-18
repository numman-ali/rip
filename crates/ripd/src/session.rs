use std::path::PathBuf;
use std::sync::Arc;

use rip_kernel::{Event, Runtime};
use rip_log::{write_snapshot, EventLog};
use rip_tools::{ToolInvocation, ToolRunner};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::{broadcast, Mutex};

#[derive(Deserialize)]
struct ToolCommand {
    tool: String,
    #[serde(default)]
    args: Value,
    timeout_ms: Option<u64>,
}

#[derive(Deserialize)]
struct CheckpointEnvelope {
    checkpoint: CheckpointCommand,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum CheckpointCommand {
    Create { label: String, files: Vec<String> },
    Rewind { id: String },
}

enum InputAction {
    Tool(ToolCommand),
    Checkpoint(CheckpointCommand),
    Prompt,
}

pub struct SessionContext {
    pub runtime: Arc<Runtime>,
    pub tool_runner: Arc<ToolRunner>,
    pub sender: broadcast::Sender<Event>,
    pub events: Arc<Mutex<Vec<Event>>>,
    pub event_log: Arc<EventLog>,
    pub snapshot_dir: Arc<PathBuf>,
    pub server_session_id: String,
    pub input: String,
}

pub async fn run_session(context: SessionContext) {
    let SessionContext {
        runtime,
        tool_runner,
        sender,
        events,
        event_log,
        snapshot_dir,
        server_session_id,
        input,
    } = context;
    let mut session = runtime.start_session(input.clone());
    let action = parse_action(&input);
    let runtime_session_id = session.id().to_string();

    if let Some(event) = session.next_event() {
        emit_event(event, &sender, &events, &event_log).await;
    }

    match action {
        InputAction::Tool(command) => {
            let mut seq = session.seq();
            let tool_events = tool_runner
                .run(
                    &runtime_session_id,
                    &mut seq,
                    ToolInvocation {
                        name: command.tool,
                        args: command.args,
                        timeout_ms: command.timeout_ms,
                    },
                )
                .await;
            session.set_seq(seq);
            emit_events(tool_events, &sender, &events, &event_log).await;
        }
        InputAction::Checkpoint(command) => {
            let mut seq = session.seq();
            let checkpoint_events = match command {
                CheckpointCommand::Create { label, files } => tool_runner.create_checkpoint(
                    &runtime_session_id,
                    &mut seq,
                    label,
                    files.into_iter().map(PathBuf::from).collect(),
                ),
                CheckpointCommand::Rewind { id } => {
                    tool_runner.rewind_checkpoint(&runtime_session_id, &mut seq, &id)
                }
            };
            session.set_seq(seq);
            emit_events(checkpoint_events, &sender, &events, &event_log).await;
        }
        InputAction::Prompt => {}
    }

    while let Some(event) = session.next_event() {
        emit_event(event, &sender, &events, &event_log).await;
    }

    let guard = events.lock().await;
    let _ = write_snapshot(&*snapshot_dir, &server_session_id, &guard);
}

fn parse_action(input: &str) -> InputAction {
    let trimmed = input.trim();
    if trimmed.starts_with('{') {
        if let Ok(envelope) = serde_json::from_str::<CheckpointEnvelope>(trimmed) {
            return InputAction::Checkpoint(envelope.checkpoint);
        }
        if let Ok(command) = serde_json::from_str::<ToolCommand>(trimmed) {
            return InputAction::Tool(command);
        }
    }

    InputAction::Prompt
}

async fn emit_events(
    events: Vec<Event>,
    sender: &broadcast::Sender<Event>,
    buffer: &Arc<Mutex<Vec<Event>>>,
    event_log: &EventLog,
) {
    for event in events {
        emit_event(event, sender, buffer, event_log).await;
    }
}

async fn emit_event(
    event: Event,
    sender: &broadcast::Sender<Event>,
    buffer: &Arc<Mutex<Vec<Event>>>,
    event_log: &EventLog,
) {
    let _ = sender.send(event.clone());
    let mut guard = buffer.lock().await;
    guard.push(event.clone());
    let _ = event_log.append(&event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_action_accepts_tool_command() {
        let input = r#"{"tool":"write","args":{"path":"a.txt","content":"hi"}}"#;
        match parse_action(input) {
            InputAction::Tool(command) => {
                assert_eq!(command.tool, "write");
                assert!(command.args.get("path").is_some());
            }
            _ => panic!("expected tool action"),
        }
    }

    #[test]
    fn parse_action_accepts_checkpoint_command() {
        let input = r#"{"checkpoint":{"action":"create","label":"snap","files":["a.txt"]}}"#;
        match parse_action(input) {
            InputAction::Checkpoint(CheckpointCommand::Create { label, files }) => {
                assert_eq!(label, "snap");
                assert_eq!(files, vec!["a.txt".to_string()]);
            }
            _ => panic!("expected checkpoint create"),
        }
    }

    #[test]
    fn parse_action_defaults_to_prompt() {
        match parse_action("hello") {
            InputAction::Prompt => {}
            _ => panic!("expected prompt"),
        }
    }
}
