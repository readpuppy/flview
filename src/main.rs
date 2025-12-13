mod fsutil;
mod hex;
mod tui;
mod web;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "flview")]
#[command(about = "Terminal + web file viewer", long_about = None)]
struct Cli {
    /// Root directory to browse (defaults to current directory)
    #[arg(long)]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the terminal UI
    Tui,

    /// Run the web server
    Web {
        /// Address to bind (e.g. 127.0.0.1:8080)
        #[arg(long, default_value = "127.0.0.1:8080")]
        addr: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let root = match cli.root {
        Some(p) => p,
        None => std::env::current_dir()?,
    };

    match cli.command {
        Command::Tui => {
            tui::run(root)?;
        }
        Command::Web { addr } => {
            web::run(root, addr).await?;
        }
    }

    Ok(())
}
