use sw1nn_lolcommits_rs::server;
use xdg::BaseDirectories;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let xdg_dirs = BaseDirectories::with_prefix("lolcommits")?;
    let data_home = xdg_dirs.get_data_home();

    tracing::info!(path = %data_home.display(), "Serving images from");

    let app = server::create_router(data_home);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    tracing::info!("Server running on http://127.0.0.1:3000");

    axum::serve(listener, app).await?;

    Ok(())
}
