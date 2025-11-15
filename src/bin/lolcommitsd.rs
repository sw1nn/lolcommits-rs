use axum::{
    response::{Html, IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sw1nn_lolcommits_rs::image_metadata;
use tower_http::{
    services::ServeDir,
    trace::{DefaultMakeSpan, TraceLayer},
};
use xdg::BaseDirectories;

#[derive(Debug, Serialize, Deserialize)]
struct ImageInfo {
    filename: String,
    repo_name: String,
    timestamp: String,
    commit_sha: String,
    commit_message: String,
    commit_type: String,
    commit_scope: String,
    branch_name: String,
    diff_stats: String,
    date_time: Option<String>,
}

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

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/api/images", get(list_images))
        .nest_service("/images", ServeDir::new(&data_home))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    tracing::info!("Server running on http://127.0.0.1:3000");

    axum::serve(listener, app).await?;

    Ok(())
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn list_images() -> Response {
    match get_image_list() {
        Ok(images) => Json(images).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to list images");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list images: {}", e),
            )
                .into_response()
        }
    }
}

fn get_image_list() -> Result<Vec<ImageInfo>, Box<dyn std::error::Error>> {
    let xdg_dirs = BaseDirectories::with_prefix("lolcommits")?;
    let data_home = xdg_dirs.get_data_home();

    let mut images = Vec::new();

    for entry in std::fs::read_dir(&data_home)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("png") {
            if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                if let Some(info) = parse_filename(&path, filename) {
                    images.push(info);
                }
            }
        }
    }

    // Sort by timestamp descending (newest first)
    images.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(images)
}

fn parse_filename(path: &std::path::Path, filename: &str) -> Option<ImageInfo> {
    // Try to read metadata from PNG file first
    if let Ok(Some(metadata)) = image_metadata::read_png_metadata(path) {
        tracing::debug!(filename, "Read metadata from PNG");
        return Some(ImageInfo {
            filename: filename.to_string(),
            repo_name: metadata.repo_name,
            timestamp: metadata.timestamp.clone(),
            commit_sha: metadata.commit_sha,
            commit_message: metadata.commit_message,
            commit_type: metadata.commit_type,
            commit_scope: metadata.commit_scope,
            branch_name: metadata.branch_name,
            diff_stats: metadata.diff_stats,
            date_time: Some(metadata.timestamp),
        });
    }

    // Fallback: parse filename for old images without metadata
    // Expected format: {repo_name}-{timestamp}-{commit_sha}.png
    // timestamp format: %Y%m%d-%H%M%S
    tracing::debug!(filename, "Falling back to filename parsing");
    let name = filename.strip_suffix(".png")?;
    let parts: Vec<&str> = name.rsplitn(3, '-').collect();

    if parts.len() != 3 {
        return None;
    }

    let commit_sha = parts[0].to_string();
    let time_part = parts[1];
    let repo_name = parts[2].to_string();

    // Parse timestamp for display
    let timestamp = format!("{}-{}", repo_name, time_part);
    let date_time = parse_timestamp(time_part);

    Some(ImageInfo {
        filename: filename.to_string(),
        repo_name,
        timestamp: timestamp.clone(),
        commit_sha,
        commit_message: String::new(),
        commit_type: String::new(),
        commit_scope: String::new(),
        branch_name: String::new(),
        diff_stats: String::new(),
        date_time,
    })
}

fn parse_timestamp(timestamp: &str) -> Option<String> {
    // Format: YYYYMMDD-HHMMSS
    if timestamp.len() != 15 {
        return None;
    }

    let year = timestamp.get(0..4)?;
    let month = timestamp.get(4..6)?;
    let day = timestamp.get(6..8)?;
    let hour = timestamp.get(9..11)?;
    let minute = timestamp.get(11..13)?;
    let second = timestamp.get(13..15)?;

    let datetime_str = format!(
        "{}-{}-{} {}:{}:{}",
        year, month, day, hour, minute, second
    );

    Some(datetime_str)
}
