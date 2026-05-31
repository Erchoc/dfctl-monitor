//! Braille pixel canvas — moved to the shared `crate::tui::braille` module so
//! `trace` and other subcommands can draw with the same primitive. Re-exported
//! here to keep monitor's `render::braille::Canvas` call-sites unchanged.
pub use crate::tui::braille::*;
