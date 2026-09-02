use clap::{CommandFactory, Parser, Subcommand, ValueHint};
use owo_colors::OwoColorize;
use std::path::PathBuf;

use sw1nn_lolcommits_rs::{
    capture, config,
    error::{Error, Result},
    oidc, token_store,
};

/// Timeout for the OIDC endpoints. Generous: the token endpoint is polled while
/// the user is off approving the login in a browser.
const OIDC_TIMEOUT_SECS: u64 = 30;

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

    /// Log in to the identity provider and store the credentials
    Login(AuthArgs),

    /// Forget the stored credentials
    Logout(AuthArgs),

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

#[derive(clap::Args, Debug)]
struct AuthArgs {
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
        Commands::Login(args) => login(args),
        Commands::Logout(args) => logout(args),
    }
}

fn login(args: AuthArgs) -> Result<()> {
    let config = config::Config::load_from(args.config)?;
    let auth = config.auth.unwrap_or_default();

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(OIDC_TIMEOUT_SECS))
        .build()?;

    let authorization = oidc::device_authorization(&client, &auth)?;
    let url = authorization.verification_url();

    println!();
    println!("  Your code is {}", authorization.user_code.bold().green());
    println!("  Approve it at {}", url.magenta());
    println!();

    open_browser(url);

    println!("Waiting for approval...");
    let tokens = oidc::poll_for_token(&client, &auth, &authorization)?;
    let backend = token_store::save(&auth, &tokens)?;

    // Falling back to a file is expected on a headless host, but it also
    // happens when a desktop keyring is merely locked, so say so rather than
    // writing a refresh token to disk quietly.
    if let token_store::StoreBackend::File(_) = backend {
        println!(
            "{} Secret Service unavailable; credentials written to a 0600 file",
            "!".yellow()
        );
    }

    println!(
        "{} Logged in, credentials stored in {}",
        "✓".green(),
        backend.to_string().magenta()
    );
    Ok(())
}

fn logout(args: AuthArgs) -> Result<()> {
    let config = config::Config::load_from(args.config)?;
    let auth = config.auth.unwrap_or_default();

    // A store that refused the delete must not be reported as a logout.
    token_store::clear(&auth).inspect_err(|error| {
        eprintln!(
            "{} Could not clear stored credentials: {}",
            "✗".red(),
            error.to_string().red()
        );
    })?;

    println!("{} Logged out", "✓".green());
    Ok(())
}

/// Best effort: headless hosts have no browser, and the URL is printed anyway.
fn open_browser(url: &str) {
    tracing::debug!(url, "Opening browser");

    let _ = std::process::Command::new("xdg-open")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .inspect_err(|error| tracing::debug!(%error, "Could not launch xdg-open"));
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
        Err(Error::NotLoggedIn) => {
            // Seen mid-`git commit`, so it says exactly what to run next.
            eprintln!("error: not logged in or session expired");
            eprintln!("       run: lolcommits-ctl login");
            Err(Error::NotLoggedIn)
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
    fn login_accepts_optional_config() -> TestResult {
        let cli = Cli::try_parse_from(["lolcommits-ctl", "login"])?;
        let Commands::Login(args) = cli.command else {
            panic!("expected login command");
        };
        assert!(args.config.is_none());

        let cli = Cli::try_parse_from(["lolcommits-ctl", "login", "--config", "/tmp/config.toml"])?;
        let Commands::Login(args) = cli.command else {
            panic!("expected login command");
        };
        assert_eq!(args.config, Some(PathBuf::from("/tmp/config.toml")));
        Ok(())
    }

    #[test]
    fn logout_accepts_optional_config() -> TestResult {
        let cli = Cli::try_parse_from(["lolcommits-ctl", "logout"])?;
        let Commands::Logout(args) = cli.command else {
            panic!("expected logout command");
        };
        assert!(args.config.is_none());

        let cli =
            Cli::try_parse_from(["lolcommits-ctl", "logout", "--config", "/tmp/config.toml"])?;
        let Commands::Logout(args) = cli.command else {
            panic!("expected logout command");
        };
        assert_eq!(args.config, Some(PathBuf::from("/tmp/config.toml")));
        Ok(())
    }

    #[test]
    fn login_rejects_positional_arguments() {
        assert!(Cli::try_parse_from(["lolcommits-ctl", "login", "extra"]).is_err());
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
