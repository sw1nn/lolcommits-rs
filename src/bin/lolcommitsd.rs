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

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    tracing::info!("Server running on http://127.0.0.1:3000");

    axum::serve(listener, app).await?;

    Ok(())
}
