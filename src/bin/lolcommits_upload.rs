use clap::Parser;
use owo_colors::OwoColorize;
use std::path::PathBuf;

use sw1nn_lolcommits_rs::{
    capture, config,
    error::{Error, Result},
};

#[derive(Parser, Debug)]
#[command(name = "lolcommits_upload")]
#[command(about = "Take a snapshot with your webcam when you commit")]
#[command(version)]
struct Args {
    #[arg(
        default_value = "HEAD",
        help = "The commit revision (any git revision parameter)"
    )]
    revision: String,

    #[arg(long, action = clap::ArgAction::SetTrue, help = "Force upload even if SHA already exists")]
    force: bool,

    #[arg(long, short, action = clap::ArgAction::SetTrue, help = "Suppress camera busy errors (exit 0 instead)")]
    quiet: bool,

    #[arg(long, value_name = "FILE", help = "Path to config file")]
    config: Option<PathBuf>,

    /// Generate shell completions for the given shell
    #[arg(long, value_name = "SHELL")]
    completions: Option<clap_complete::Shell>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off")),
        )
        .init();

    let args = Args::parse();

    if let Some(shell) = args.completions {
        use clap::CommandFactory;
        clap_complete::generate(
            shell,
            &mut Args::command(),
            "lolcommits_upload",
            &mut std::io::stdout(),
        );
        return Ok(());
    }

    // Load configuration
    let config = config::Config::load_from(args.config)?;
    tracing::debug!(?config, "Loaded configuration");

    let server_url = config
        .client
        .as_ref()
        .map(|c| c.server_url.clone())
        .unwrap_or_else(|| "server".to_string());

    let capture_args = capture::CaptureArgs {
        revision: args.revision,
        force: args.force,
    };

    if !tracing::enabled!(tracing::Level::INFO) {
        println!("📸 Capturing lolcommit...");
    }

    match capture::capture_lolcommit(config, capture_args) {
        Ok(()) => {
            if !tracing::enabled!(tracing::Level::INFO) {
                println!(
                    "{} Lolcommit uploaded successfully to {}",
                    "✓".green(),
                    server_url.magenta()
                );
            }
            Ok(())
        }
        Err(Error::CameraBusy { device }) if args.quiet => {
            tracing::info!(device, "Camera busy, skipping lolcommit capture");
            Ok(())
        }
        Err(Error::CameraBusy { device }) => {
            eprintln!("{} Camera {} is busy", "✗".red(), device.magenta());
            Err(Error::CameraBusy { device })
        }
        Err(Error::ServerConnectionFailed { url, source }) => {
            eprintln!(
                "{} Failed to connect to lolcommitsd at {}: {}",
                "✗".red(),
                url.magenta(),
                source.to_string().red()
            );
            Err(Error::ServerConnectionFailed { url, source })
        }
        Err(Error::UploadFailed { status, body }) => {
            eprintln!(
                "{} Upload failed with status {}: {}",
                "✗".red(),
                status.to_string().yellow(),
                body.red()
            );
            Err(Error::UploadFailed { status, body })
        }
        Err(e) => {
            eprintln!("{} {}", "✗".red(), e.to_string().red());
            Err(e)
        }
    }
}
