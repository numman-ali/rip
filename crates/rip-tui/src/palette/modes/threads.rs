//! Threads palette mode (Phase C.5).
//!
//! Lists the current continuity + any recent continuities the driver
//! has pre-fetched via the `thread.list` capability. Per the revamp
//! plan (Parts 16 + 17), thread *creation* / *rename* do not have
//! capabilities in the registry today — those entries are carried by
//! the Command mode and ride as `[deferred]`. This mode ships only
//! with **switching** to an existing continuity, which routes through
//! `thread.get` on apply.
//!
//! Lightweight `ThreadSummary` struct lives here so the mode stays
//! pure `rip-tui` (no ripd dependency). The driver converts whatever
//! `thread.list` returns into `ThreadSummary` before seeding the
//! mode. When `thread.list` isn't available yet (local runtime) the
//! mode gracefully degrades to just the current thread.

use crate::PaletteEntry;

use super::super::PaletteSource;

#[derive(Debug, Clone, Default)]
pub struct ThreadSummary {
    pub thread_id: String,
    pub title: Option<String>,
    pub last_message_preview: Option<String>,
    pub updated_at_ms: Option<u64>,
    pub is_current: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ThreadsMode {
    pub threads: Vec<ThreadSummary>,
}

impl ThreadsMode {
    pub fn new(threads: Vec<ThreadSummary>) -> Self {
        Self { threads }
    }
}

impl PaletteSource for ThreadsMode {
    fn id(&self) -> &'static str {
        "threads"
    }

    fn label(&self) -> &str {
        "Threads"
    }

    fn placeholder(&self) -> &str {
        "switch thread"
    }

    fn entries(&self) -> Vec<PaletteEntry> {
        // Always surface the current thread first (pinned), then the
        // rest in descending `updated_at_ms` order so recency bubbles
        // up without a separate "Recents" bucket.
        let mut ordered = self.threads.clone();
        ordered.sort_by(|a, b| match (a.is_current, b.is_current) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b.updated_at_ms.cmp(&a.updated_at_ms),
        });

        ordered
            .into_iter()
            .map(|thread| {
                let title = thread
                    .title
                    .clone()
                    .unwrap_or_else(|| short_id(&thread.thread_id));
                let subtitle = thread
                    .last_message_preview
                    .clone()
                    .unwrap_or_else(|| "—".to_string());
                let mut chips = Vec::new();
                if thread.is_current {
                    chips.push("current".to_string());
                }
                PaletteEntry {
                    value: thread.thread_id,
                    title,
                    subtitle: Some(subtitle),
                    chips,
                }
            })
            .collect()
    }

    fn empty_state(&self) -> &str {
        "no threads yet"
    }
}

fn short_id(id: &str) -> String {
    if id.len() <= 12 {
        id.to_string()
    } else {
        format!("{}…", &id[..12])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_current_thread_first() {
        let mode = ThreadsMode::new(vec![
            ThreadSummary {
                thread_id: "t-older".to_string(),
                title: Some("older".to_string()),
                last_message_preview: None,
                updated_at_ms: Some(100),
                is_current: false,
            },
            ThreadSummary {
                thread_id: "t-current".to_string(),
                title: Some("current".to_string()),
                last_message_preview: Some("hi".to_string()),
                updated_at_ms: Some(50),
                is_current: true,
            },
            ThreadSummary {
                thread_id: "t-newer".to_string(),
                title: None,
                last_message_preview: None,
                updated_at_ms: Some(200),
                is_current: false,
            },
        ]);

        let entries = mode.entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].value, "t-current");
        assert!(entries[0].chips.iter().any(|c| c == "current"));
        // Non-current entries sorted newest first.
        assert_eq!(entries[1].value, "t-newer");
        assert_eq!(entries[2].value, "t-older");
    }
}
