//! Frame copy-to-clipboard support.
//!
//! Handles copying the currently-selected frame's JSON to the
//! system clipboard. OSC 52 is preferred — it works over SSH and
//! doesn't touch the local OS clipboard API — but large payloads
//! fall back to an in-app buffer (user can still paste via the
//! palette's `copy last error breadcrumb` action) because many
//! terminals drop OSC 52 sequences past ~10 KB.

use std::io;
use std::io::Write;

use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use rip_tui::canvas::{Block as CanvasBlock, CachedText, CanvasMessage};
use rip_tui::TuiState;

pub(super) const OSC52_MAX_BYTES: usize = 10_000;

pub(super) fn copy_selected(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut TuiState,
) -> anyhow::Result<()> {
    let action = prepare_copy_selected(state);
    let CopySelectedAction::Osc52(payload) = action else {
        return Ok(());
    };

    let seq = osc52_sequence(payload.as_bytes());
    terminal.backend_mut().write_all(seq.as_bytes())?;
    terminal.backend_mut().flush()?;

    state.clipboard_buffer = None;
    state.set_status_message("clipboard: osc52");
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CopySelectedAction {
    None,
    Store,
    Osc52(String),
}

pub(super) fn prepare_copy_selected(state: &mut TuiState) -> CopySelectedAction {
    let payload = if let Some(event) = state.selected_event() {
        match serde_json::to_string_pretty(event) {
            Ok(json) => json,
            Err(_) => {
                state.set_status_message("clipboard: failed to serialize frame");
                return CopySelectedAction::None;
            }
        }
    } else if let Some(message) = preferred_copyable_message(state) {
        message
    } else {
        state.set_status_message("clipboard: nothing copyable selected");
        return CopySelectedAction::None;
    };

    let osc52_disabled = std::env::var_os("RIP_TUI_DISABLE_OSC52").is_some();
    if osc52_disabled || payload.len() > OSC52_MAX_BYTES {
        state.clipboard_buffer = Some(payload);
        if osc52_disabled {
            state.set_status_message("clipboard: stored (OSC52 disabled)");
        } else {
            state.set_status_message("clipboard: stored (too large for OSC52)");
        }
        return CopySelectedAction::Store;
    }

    CopySelectedAction::Osc52(payload)
}

fn preferred_copyable_message(state: &TuiState) -> Option<String> {
    state
        .focused_message()
        .and_then(copyable_message_text)
        .or_else(|| {
            state
                .canvas
                .messages
                .iter()
                .rev()
                .find_map(copyable_message_text)
        })
}

fn copyable_message_text(message: &CanvasMessage) -> Option<String> {
    let text = match message {
        CanvasMessage::UserTurn { blocks, .. } => blocks_to_text(blocks),
        CanvasMessage::AgentTurn {
            blocks,
            streaming_tail,
            ..
        } => {
            let mut text = blocks_to_text(blocks);
            if !streaming_tail.trim().is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(streaming_tail);
            }
            text
        }
        CanvasMessage::ToolCard {
            tool_name,
            args_block,
            body,
            ..
        } => {
            let mut text = format!("tool: {tool_name}");
            let args = block_to_text(args_block);
            if !args.trim().is_empty() {
                text.push_str("\n\nargs:\n");
                text.push_str(args.trim_end());
            }
            let body_text = blocks_to_text(body);
            if !body_text.trim().is_empty() {
                text.push_str("\n\n");
                text.push_str(body_text.trim_end());
            }
            text
        }
        CanvasMessage::TaskCard { title, body, .. } => {
            let mut text = title.clone().unwrap_or_else(|| "task".to_string());
            let body_text = blocks_to_text(body);
            if !body_text.trim().is_empty() {
                text.push_str("\n\n");
                text.push_str(body_text.trim_end());
            }
            text
        }
        CanvasMessage::JobNotice { job_kind, .. } => job_kind.clone(),
        CanvasMessage::SystemNotice { text, .. } => text.clone(),
        CanvasMessage::ContextNotice {
            strategy, status, ..
        } => {
            format!("context {strategy} · {status:?}")
        }
        CanvasMessage::CompactionCheckpoint {
            from_seq, to_seq, ..
        } => format!("compaction checkpoint · seq {from_seq}…{to_seq}"),
        CanvasMessage::ExtensionPanel { title, .. } => title.clone(),
    };

    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn blocks_to_text(blocks: &[CanvasBlock]) -> String {
    let mut parts = Vec::new();
    for block in blocks {
        let text = block_to_text(block);
        let trimmed = text.trim_end();
        if !trimmed.is_empty() {
            parts.push(trimmed.to_string());
        }
    }
    parts.join("\n\n")
}

fn block_to_text(block: &CanvasBlock) -> String {
    match block {
        CanvasBlock::Paragraph(text)
        | CanvasBlock::Markdown(text)
        | CanvasBlock::ToolArgsJson(text)
        | CanvasBlock::ToolStdout(text)
        | CanvasBlock::ToolStderr(text) => cached_text_to_string(text),
        CanvasBlock::Heading { level, text } => {
            format!(
                "{} {}",
                "#".repeat((*level).clamp(1, 6) as usize),
                cached_text_to_string(text)
            )
        }
        CanvasBlock::CodeFence { lang, text } => {
            let mut out = match lang {
                Some(lang) if !lang.is_empty() => format!("```{lang}\n"),
                _ => "```\n".to_string(),
            };
            out.push_str(&cached_text_to_string(text));
            out.push_str("\n```");
            out
        }
        CanvasBlock::BlockQuote(inner) => blocks_to_text(inner)
            .lines()
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("\n"),
        CanvasBlock::List { ordered, items } => items
            .iter()
            .enumerate()
            .map(|(idx, item)| {
                let marker = if *ordered {
                    format!("{}. ", idx + 1)
                } else {
                    "- ".to_string()
                };
                let text = blocks_to_text(item);
                if let Some((first, rest)) = text.split_once('\n') {
                    format!("{marker}{first}\n{}", rest)
                } else {
                    format!("{marker}{text}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        CanvasBlock::Thematic => "────".to_string(),
        CanvasBlock::ArtifactChip { artifact_id, .. } => {
            let short: String = artifact_id.chars().take(8).collect();
            format!("⧉ {short}")
        }
    }
}

fn cached_text_to_string(text: &CachedText) -> String {
    let mut out = String::new();
    for (idx, line) in text.text.lines.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        for span in &line.spans {
            out.push_str(span.content.as_ref());
        }
    }
    out
}

pub(super) fn osc52_sequence(bytes: &[u8]) -> String {
    let encoded = base64_encode(bytes);
    format!("\x1b]52;c;{encoded}\x07")
}

pub(super) fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity((bytes.len().saturating_add(2) / 3) * 4);
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push(TABLE[(n & 0x3f) as usize] as char);
        i += 3;
    }

    match bytes.len().saturating_sub(i) {
        0 => {}
        1 => {
            let n = (bytes[i] as u32) << 16;
            out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
            out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
            out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => unreachable!("len mod 3 is always 0..=2"),
    }

    out
}

#[cfg(test)]
mod tests;
