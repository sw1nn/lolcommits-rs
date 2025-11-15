use axum::{
    extract::{DefaultBodyLimit, Multipart},
    http::StatusCode,
    Json, Router,
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize, Serializer};
use std::path::PathBuf;
use tower_http::{
    services::ServeDir,
    trace::{DefaultMakeSpan, TraceLayer},
};

use crate::{config, git, image_metadata, image_processor};

#[derive(Debug, Serialize)]
struct ConfigResponse {
    gallery_title: String,
}

#[derive(Debug, Serialize)]
struct UploadResponse {
    status: String,
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct UploadMetadata {
    sha: String,
    message: String,
    commit_type: String,
    scope: String,
    timestamp: String,
    repo_name: String,
    branch_name: String,
    files_changed: u32,
    insertions: u32,
    deletions: u32,
    enable_chyron: bool,
}

#[derive(Debug)]
pub struct ImageMetadata(git::CommitMetadata);

impl std::ops::Deref for ImageMetadata {
    type Target = git::CommitMetadata;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for ImageMetadata {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;

        let filename = self
            .as_ref()
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        let mut state = serializer.serialize_struct("ImageMetadata", 9)?;
        state.serialize_field("filename", &filename)?;
        state.serialize_field("sha", &self.0.sha)?;
        state.serialize_field("message", &self.0.message)?;
        state.serialize_field("commit_type", &self.0.commit_type)?;
        state.serialize_field("scope", &self.0.scope)?;
        state.serialize_field("timestamp", &self.0.timestamp)?;
        state.serialize_field("repo_name", &self.0.repo_name)?;
        state.serialize_field("branch_name", &self.0.branch_name)?;
        state.serialize_field("stats", &self.0.stats)?;
        state.end()
    }
}

pub fn create_router(data_home: std::path::PathBuf) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/api/images", get(list_images))
        .route("/api/config", get(get_config))
        .route("/api/upload", post(upload_handler))
        .nest_service("/images", ServeDir::new(&data_home))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024)) // 4 MiB
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("static/index.html"))
}

async fn list_images() -> Response {
    match config::Config::load() {
        Ok(config) => match get_image_list(&config) {
            Ok(images) => {
                let responses: Vec<ImageMetadata> = images.into_iter().map(ImageMetadata).collect();
                Json(responses).into_response()
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to list images");
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to list images: {}", e),
                )
                    .into_response()
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "Failed to load config");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load config: {}", e),
            )
                .into_response()
        }
    }
}

async fn get_config() -> Response {
    match config::Config::load() {
        Ok(cfg) => Json(ConfigResponse {
            gallery_title: cfg.server.gallery_title,
        })
        .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Failed to load config");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load config: {}", e),
            )
                .into_response()
        }
    }
}

fn get_image_list(config: &config::Config) -> Result<Vec<git::CommitMetadata>, Box<dyn std::error::Error>> {
    let images_dir = PathBuf::from(&config.server.images_dir);

    // Create directory if it doesn't exist
    if !images_dir.exists() {
        return Ok(Vec::new());
    }

    let mut images: Vec<git::CommitMetadata> = std::fs::read_dir(&images_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("png"))
        .filter_map(|path| image_metadata::parse_image_file(&path))
        .collect();

    // Sort by timestamp descending (newest first)
    images.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(images)
}

async fn upload_handler(mut multipart: Multipart) -> Response {
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut metadata: Option<UploadMetadata> = None;

    // Parse multipart form
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().map(|s| s.to_string()).unwrap_or_default();
        tracing::debug!(field_name = %name, "Received field");

        match name.as_str() {
            "image" => {
                match field.bytes().await {
                    Ok(bytes) => {
                        tracing::debug!(size = bytes.len(), "Received image");
                        image_bytes = Some(bytes.to_vec());
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to read image bytes");
                    }
                }
            }
            "metadata" => {
                match field.bytes().await {
                    Ok(bytes) => {
                        if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                            if let Ok(parsed) = serde_json::from_str::<UploadMetadata>(&text) {
                                tracing::debug!(?parsed, "Received metadata");
                                metadata = Some(parsed);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to read metadata bytes");
                    }
                }
            }
            _ => {
                tracing::debug!(field_name = %name, "Ignoring unknown field");
            }
        }
    }

    let Some(image_bytes) = image_bytes else {
        return (
            StatusCode::BAD_REQUEST,
            "Missing image field"
        ).into_response();
    };

    let Some(metadata) = metadata else {
        return (
            StatusCode::BAD_REQUEST,
            "Missing metadata field"
        ).into_response();
    };

    tracing::info!(
        sha = %metadata.sha,
        repo = %metadata.repo_name,
        "Received upload, spawning async processor"
    );

    // Spawn async processing task
    tokio::spawn(async move {
        if let Err(e) = process_image_async(image_bytes, metadata).await {
            tracing::error!(error = %e, "Failed to process image");
        }
    });

    // Return 202 Accepted immediately
    (
        StatusCode::ACCEPTED,
        Json(UploadResponse {
            status: "accepted".to_string(),
            message: "Processing in background".to_string(),
        }),
    )
        .into_response()
}

async fn process_image_async(
    image_bytes: Vec<u8>,
    metadata: UploadMetadata,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!(sha = %metadata.sha, "Starting async image processing");

    // Load config
    let config = config::Config::load()?;

    // Decode image
    let image = image::load_from_memory(&image_bytes)?;
    tracing::debug!("Decoded image");

    // Background replacement
    let processed_image = image_processor::replace_background(image, &config)?;
    tracing::info!("Background replaced");

    // Create commit metadata
    let commit_metadata = git::CommitMetadata {
        path: PathBuf::new(),
        sha: metadata.sha.clone(),
        message: metadata.message,
        commit_type: metadata.commit_type,
        scope: metadata.scope,
        timestamp: metadata.timestamp,
        repo_name: metadata.repo_name.clone(),
        branch_name: metadata.branch_name,
        stats: git::DiffStats {
            files_changed: metadata.files_changed,
            insertions: metadata.insertions,
            deletions: metadata.deletions,
        },
    };

    // Apply chyron if enabled
    let final_image = if metadata.enable_chyron {
        let image_with_chyron =
            image_processor::overlay_chyron(processed_image, &commit_metadata, &config)?;
        tracing::debug!("Overlaid chyron");
        image_with_chyron
    } else {
        tracing::debug!("Chyron disabled");
        processed_image
    };

    // Get output path
    let output_path = get_output_path(&config, &metadata.repo_name, &metadata.sha)?;

    // Write to temporary file first, then atomically move to final destination
    let temp_file = tempfile::NamedTempFile::new_in(
        output_path
            .parent()
            .ok_or_else(|| std::io::Error::other("Invalid output path"))?,
    )?;
    let temp_path = temp_file.path();

    tracing::debug!(temp_path = %temp_path.display(), "Writing to temporary file");
    image_metadata::save_png_with_metadata(&final_image, temp_path, &commit_metadata)?;

    // Atomically move temp file to final destination
    temp_file
        .persist(&output_path)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    tracing::info!(path = %output_path.display(), "Saved lolcommit with metadata");

    Ok(())
}

fn get_output_path(
    config: &config::Config,
    repo_name: &str,
    commit_sha: &str,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let images_dir = PathBuf::from(&config.server.images_dir);

    // Ensure directory exists
    std::fs::create_dir_all(&images_dir)?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{}-{}-{}.png", repo_name, timestamp, commit_sha);

    let output_path = images_dir.join(filename);

    Ok(output_path)
}
