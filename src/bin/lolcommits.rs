use clap::Parser;
use owo_colors::OwoColorize;
use std::path::PathBuf;

use sw1nn_lolcommits_rs::{capture, config, error::{Error, Result}};

#[derive(Parser, Debug)]
#[command(name = "lolcommits")]
#[command(about = "Take a snapshot with your webcam when you commit")]
struct Args {
    #[arg(help = "The commit SHA")]
    sha: String,

    #[arg(long, action = clap::ArgAction::SetTrue, help = "Enable chyron overlay (overrides config)")]
    chyron: bool,

    #[arg(long, action = clap::ArgAction::SetTrue, help = "Disable chyron overlay (overrides config)")]
    no_chyron: bool,

    #[arg(long, value_name = "FILE", help = "Path to config file")]
    config: Option<PathBuf>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off")),
        )
        .init();

    let args = Args::parse();

    // Load configuration
    let config = config::Config::load_from(args.config)?;
    tracing::debug!(?config, "Loaded configuration");

    let server_url = config.client.server_url.clone();

    let capture_args = capture::CaptureArgs {
        sha: args.sha,
        chyron: args.chyron,
        no_chyron: args.no_chyron,
    };

    if !tracing::enabled!(tracing::Level::INFO) {
        println!("📸 Capturing lolcommit...");
    }

    match capture::capture_lolcommit(capture_args, config) {
        Ok(()) => {
            if !tracing::enabled!(tracing::Level::INFO) {
                println!("{} Lolcommit uploaded successfully to {}", "✓".green(), server_url.magenta());
            }
            Ok(())
        }
        Err(Error::ServerConnectionFailed { url, source }) => {
            if !tracing::enabled!(tracing::Level::INFO) {
                eprintln!("{} Failed to connect to lolcommitsd at {}: {}", "✗".red(), url.magenta(), source.to_string().red());
            }
            Err(Error::ServerConnectionFailed { url, source })
        }
        Err(e) => Err(e),
    }
}
