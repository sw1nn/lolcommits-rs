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

/// Uses journald when running as a service (no terminal), fmt when running interactively
pub fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "lolcommits=info,tower_http=warn".into());

    if std::io::stdout().is_terminal() {
        // Running in a terminal, use formatted output
        tracing_subscriber::fmt().with_env_filter(env_filter).init();
    } else {
        // Running as a service, use journald
        tracing_subscriber::registry()
            .with(env_filter)
            .with(tracing_journald::layer().expect("Failed to connect to journald"))
            .init();
    }
}
