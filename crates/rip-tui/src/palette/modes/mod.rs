//! Palette modes. Phase A ships Models (migrated from rip-cli's
//! `ModelPaletteCatalog`); Phase C.5 adds Command, Go To, Threads,
//! and Options. Each mode is a plain struct implementing
//! `PaletteSource`; the driver composes entries from state and
//! routes selections back through a mode-specific apply path.

pub mod command;
pub mod go_to;
pub mod models;
pub mod options;
pub mod threads;
