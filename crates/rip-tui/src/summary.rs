use rip_kernel::{Event, EventKind, ProviderEventStatus};

pub fn event_type(event: &Event) -> &'static str {
    match &event.kind {
        EventKind::SessionStarted { .. } => "session_started",
        EventKind::OutputTextDelta { .. } => "output_text_delta",
        EventKind::SessionEnded { .. } => "session_ended",
        EventKind::ToolStarted { .. } => "tool_started",
        EventKind::ToolStdout { .. } => "tool_stdout",
        EventKind::ToolStderr { .. } => "tool_stderr",
        EventKind::ToolEnded { .. } => "tool_ended",
        EventKind::ToolFailed { .. } => "tool_failed",
        EventKind::ProviderEvent { .. } => "provider_event",
        EventKind::CheckpointCreated { .. } => "checkpoint_created",
        EventKind::CheckpointRewound { .. } => "checkpoint_rewound",
        EventKind::CheckpointFailed { .. } => "checkpoint_failed",
    }
}

pub fn event_summary(event: &Event) -> String {
    match &event.kind {
        EventKind::SessionStarted { input } => format!("{:?}", truncate(input, 64)),
        EventKind::OutputTextDelta { delta } => format!("{:?}", truncate(delta, 64)),
        EventKind::SessionEnded { reason } => format!("{:?}", truncate(reason, 64)),
        EventKind::ToolStarted { name, .. } => name.to_string(),
        EventKind::ToolStdout { chunk, .. } | EventKind::ToolStderr { chunk, .. } => {
            format!("{:?}", truncate(chunk, 64))
        }
        EventKind::ToolEnded { exit_code, .. } => format!("exit={exit_code}"),
        EventKind::ToolFailed { error, .. } => format!("{:?}", truncate(error, 64)),
        EventKind::ProviderEvent {
            status,
            event_name,
            errors,
            response_errors,
            ..
        } => match status {
            ProviderEventStatus::Event => event_name.as_deref().unwrap_or("event").to_string(),
            ProviderEventStatus::Done => "done".to_string(),
            ProviderEventStatus::InvalidJson => {
                if !errors.is_empty() || !response_errors.is_empty() {
                    format!(
                        "invalid_json ({})",
                        errors.len().saturating_add(response_errors.len())
                    )
                } else {
                    "invalid_json".to_string()
                }
            }
        },
        EventKind::CheckpointCreated { label, .. } | EventKind::CheckpointRewound { label, .. } => {
            format!("{:?}", truncate(label, 64))
        }
        EventKind::CheckpointFailed { error, .. } => format!("{:?}", truncate(error, 64)),
    }
}

