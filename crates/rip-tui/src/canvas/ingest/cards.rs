//! Tool + task card ingestion.
//!
//! `ToolCard` handlers mirror the sync `tool_*` frame triplet
//! (Started / Stdout|Stderr / Ended|Failed). `TaskCard` handlers mirror
//! the background `tool_task_*` frames. Both card types fold contributed
//! artifact ids via `merge_artifact_ids` — deduped, stable order —
//! so the renderer's artifact chip rail matches the kernel's truth.

use rip_kernel::ToolTaskStatus;
use serde_json::Value;

use super::super::model::*;
use super::super::CanvasModel;
use super::ToolStream;

pub(super) fn push_tool_card(
    canvas: &mut CanvasModel,
    event: &rip_kernel::Event,
    tool_id: &str,
    name: &str,
    args: &Value,
) {
    let id = canvas.mint_id();
    let args_text = match serde_json::to_string_pretty(args) {
        Ok(pretty) => pretty,
        Err(_) => args.to_string(),
    };
    canvas.messages.push(CanvasMessage::ToolCard {
        message_id: id,
        tool_id: tool_id.to_string(),
        tool_name: name.to_string(),
        args_block: Block::ToolArgsJson(CachedText::plain(&args_text)),
        status: ToolCardStatus::Running,
        body: Vec::new(),
        expanded: false,
        artifact_ids: Vec::new(),
        started_seq: event.seq,
        started_at_ms: event.timestamp_ms,
    });
}

pub(super) fn append_tool_body(
    canvas: &mut CanvasModel,
    tool_id: &str,
    stream: ToolStream,
    chunk: &str,
) {
    if chunk.is_empty() {
        return;
    }
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::ToolCard {
            tool_id: id, body, ..
        } = message
        {
            if id == tool_id {
                let block = match stream {
                    ToolStream::Stdout => Block::ToolStdout(CachedText::plain(chunk)),
                    ToolStream::Stderr => Block::ToolStderr(CachedText::plain(chunk)),
                };
                body.push(block);
                return;
            }
        }
    }
}

pub(super) fn finalize_tool_card_success(
    canvas: &mut CanvasModel,
    tool_id: &str,
    exit_code: i32,
    duration_ms: u64,
    artifacts: Option<&Value>,
) {
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::ToolCard {
            tool_id: id,
            status,
            artifact_ids,
            ..
        } = message
        {
            if id == tool_id {
                *status = ToolCardStatus::Succeeded {
                    duration_ms,
                    exit_code,
                };
                if let Some(value) = artifacts {
                    merge_artifact_ids(artifact_ids, value);
                }
                return;
            }
        }
    }
}

pub(super) fn finalize_tool_card_success_at(
    canvas: &mut CanvasModel,
    tool_id: &str,
    timestamp_ms: u64,
) {
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::ToolCard {
            tool_id: id,
            status,
            started_at_ms,
            ..
        } = message
        {
            if id == tool_id {
                *status = ToolCardStatus::Succeeded {
                    duration_ms: timestamp_ms.saturating_sub(*started_at_ms),
                    exit_code: 0,
                };
                return;
            }
        }
    }
}

pub(super) fn finalize_tool_card_failure(canvas: &mut CanvasModel, tool_id: &str, error: &str) {
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::ToolCard {
            tool_id: id,
            status,
            ..
        } = message
        {
            if id == tool_id {
                *status = ToolCardStatus::Failed {
                    error: error.to_string(),
                };
                return;
            }
        }
    }
}

pub(super) fn push_task_card(
    canvas: &mut CanvasModel,
    task_id: &str,
    tool_name: &str,
    title: Option<String>,
    execution_mode: rip_kernel::ToolTaskExecutionMode,
    artifacts: Option<&Value>,
) {
    let id = canvas.mint_id();
    let mut artifact_ids: Vec<String> = Vec::new();
    if let Some(value) = artifacts {
        merge_artifact_ids(&mut artifact_ids, value);
    }
    canvas.messages.push(CanvasMessage::TaskCard {
        message_id: id,
        task_id: task_id.to_string(),
        tool_name: tool_name.to_string(),
        title,
        execution_mode,
        status: TaskCardStatus::Queued,
        body: Vec::new(),
        expanded: false,
        artifact_ids,
        started_at_ms: None,
    });
}

pub(super) fn update_task_card_status(
    canvas: &mut CanvasModel,
    task_id: &str,
    new_status: ToolTaskStatus,
    exit_code: Option<i32>,
    new_started_at_ms: Option<u64>,
    error: Option<&str>,
    artifacts: Option<&Value>,
) {
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::TaskCard {
            task_id: id,
            status,
            artifact_ids,
            started_at_ms,
            ..
        } = message
        {
            if id == task_id {
                *status = match new_status {
                    ToolTaskStatus::Queued => TaskCardStatus::Queued,
                    ToolTaskStatus::Running => TaskCardStatus::Running,
                    ToolTaskStatus::Exited => TaskCardStatus::Exited { exit_code },
                    ToolTaskStatus::Cancelled => TaskCardStatus::Cancelled,
                    ToolTaskStatus::Failed => TaskCardStatus::Failed {
                        error: error.map(ToString::to_string),
                    },
                };
                if started_at_ms.is_none() {
                    *started_at_ms = new_started_at_ms;
                }
                if let Some(value) = artifacts {
                    merge_artifact_ids(artifact_ids, value);
                }
                return;
            }
        }
    }
}

pub(super) fn append_task_body(
    canvas: &mut CanvasModel,
    task_id: &str,
    stream: ToolStream,
    chunk: &str,
) {
    if chunk.is_empty() {
        return;
    }
    for message in canvas.messages.iter_mut().rev() {
        if let CanvasMessage::TaskCard {
            task_id: id, body, ..
        } = message
        {
            if id == task_id {
                let block = match stream {
                    ToolStream::Stdout => Block::ToolStdout(CachedText::plain(chunk)),
                    ToolStream::Stderr => Block::ToolStderr(CachedText::plain(chunk)),
                };
                body.push(block);
                return;
            }
        }
    }
}

fn merge_artifact_ids(target: &mut Vec<String>, value: &Value) {
    let mut ids = Vec::new();
    collect_artifact_ids(value, &mut ids);
    for id in ids {
        if !target.iter().any(|existing| existing == &id) {
            target.push(id);
        }
    }
}

fn collect_artifact_ids(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
        Value::String(s) if looks_like_artifact_id(s) => out.push(s.clone()),
        Value::String(_) => {}
        Value::Array(items) => {
            for item in items {
                collect_artifact_ids(item, out);
            }
        }
        Value::Object(map) => {
            for v in map.values() {
                collect_artifact_ids(v, out);
            }
        }
    }
}

fn looks_like_artifact_id(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}
