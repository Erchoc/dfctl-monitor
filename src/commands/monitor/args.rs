use chrono::{DateTime, Local};
use clap::Parser;

#[derive(Parser, Clone, Debug)]
pub struct MonitorArgs {
    /// Application name (defaults to "demo-app" for the mock backend)
    #[arg(default_value = "demo-app")]
    pub app: String,

    /// Auto-refresh
    #[arg(short, long)]
    pub watch: bool,

    /// Refresh interval (only with --watch)
    #[arg(long, default_value = "60s")]
    pub interval: humantime::Duration,

    /// Output JSON instead of TUI
    #[arg(short, long)]
    pub json: bool,

    /// Time range: relative duration (e.g., 1h, 3h, 24h)
    #[arg(long, default_value = "3h")]
    pub since: humantime::Duration,

    /// Absolute start time (overrides --since)
    #[arg(long)]
    pub from: Option<DateTime<Local>>,

    /// Absolute end time (default: now)
    #[arg(long)]
    pub to: Option<DateTime<Local>>,

    /// Filter by pod name (repeatable or comma-separated)
    #[arg(short, long, value_delimiter = ',')]
    pub pod: Vec<String>,

    /// Specific metric(s) to show in single-metric mode
    #[arg(short, long, value_delimiter = ',')]
    pub metric: Vec<String>,
}
