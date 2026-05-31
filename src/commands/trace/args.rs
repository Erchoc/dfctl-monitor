use clap::Parser;

#[derive(Parser, Clone, Debug)]
pub struct TraceArgs {
    /// Trace ID (uuid / hex). Any value works with the mock backend.
    #[arg(default_value = "7f3a91c2")]
    pub uuid: String,

    /// Output JSON (trace + computed stats) instead of the TUI
    #[arg(short, long)]
    pub json: bool,

    /// Open straight into the summary view
    #[arg(short, long)]
    pub summary: bool,

    /// Jump to the first error span on open
    #[arg(short, long)]
    pub errors: bool,

    /// Highlight/filter spans by service (repeatable or comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub service: Vec<String>,

    /// Poll for updates while the trace is still being written
    #[arg(short, long)]
    pub watch: bool,

    /// Refresh interval (only with --watch)
    #[arg(long, default_value = "5s")]
    pub interval: humantime::Duration,
}
