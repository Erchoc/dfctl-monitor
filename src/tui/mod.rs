//! Shared TUI primitives used by every `dfctl` subcommand (monitor, trace, …).
//!
//! These modules deliberately have *no* dependency on any command's data model
//! so they can be reused freely. Command-specific widgets live under each
//! command's own `widgets/` directory.

pub mod braille;
pub mod colors;
pub mod health;
pub mod kpi_card;
