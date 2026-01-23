use std::collections::HashMap;

use rip_kernel::{Event, EventKind};
use rip_log::EventLog;

use crate::context_bundle::{
    ContextBundleCompilerV1, ContextBundleItemV1, ContextBundleProvenanceV1, ContextBundleSourceV1,
    ContextBundleV1,
};

pub(crate) const CONTEXT_COMPILER_ID_V1: &str = "rip.context_compiler.v1";
pub(crate) const CONTEXT_COMPILER_STRATEGY_RECENT_MESSAGES_V1: &str = "recent_messages_v1";

// Kernel v1: hard cap on raw message turns included (assistant replies are derived per-message).
const RECENT_MESSAGES_V1_LIMIT: usize = 16;

pub(crate) struct CompileRecentMessagesV1Request<'a> {
    pub(crate) continuity_id: &'a str,
    pub(crate) continuity_events: &'a [Event],
    pub(crate) event_log: &'a EventLog,
    pub(crate) from_seq: u64,
    pub(crate) from_message_id: Option<String>,
    pub(crate) run_session_id: &'a str,
    pub(crate) actor_id: &'a str,
    pub(crate) origin: &'a str,
}

pub(crate) fn compile_recent_messages_v1(
    req: CompileRecentMessagesV1Request<'_>,
) -> Result<ContextBundleV1, String> {
    let ended_runs_by_message_id = ended_runs_by_message_id(req.continuity_events, req.from_seq);
    let selected = select_recent_messages(
        req.continuity_events,
        req.from_seq,
        RECENT_MESSAGES_V1_LIMIT,
    );

    let mut items = Vec::new();
    for message in selected {
        items.push(ContextBundleItemV1::Message {
            role: "user".to_string(),
            content: message.content.clone(),
            actor_id: Some(message.actor_id.clone()),
            origin: Some(message.origin.clone()),
            thread_seq: Some(message.seq),
            thread_event_id: Some(message.event_id.clone()),
        });

        if let Some(ended_session_id) = ended_runs_by_message_id.get(&message.event_id) {
            let assistant_text = aggregate_session_output_text(req.event_log, ended_session_id);
            if !assistant_text.is_empty() {
                items.push(ContextBundleItemV1::Message {
                    role: "assistant".to_string(),
                    content: assistant_text,
                    actor_id: None,
                    origin: None,
                    thread_seq: None,
                    thread_event_id: None,
                });
            }
        }
    }

    Ok(ContextBundleV1::new(
        ContextBundleCompilerV1 {
            id: CONTEXT_COMPILER_ID_V1.to_string(),
            strategy: CONTEXT_COMPILER_STRATEGY_RECENT_MESSAGES_V1.to_string(),
        },
        ContextBundleSourceV1 {
            thread_id: req.continuity_id.to_string(),
            from_seq: req.from_seq,
            from_message_id: req.from_message_id,
        },
        ContextBundleProvenanceV1 {
            run_session_id: req.run_session_id.to_string(),
            actor_id: req.actor_id.to_string(),
            origin: req.origin.to_string(),
        },
        items,
    ))
}

#[derive(Debug, Clone)]
struct SelectedMessage {
    seq: u64,
    event_id: String,
    actor_id: String,
    origin: String,
    content: String,
}

fn select_recent_messages(
    continuity_events: &[Event],
    from_seq: u64,
    limit: usize,
) -> Vec<SelectedMessage> {
    let mut selected_rev: Vec<SelectedMessage> = Vec::new();
    if limit == 0 {
        return Vec::new();
    }

    for event in continuity_events.iter().rev() {
        if event.seq > from_seq {
            continue;
        }
        let EventKind::ContinuityMessageAppended {
            actor_id,
            origin,
            content,
        } = &event.kind
        else {
            continue;
        };

        selected_rev.push(SelectedMessage {
            seq: event.seq,
            event_id: event.id.clone(),
            actor_id: actor_id.clone(),
            origin: origin.clone(),
            content: content.clone(),
        });
        if selected_rev.len() >= limit {
            break;
        }
    }

    selected_rev.reverse();
    selected_rev
}

