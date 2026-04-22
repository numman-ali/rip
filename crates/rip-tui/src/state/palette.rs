//! Palette state and entry types.
//!
//! The palette is the TUI's primary control plane: the same overlay
//! drives command search, model switching, go-to navigation, the
//! thread switcher, and quick-toggle options. Each of those flavors
//! is a `PaletteMode` backed by a `PaletteSource` implementation that
//! builds the entries; `PaletteState` is the runtime snapshot the
//! overlay renders (mode + origin + query + selection + entries).
//!
//! Selection is filtered on the fly: `filtered_indices` runs the
//! query over entry fields (value/title/subtitle/chips) with simple
//! AND-of-lowercased-terms matching, and `selected_entry` returns the
//! currently-highlighted entry from the filtered list. Modes that
//! allow free-form input (e.g. Models typed routes) opt in via
//! `allow_custom_value`; when the query matches nothing, the typed
//! query surfaces as a single custom candidate.

/// Which palette flavor is currently mounted. The `Option` variant is
/// named for the toggle category ("Options") — not Rust's `Option`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMode {
    Command,
    Navigation,
    Model,
    Session,
    Option,
}

impl PaletteMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Command => "Command",
            Self::Navigation => "Navigation",
            Self::Model => "Models",
            Self::Session => "Sessions",
            Self::Option => "Options",
        }
    }
}

/// Spatial origin for the palette overlay (C.6). The driver picks a
/// zone based on how the palette was summoned: top-center for the
/// default `⌃K`, top-right when anchored to the hero model chip, etc.
/// The renderer translates the origin into a concrete `Rect` inside
/// the current frame; nothing persists across sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteOrigin {
    TopCenter,
    TopRight,
    TopLeft,
    Center,
    BottomCenter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteEntry {
    pub value: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub chips: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteState {
    pub mode: PaletteMode,
    pub origin: PaletteOrigin,
    pub query: String,
    pub selected: usize,
    pub entries: Vec<PaletteEntry>,
    pub empty_message: String,
    pub allow_custom_value: bool,
    pub custom_prompt: String,
}

impl PaletteState {
    pub fn new(
        mode: PaletteMode,
        origin: PaletteOrigin,
        entries: Vec<PaletteEntry>,
        empty_message: String,
        allow_custom_value: bool,
        custom_prompt: String,
    ) -> Self {
        Self {
            mode,
            origin,
            query: String::new(),
            selected: 0,
            entries,
            empty_message,
            allow_custom_value,
            custom_prompt,
        }
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        let query = self.query.trim();
        if query.is_empty() {
            return (0..self.entries.len()).collect();
        }

        let terms = query
            .split_whitespace()
            .map(|term| term.to_ascii_lowercase())
            .collect::<Vec<_>>();

        self.entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| entry_matches(entry, &terms).then_some(idx))
            .collect()
    }

    pub fn selected_entry(&self) -> Option<&PaletteEntry> {
        let indices = self.filtered_indices();
        let idx = *indices.get(self.selected)?;
        self.entries.get(idx)
    }

    pub fn custom_candidate(&self) -> Option<&str> {
        let query = self.query.trim();
        if !self.allow_custom_value || query.is_empty() {
            return None;
        }
        self.filtered_indices().is_empty().then_some(query)
    }

    pub(super) fn clamp_selected(&mut self) {
        let len = self.filtered_indices().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    pub(super) fn move_selection(&mut self, delta: i32) {
        let len = self.filtered_indices().len();
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

fn entry_matches(entry: &PaletteEntry, terms: &[String]) -> bool {
    let mut haystack = String::new();
    haystack.push_str(&entry.value);
    haystack.push('\n');
    haystack.push_str(&entry.title);
    if let Some(subtitle) = entry.subtitle.as_deref() {
        haystack.push('\n');
        haystack.push_str(subtitle);
    }
    for chip in &entry.chips {
        haystack.push('\n');
        haystack.push_str(chip);
    }

    let haystack = haystack.to_ascii_lowercase();
    terms.iter().all(|term| haystack.contains(term))
}
