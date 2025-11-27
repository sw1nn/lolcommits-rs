use clap::Parser;
use std::path::PathBuf;
use sw1nn_lolcommits_rs::{config, init_tracing, server};

#[derive(Parser, Debug)]
#[command(name = "lolcommitsd")]
#[command(about = "Lolcommits server daemon")]
#[command(version)]
struct Args {
    #[arg(long, value_name = "FILE", help = "Path to config file")]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    tracing::info!("Starting lolcommitsd({})", env!("CARGO_PKG_VERSION"));

    let args = Args::parse();
    let cfg = config::Config::load_from(args.config)?;

    tracing::info!(config = ?cfg, "Parsed config");

    let server_cfg = cfg.server.clone().unwrap_or_default();
    let images_dir = PathBuf::from(&server_cfg.images_dir);

    let app = server::create_router(images_dir);

    let bind_addr = format!("{}:{}", server_cfg.bind_address, server_cfg.bind_port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(address = %bind_addr, "Server running");

    axum::serve(listener, app).await?;

    Ok(())
}
