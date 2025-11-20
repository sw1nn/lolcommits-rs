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

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "Starting lolcommitsd");

    let args = Args::parse();
    let cfg = config::Config::load_from(args.config)?;
    let images_dir = PathBuf::from(&cfg.server.images_dir);

    tracing::info!(config = ?cfg, "Parsed config");

    let app = server::create_router(images_dir);

    let bind_addr = format!("{}:{}", cfg.server.bind_address, cfg.server.bind_port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(address = %bind_addr, "Server running");

    axum::serve(listener, app).await?;

    Ok(())
}
