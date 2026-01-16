use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

#[derive(Debug)]
pub struct Runtime;

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self
    }

    pub fn start_session(&self, input: String) -> Session {
        Session::new(input)
    }
}

#[derive(Debug)]
pub struct Session {
    id: String,
    input: String,
    seq: u64,
    stage: Stage,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Stage {
    Start,
    Output,
    End,
    Done,
}

impl Session {
    pub fn new(input: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            input,
            seq: 0,
            stage: Stage::Start,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn next_event(&mut self) -> Option<Event> {
        let kind = match self.stage {
            Stage::Start => {
                self.stage = Stage::Output;
                EventKind::SessionStarted
            }
            Stage::Output => {
                self.stage = Stage::End;
                EventKind::Output {
                    content: format!("ack: {}", self.input),
                }
            }
            Stage::End => {
                self.stage = Stage::Done;
                EventKind::SessionEnded {
                    reason: "completed".to_string(),
                }
            }
            Stage::Done => return None,
        };

        let event = Event {
            id: Uuid::new_v4().to_string(),
            session_id: self.id.clone(),
            timestamp_ms: now_ms(),
            seq: self.seq,
            kind,
        };
        self.seq += 1;
        Some(event)
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
}
