//! Palette engine (Phase A.4 of the TUI revamp).
//!
//! Hosts the `PaletteSource` trait — the per-mode contract that
//! supplies entries to the command palette. Phase A ships one mode
//! (Models, migrated from rip-cli's `ModelPaletteCatalog`); later
//! phases (C.5) add Command, Go To, Threads, Options.
//!
//! Naming: the existing `PaletteMode` enum is a UI tag (which mode is
//! currently open). The trait is the *source of behavior* for a mode —
//! kept under a distinct name (`PaletteSource`) so the enum can stay
//! until Phase C retires it, per the plan's "own a `Box<dyn
//! PaletteMode>`, not an enum" end-state.

use crate::PaletteEntry;

pub mod modes;

pub use modes::models::{ModelRoute, ModelsMode, ResolvedModelRoute};

/// Per-mode contract for the command palette.
///
/// Entries are produced on demand; the renderer filters them against
/// the current query. Mode-specific resolution (e.g. parsing a
/// typed-in model route) lives as concrete methods on the
/// implementing struct — the trait stays narrow until Phase C layers
/// in `PaletteCtx` + `PaletteAction` for full behavior dispatch.
pub trait PaletteSource {
    fn id(&self) -> &'static str;
    fn label(&self) -> &str;
    fn placeholder(&self) -> &str {
        ""
    }
    fn entries(&self) -> Vec<PaletteEntry>;
    fn empty_state(&self) -> &str {
        "No results"
    }
    /// When `Some`, a typed query that matches no entry is offered as
    /// a custom candidate. The returned string is the prompt (e.g.
    /// "Use typed route").
    fn allow_custom(&self) -> Option<&str> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MinimalMode;

    impl PaletteSource for MinimalMode {
        fn id(&self) -> &'static str {
            "minimal"
        }
        fn label(&self) -> &str {
            "Minimal"
        }
        fn entries(&self) -> Vec<PaletteEntry> {
            Vec::new()
        }
    }

    #[test]
    fn palette_source_default_methods_cover_unused_modes() {
        // Most shipped modes override `placeholder`, `empty_state`, and
        // `allow_custom`. The defaults exist so a new mode author can
        // land a stub without wiring every knob. Exercise them here so
        // they don't stay as dead code that skews coverage.
        let mode = MinimalMode;
        assert_eq!(mode.id(), "minimal");
        assert_eq!(mode.label(), "Minimal");
        assert!(mode.entries().is_empty());
        assert_eq!(mode.placeholder(), "");
        assert_eq!(mode.empty_state(), "No results");
        assert!(mode.allow_custom().is_none());
    }
}
