mod fsutil;
mod hex;
mod site;
mod tui;
mod web;

use std::net::Ipv4Addr;
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

    /// Fetch a URL and print TLS + response headers
    Site {
        /// Target URL (http/https)
        url: String,

        /// Allow insecure server connections when using SSL (skip cert validation)
        #[arg(short = 'k', long)]
        insecure: bool,

        /// Additional request header (repeatable), e.g. -H 'X-Foo: bar'
        #[arg(short = 'H', action = clap::ArgAction::Append)]
        header: Vec<String>,

        /// Request body data (implies POST), similar to curl -d
        #[arg(short = 'd')]
        data: Option<String>,

        /// Override DNS resolution and connect to this IPv4 address instead
        #[arg(long, value_name = "IPV4")]
        resolveto: Option<Ipv4Addr>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Tui => {
            let root = match cli.root {
                Some(p) => p,
                None => std::env::current_dir()?,
            };
            tui::run(root)?;
        }
        Command::Web { addr } => {
            let root = match cli.root {
                Some(p) => p,
                None => std::env::current_dir()?,
            };
            web::run(root, addr).await?;
        }
        Command::Site {
            url,
            insecure,
            header,
            data,
            resolveto,
        } => {
            site::run(url, insecure, header, data, resolveto).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_site_args() {
        let cli = Cli::try_parse_from([
            "flview",
            "site",
            "https://example.com/",
            "-k",
            "-H",
            "X-Test: 1",
            "-H",
            "X-Other: two",
            "-d",
            "payload",
            "--resolveto",
            "1.2.3.4",
        ])
        .unwrap();

        match cli.command {
            Command::Site {
                url,
                insecure,
                header,
                data,
                resolveto,
            } => {
                assert_eq!(url, "https://example.com/");
                assert!(insecure);
                assert_eq!(header, vec!["X-Test: 1", "X-Other: two"]);
                assert_eq!(data.as_deref(), Some("payload"));
                assert_eq!(resolveto, Some(Ipv4Addr::new(1, 2, 3, 4)));
            }
            _ => panic!("expected site command"),
        }
    }
}
