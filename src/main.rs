use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Monitor(args) => commands::monitor::run(args),
    }
}
