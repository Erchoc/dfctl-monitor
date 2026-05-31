use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod tui;

#[derive(Parser)]
#[command(name = "dfctl", version, about = "dfctl — application metrics in your terminal")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// View live application metrics
    Monitor(commands::monitor::MonitorArgs),
    /// Inspect a distributed trace's call chain and summary
    Trace(commands::trace::TraceArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Monitor(args) => commands::monitor::run(args),
        Command::Trace(args) => commands::trace::run(args),
    }
}