fn truncate(input: &str, max_len: usize) -> String {
    if input.chars().count() <= max_len {
        return input.to_string();
    }
    input.chars().take(max_len).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;
    use rip_kernel::{CheckpointAction, Event, EventKind, ProviderEventStatus};

    fn make_event(kind: EventKind) -> Event {
        Event {
            id: "e1".to_string(),
            session_id: "s1".to_string(),
            timestamp_ms: 0,
            seq: 0,
            kind,
        }
    }

    #[test]
    fn event_type_maps_variants() {
        let cases = [
            (
                EventKind::SessionStarted {
                    input: "hi".to_string(),
                },
                "session_started",
            ),
            (
                EventKind::OutputTextDelta {
                    delta: "hi".to_string(),
                },
                "output_text_delta",
            ),
            (
                EventKind::SessionEnded {
                    reason: "done".to_string(),
                },
                "session_ended",
            ),
            (
                EventKind::ToolStarted {
                    tool_id: "t1".to_string(),
                    name: "ls".to_string(),
                    args: serde_json::json!({}),
                    timeout_ms: None,
                },
                "tool_started",
            ),
            (
                EventKind::ToolStdout {
                    tool_id: "t1".to_string(),
                    chunk: "out".to_string(),
                },
                "tool_stdout",
            ),
            (
                EventKind::ToolStderr {
                    tool_id: "t1".to_string(),
                    chunk: "err".to_string(),
                },
                "tool_stderr",
            ),
            (
                EventKind::ToolEnded {
                    tool_id: "t1".to_string(),
                    exit_code: 0,
                    duration_ms: 1,
                    artifacts: None,
                },
                "tool_ended",
            ),
            (
                EventKind::ToolFailed {
                    tool_id: "t1".to_string(),
                    error: "fail".to_string(),
                },
                "tool_failed",
            ),
            (
                EventKind::ProviderEvent {
                    provider: "openresponses".to_string(),
                    status: ProviderEventStatus::Event,
                    event_name: Some("response.output_text.delta".to_string()),
                    data: None,
                    raw: None,
                    errors: vec![],
                    response_errors: vec![],
                },
                "provider_event",
            ),
            (
                EventKind::CheckpointCreated {
                    checkpoint_id: "c1".to_string(),
                    label: "snap".to_string(),
                    created_at_ms: 1,
                    files: vec![],
                    auto: false,
                    tool_name: None,
                },
                "checkpoint_created",
            ),
            (
                EventKind::CheckpointRewound {
                    checkpoint_id: "c1".to_string(),
                    label: "snap".to_string(),
                    files: vec![],
                },
                "checkpoint_rewound",
            ),
            (
                EventKind::CheckpointFailed {
                    action: CheckpointAction::Create,
                    error: "nope".to_string(),
                },
                "checkpoint_failed",
            ),
        ];

        for (kind, expected) in cases {
            let event = make_event(kind);
            assert_eq!(event_type(&event), expected);
        }
    }

    #[test]
    fn event_summary_formats_provider_event_statuses() {
        let event = make_event(EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Event,
            event_name: None,
            data: None,
            raw: None,
            errors: vec![],
            response_errors: vec![],
        });
        assert_eq!(event_summary(&event), "event");

        let event = make_event(EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Done,
            event_name: Some("response.completed".to_string()),
            data: None,
            raw: None,
            errors: vec![],
            response_errors: vec![],
        });
        assert_eq!(event_summary(&event), "done");

        let event = make_event(EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::InvalidJson,
            event_name: None,
            data: None,
            raw: None,
            errors: vec!["bad json".to_string()],
            response_errors: vec!["schema".to_string()],
        });
        assert_eq!(event_summary(&event), "invalid_json (2)");
    }

    #[test]
    fn event_summary_truncates_long_values() {
        let long = "a".repeat(70);
        let event = make_event(EventKind::SessionStarted { input: long });
        let summary = event_summary(&event);
        assert!(summary.starts_with("\""));
        assert!(summary.ends_with("…\""));
        assert_eq!(summary.chars().count(), 67);
    }

    #[test]
    fn event_summary_handles_additional_variants() {
        let event = make_event(EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::Event,
            event_name: Some("response.output_text.delta".to_string()),
            data: None,
            raw: None,
            errors: vec![],
            response_errors: vec![],
        });
        assert_eq!(event_summary(&event), "response.output_text.delta");

        let event = make_event(EventKind::ProviderEvent {
            provider: "openresponses".to_string(),
            status: ProviderEventStatus::InvalidJson,
            event_name: None,
            data: None,
            raw: None,
            errors: vec![],
            response_errors: vec![],
        });
        assert_eq!(event_summary(&event), "invalid_json");

        let event = make_event(EventKind::ToolStarted {
            tool_id: "t1".to_string(),
            name: "ls".to_string(),
            args: serde_json::json!({}),
            timeout_ms: None,
        });
        assert_eq!(event_summary(&event), "ls");

        let event = make_event(EventKind::ToolEnded {
            tool_id: "t1".to_string(),
            exit_code: 42,
            duration_ms: 10,
            artifacts: None,
        });
        assert_eq!(event_summary(&event), "exit=42");

        let event = make_event(EventKind::CheckpointCreated {
            checkpoint_id: "c1".to_string(),
            label: "snap".to_string(),
            created_at_ms: 1,
            files: vec![],
            auto: false,
            tool_name: None,
        });
        assert!(event_summary(&event).contains("snap"));
    }

    #[test]
    fn truncate_returns_input_when_short() {
        assert_eq!(truncate("short", 10), "short".to_string());
    }
}
