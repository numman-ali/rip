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
