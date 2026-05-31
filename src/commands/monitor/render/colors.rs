//! Colour palette for the monitor TUI.
//!
//! The palette now lives in the shared `crate::tui::colors` module so it can be
//! reused by sibling subcommands (e.g. `trace`). This re-export keeps the many
//! `render::colors::*` call-sites in the monitor code working unchanged.
pub use crate::tui::colors::*;