fn ended_runs_by_message_id(continuity_events: &[Event], from_seq: u64) -> HashMap<String, String> {
    let mut ended: HashMap<String, String> = HashMap::new();
    for event in continuity_events {
        if event.seq > from_seq {
            break;
        }

        if let EventKind::ContinuityRunEnded {
            run_session_id,
            message_id,
            ..
        } = &event.kind
        {
            ended.insert(message_id.clone(), run_session_id.clone());
        }
    }
    ended
}

fn aggregate_session_output_text(event_log: &EventLog, session_id: &str) -> String {
    let Ok(events) = event_log.replay_session(session_id) else {
        return String::new();
    };

    let mut out = String::new();
    for event in events {
        if let EventKind::OutputTextDelta { delta } = event.kind {
            out.push_str(&delta);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rip_kernel::StreamKind;
    use tempfile::tempdir;

    #[test]
    fn compile_recent_messages_v1_includes_user_and_assistant_when_run_ended_with_output() {
        let dir = tempdir().expect("tmp");
        let log = EventLog::new(dir.path().join("events.jsonl")).expect("log");

        // Session stream with assistant output.
        let session_id = "s1";
        log.append(&Event {
            id: "e0".to_string(),
            session_id: session_id.to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind: EventKind::SessionStarted {
                input: "hi".to_string(),
            },
        })
        .expect("append");
        log.append(&Event {
            id: "e1".to_string(),
            session_id: session_id.to_string(),
            timestamp_ms: 1,
            seq: 1,
            kind: EventKind::OutputTextDelta {
                delta: "hello".to_string(),
            },
        })
        .expect("append");
        log.append(&Event {
            id: "e2".to_string(),
            session_id: session_id.to_string(),
            timestamp_ms: 2,
            seq: 2,
            kind: EventKind::SessionEnded {
                reason: "completed".to_string(),
            },
        })
        .expect("append");

        // Continuity stream: one message, and run ended pointing at the session.
        let thread_id = "t1";
        let message_id = "m1";
        let continuity_events = vec![
            Event {
                id: message_id.to_string(),
                session_id: thread_id.to_string(),
                timestamp_ms: 0,
                seq: 0,
                kind: EventKind::ContinuityMessageAppended {
                    actor_id: "alice".to_string(),
                    origin: "cli".to_string(),
                    content: "hi".to_string(),
                },
            },
            Event {
                id: "r1".to_string(),
                session_id: thread_id.to_string(),
                timestamp_ms: 1,
                seq: 1,
                kind: EventKind::ContinuityRunEnded {
                    run_session_id: session_id.to_string(),
                    message_id: message_id.to_string(),
                    reason: "completed".to_string(),
                    actor_id: Some("alice".to_string()),
                    origin: Some("cli".to_string()),
                },
            },
        ];

        // Sanity: stream_kind classification is correct for test fixtures.
        assert_eq!(continuity_events[0].stream_kind(), StreamKind::Continuity);

        let bundle = compile_recent_messages_v1(CompileRecentMessagesV1Request {
            continuity_id: thread_id,
            continuity_events: &continuity_events,
            event_log: &log,
            from_seq: 1,
            from_message_id: Some(message_id.to_string()),
            run_session_id: "run_1",
            actor_id: "alice",
            origin: "cli",
        })
        .expect("compile");

        let json = serde_json::to_value(&bundle).expect("json");
        let items = json.get("items").and_then(|v| v.as_array()).expect("items");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].get("role").and_then(|v| v.as_str()), Some("user"));
        assert_eq!(
            items[1].get("role").and_then(|v| v.as_str()),
            Some("assistant")
        );
        assert_eq!(
            items[1].get("content").and_then(|v| v.as_str()),
            Some("hello")
        );
    }
}
