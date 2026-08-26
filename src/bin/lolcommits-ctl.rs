use clap::{CommandFactory, Parser, Subcommand, ValueHint};
use owo_colors::OwoColorize;
use std::path::PathBuf;

use sw1nn_lolcommits_rs::{
    capture, config,
    error::{Error, Result},
};

#[derive(Parser, Debug)]
#[command(name = "lolcommits-ctl")]
#[command(about = "Control the lolcommits webcam snapshot client")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Take a snapshot with your webcam and upload it
    Upload(UploadArgs),

    /// Generate shell completions for the given shell
    Completions {
        #[arg(value_name = "SHELL")]
        shell: clap_complete::Shell,
    },
}

#[derive(clap::Args, Debug)]
struct UploadArgs {
    #[arg(
        default_value = "HEAD",
        help = "The commit revision (any git revision parameter)"
    )]
    revision: String,

    #[arg(long, action = clap::ArgAction::SetTrue, help = "Force upload even if SHA already exists")]
    force: bool,

    #[arg(long, short, action = clap::ArgAction::SetTrue, help = "Suppress camera busy errors (exit 0 instead)")]
    quiet: bool,

    #[arg(long, value_name = "FILE", help = "Path to config file", value_hint = ValueHint::FilePath)]
    config: Option<PathBuf>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("off")),
        )
        .init();

    match Cli::parse().command {
        Commands::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "lolcommits-ctl",
                &mut std::io::stdout(),
            );
            Ok(())
        }
        Commands::Upload(args) => upload(args),
    }
}

fn upload(args: UploadArgs) -> Result<()> {
    let config = config::Config::load_from(args.config)?;
    tracing::debug!(?config, "Loaded configuration");

    let server_url = config
        .client
        .as_ref()
        .map(|c| c.server_url.clone())
        .unwrap_or_else(|| "server".to_owned());

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

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn upload_defaults_revision_to_head() -> TestResult {
        let cli = Cli::try_parse_from(["lolcommits-ctl", "upload"])?;
        let Commands::Upload(args) = cli.command else {
            panic!("expected upload command");
        };
        assert_eq!(args.revision, "HEAD");
        assert!(!args.force);
        assert!(!args.quiet);
        assert!(args.config.is_none());
        Ok(())
    }

    #[test]
    fn upload_accepts_revision_and_flags() -> TestResult {
        let cli = Cli::try_parse_from([
            "lolcommits-ctl",
            "upload",
            "HEAD~1",
            "--force",
            "--quiet",
            "--config",
            "/tmp/config.toml",
        ])?;
        let Commands::Upload(args) = cli.command else {
            panic!("expected upload command");
        };
        assert_eq!(args.revision, "HEAD~1");
        assert!(args.force);
        assert!(args.quiet);
        assert_eq!(args.config, Some(PathBuf::from("/tmp/config.toml")));
        Ok(())
    }

    #[test]
    fn completions_parses_shell() -> TestResult {
        let cli = Cli::try_parse_from(["lolcommits-ctl", "completions", "zsh"])?;
        let Commands::Completions { shell } = cli.command else {
            panic!("expected completions command");
        };
        assert_eq!(shell, clap_complete::Shell::Zsh);
        Ok(())
    }

    #[test]
    fn bare_invocation_is_rejected() {
        assert!(Cli::try_parse_from(["lolcommits-ctl"]).is_err());
    }
}
