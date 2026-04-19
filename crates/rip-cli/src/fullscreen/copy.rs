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
    let Some(event) = state.selected_event() else {
        state.set_status_message("clipboard: no frame selected");
        return CopySelectedAction::None;
    };

    let payload = match serde_json::to_string_pretty(event) {
        Ok(json) => json,
        Err(_) => {
            state.set_status_message("clipboard: failed to serialize frame");
            return CopySelectedAction::None;
        }
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
