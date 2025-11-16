use clap::Parser;
use std::path::PathBuf;

use sw1nn_lolcommits_rs::{capture, config, error::Result};

#[derive(Parser, Debug)]
#[command(name = "lolcommits")]
#[command(about = "Take a snapshot with your webcam when you commit")]
struct Args {
    #[arg(help = "The commit message")]
    message: String,

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
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    // Load configuration
    let config = config::Config::load_from(args.config)?;
    tracing::debug!(?config, "Loaded configuration");

    let capture_args = capture::CaptureArgs {
        message: args.message,
        sha: args.sha,
        chyron: args.chyron,
        no_chyron: args.no_chyron,
    };

    capture::capture_lolcommit(capture_args, config)?;

    println!("Lolcommit uploaded successfully!");

    Ok(())
}
