use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use sw1nn_lolcommits_rs::{LogOutput, auth, config, init_tracing_with_output, server};

#[derive(Parser, Debug)]
#[command(name = "lolcommitsd")]
#[command(about = "Lolcommits server daemon")]
#[command(version)]
struct Args {
    #[arg(long, value_name = "FILE", help = "Path to config file")]
    config: Option<PathBuf>,

    #[arg(long, value_enum, help = "Log output destination (overrides config)")]
    log: Option<LogOutput>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Load config first to get log_output setting
    let cfg = config::Config::load_from(args.config)?;
    let server_cfg = cfg.server.clone().unwrap_or_default();

    // CLI --log overrides config log_output
    let log_output = args.log.unwrap_or(server_cfg.log_output);
    init_tracing_with_output(log_output);
    let metrics_handle = sw1nn_lolcommits_rs::metrics::install_recorder();

    tracing::info!("Starting lolcommitsd({})", env!("CARGO_PKG_VERSION"));
    tracing::info!(config = ?cfg, "Parsed config");

    let images_dir = PathBuf::from(&server_cfg.images_dir);

    let static_root = server_cfg.static_root();
    if static_root.is_dir() {
        tracing::info!(static_root = %static_root.display(), "Serving gallery assets");
    } else {
        tracing::warn!(
            static_root = %static_root.display(),
            "Gallery asset directory not found; the gallery will return 404 until it is installed"
        );
    }

    let auth_cfg = cfg.auth.clone().unwrap_or_default();
    tracing::info!(
        issuer = %auth_cfg.issuer,
        client_id = %auth_cfg.client_id,
        required_group = %auth_cfg.required_group,
        "Upload authentication configured"
    );
    let authenticator = Arc::new(auth::Authenticator::new(auth_cfg).await);

    let app = server::create_router(
        images_dir,
        static_root,
        metrics_handle,
        authenticator,
        server_cfg.max_concurrent_uploads,
    );

    let bind_addr = format!("{}:{}", server_cfg.bind_address, server_cfg.bind_port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(address = %bind_addr, "Server running");

    axum::serve(listener, app).await?;

    Ok(())
}
