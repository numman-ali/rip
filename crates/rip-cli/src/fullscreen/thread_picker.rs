//! Thread picker overlay data loading.
//!
//! Talks to `<server>/threads` (and, if the current thread is absent
//! from the list, `<server>/threads/<id>` as a fallback) to produce
//! the `Vec<ThreadPickerEntry>` the TUI renders. Sort keeps the
//! current thread pinned to the top; the rest are newest-first. Chips
//! that need capabilities we do not yet expose (size, actors,
//! last-message preview) render as `—` here per the plan's "never
//! synthesize from disk" rule.
//!
//! The pure `build_entries` helper is extracted so tests can exercise
//! sorting + chip formatting without spinning http. Tests live beside
//! this module at `thread_picker/tests.rs`.

use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct ThreadMetaResponse {
    pub(super) thread_id: String,
    pub(super) created_at_ms: u64,
    pub(super) title: Option<String>,
    pub(super) archived: bool,
}

pub(super) async fn load_thread_picker_entries(
    client: &Client,
    server: &str,
    current_thread_id: Option<&str>,
) -> Result<Vec<rip_tui::ThreadPickerEntry>, String> {
    let url = format!("{server}/threads");
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|err| format!("thread list failed: {err}"))?;
    if !response.status().is_success() {
        return Err(format!("thread list failed: {}", response.status()));
    }

    let mut threads = response
        .json::<Vec<ThreadMetaResponse>>()
        .await
        .map_err(|err| format!("thread list parse failed: {err}"))?;

    if let Some(current_id) = current_thread_id.filter(|id| !id.is_empty()) {
        if !threads.iter().any(|thread| thread.thread_id == current_id) {
            let url = format!("{server}/threads/{current_id}");
            if let Ok(response) = client.get(url).send().await {
                if response.status().is_success() {
                    if let Ok(meta) = response.json::<ThreadMetaResponse>().await {
                        threads.push(meta);
                    }
                }
            }
        }
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);

    Ok(build_entries(threads, current_thread_id, now_ms))
}

/// Pure transform: sort threads (current pinned first, then newest
/// first) and render each into a `ThreadPickerEntry` with the right
/// chips. Split out of `load_thread_picker_entries` so tests can
/// exercise ordering + chip rendering without http.
pub(super) fn build_entries(
    mut threads: Vec<ThreadMetaResponse>,
    current_thread_id: Option<&str>,
    now_ms: u64,
) -> Vec<rip_tui::ThreadPickerEntry> {
    threads.sort_by(|a, b| {
        match (
            current_thread_id == Some(a.thread_id.as_str()),
            current_thread_id == Some(b.thread_id.as_str()),
        ) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.created_at_ms.cmp(&a.created_at_ms),
        }
    });

    threads
        .into_iter()
        .map(|thread| {
            let mut chips = vec![
                format!("age {}", relative_age_chip(now_ms, thread.created_at_ms)),
                "size —".to_string(),
                "actors —".to_string(),
            ];
            if current_thread_id == Some(thread.thread_id.as_str()) {
                chips.insert(0, "current".to_string());
            }
            if thread.archived {
                chips.push("archived".to_string());
            }
            rip_tui::ThreadPickerEntry {
                thread_id: thread.thread_id.clone(),
                title: thread
                    .title
                    .clone()
                    .unwrap_or_else(|| short_thread_label(&thread.thread_id)),
                preview: "preview —".to_string(),
                chips,
            }
        })
        .collect()
}

pub(super) fn short_thread_label(thread_id: &str) -> String {
    if thread_id.chars().count() <= 20 {
        return thread_id.to_string();
    }
    let tail: String = thread_id.chars().rev().take(12).collect();
    let tail: String = tail.chars().rev().collect();
    format!("…{tail}")
}

pub(super) fn relative_age_chip(now_ms: u64, created_at_ms: u64) -> String {
    let age_ms = now_ms.saturating_sub(created_at_ms);
    let minute = 60_000;
    let hour = 60 * minute;
    let day = 24 * hour;
    if age_ms >= day {
        format!("{}d", age_ms / day)
    } else if age_ms >= hour {
        format!("{}h", age_ms / hour)
    } else if age_ms >= minute {
        format!("{}m", age_ms / minute)
    } else {
        "now".to_string()
    }
}

#[cfg(test)]
mod tests;
