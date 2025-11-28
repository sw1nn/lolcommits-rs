pub mod camera;
pub mod capture;
pub mod config;
pub mod error;
pub mod git;
pub mod image_metadata;
pub mod image_processor;
pub mod segmentation;
pub mod server;

use std::io::IsTerminal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Log output destination
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    clap::ValueEnum,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum LogOutput {
    /// Automatically detect based on terminal availability
    #[default]
    Auto,
    /// Force stdout output
    Stdout,
    /// Force journald output
    Journald,
}

/// Initialize tracing with optional output override.
/// Uses journald when running as a service (no terminal), fmt when running interactively.
pub fn init_tracing_with_output(output: LogOutput) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "lolcommits=info,tower_http=warn".into());

    let use_stdout = match output {
        LogOutput::Auto => std::io::stdout().is_terminal(),
        LogOutput::Stdout => true,
        LogOutput::Journald => false,
    };

    if use_stdout {
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_journald::layer().expect("Failed to connect to journald"))
            .init();
    }
}

/// Uses journald when running as a service (no terminal), fmt when running interactively
pub fn init_tracing() {
    init_tracing_with_output(LogOutput::Auto);
}
