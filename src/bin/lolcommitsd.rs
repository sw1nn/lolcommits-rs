use clap::Parser;
use sw1nn_lolcommits_rs::{config, server};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "lolcommitsd")]
#[command(about = "Lolcommits server daemon")]
struct Args {
    #[arg(long, value_name = "FILE", help = "Path to config file")]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let cfg = config::Config::load_from(args.config)?;
    let images_dir = PathBuf::from(&cfg.server.images_dir);

    tracing::info!(path = %images_dir.display(), "Serving images from");

    let app = server::create_router(images_dir);

    let bind_addr = format!("{}:{}", cfg.server.bind_address, cfg.server.bind_port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(address = %bind_addr, "Server running");

    axum::serve(listener, app).await?;

    Ok(())
}
