//! Thread picker overlay state (D.3).
//!
//! The richer cousin of the palette `Threads` mode: one entry per
//! known continuity, with a thread-local title, a short preview of the
//! last agent message, and status chips (e.g. current, archived, age).
//! The driver opens it from `⌃T` and applies the selected continuity
//! id as the next-run target without inventing new thread semantics.
//! Unlike the palette, there is no query filter — every listed thread
//! is shown; navigation is pure arrow / page keys.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadPickerEntry {
    pub thread_id: String,
    pub title: String,
    pub preview: String,
    pub chips: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadPickerState {
    pub entries: Vec<ThreadPickerEntry>,
    pub selected: usize,
}

impl ThreadPickerState {
    pub fn new(entries: Vec<ThreadPickerEntry>) -> Self {
        Self {
            entries,
            selected: 0,
        }
    }

    pub fn selected_entry(&self) -> Option<&ThreadPickerEntry> {
        self.entries.get(self.selected)
    }

    pub(super) fn move_selection(&mut self, delta: i32) {
        let len = self.entries.len();
        if len == 0 {
            self.selected = 0;
            return;
        }

        if delta < 0 {
            self.selected = self.selected.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.selected = self.selected.saturating_add(delta as usize).min(len - 1);
        }
    }
}
