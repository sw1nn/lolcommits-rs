use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, State},
    http::StatusCode,
    response::{
        Html, IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashSet;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tower_http::{
    services::ServeDir,
    trace::{DefaultMakeSpan, TraceLayer},
};

use crate::{config, error::Result, git, image_metadata, image_processor};

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
    revision: String,
    message: String,
    commit_type: String,
    scope: String,
    timestamp: String,
    repo_name: String,
    branch_name: String,
    files_changed: u32,
    insertions: u32,
    deletions: u32,
    #[serde(default)]
    force: bool,
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
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
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
        state.serialize_field("revision", &self.0.revision)?;
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

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<String>,
    revision_cache: Arc<RwLock<HashSet<String>>>,
}

pub fn create_router(data_home: std::path::PathBuf) -> Router {
    // Create broadcast channel for SSE events (capacity of 100 events)
    let (tx, _rx) = broadcast::channel(100);

    // Initialize revision cache from existing images
    let revision_cache = match initialize_revision_cache() {
        Ok(cache) => {
            tracing::info!(count = cache.len(), "Initialized revision cache");
            Arc::new(RwLock::new(cache))
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to initialize revision cache, starting with empty cache");
            Arc::new(RwLock::new(HashSet::new()))
        }
    };

    let state = AppState { tx, revision_cache };
    Router::new()
        .route("/", get(index_handler))
        .route("/api/images", get(list_images))
        .route("/api/config", get(get_config))
        .route("/api/upload", post(upload_handler))
        .route("/api/events", get(sse_handler))
        .nest_service("/images", ServeDir::new(&data_home))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024)) // 4 MiB
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
        .with_state(state)
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("static/index.html"))
}

async fn list_images() -> Response {
    match config::Config::load() {
        Ok(config) => {
            let server_config = config.server.clone().unwrap_or_default();
            match get_image_list(&server_config) {
                Ok(images) => {
                    let responses: Vec<ImageMetadata> =
                        images.into_iter().map(ImageMetadata).collect();
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
            }
        }
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
        Ok(cfg) => {
            let gallery_title = cfg
                .server
                .as_ref()
                .map(|s| s.gallery_title.clone())
                .unwrap_or_else(|| "Lolcommits Gallery".to_string());
            Json(ConfigResponse { gallery_title }).into_response()
        }
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

async fn sse_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    let rx = state.tx.subscribe();

    let stream = async_stream::stream! {
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    yield Ok(Event::default().data(msg));
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(skipped = n, "SSE client lagged, skipped messages");
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keepalive"),
    )
}

fn initialize_revision_cache() -> Result<HashSet<String>> {
    let config = config::Config::load()?;
    let server_config = config.server.clone().unwrap_or_default();
    let images = get_image_list(&server_config)?;
    Ok(images.into_iter().map(|img| img.revision).collect())
}

fn get_image_list(config: &config::ServerConfig) -> Result<Vec<git::CommitMetadata>> {
    let images_dir = PathBuf::from(&config.images_dir);

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

async fn upload_handler(State(state): State<AppState>, mut multipart: Multipart) -> Response {
    let mut image_bytes: Option<Vec<u8>> = None;
    let mut metadata: Option<UploadMetadata> = None;

    // Parse multipart form
    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().map(|s| s.to_string()).unwrap_or_default();
        tracing::debug!(field_name = %name, "Received field");

        match name.as_str() {
            "image" => match field.bytes().await {
                Ok(bytes) => {
                    tracing::debug!(size = bytes.len(), "Received image");
                    image_bytes = Some(bytes.to_vec());
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to read image bytes");
                }
            },
            "metadata" => match field.bytes().await {
                Ok(bytes) => {
                    if let Ok(text) = String::from_utf8(bytes.to_vec())
                        && let Ok(parsed) = serde_json::from_str::<UploadMetadata>(&text)
                    {
                        tracing::debug!(?parsed, "Received metadata");
                        metadata = Some(parsed);
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Failed to read metadata bytes");
                }
            },
            _ => {
                tracing::debug!(field_name = %name, "Ignoring unknown field");
            }
        }
    }

    let Some(image_bytes) = image_bytes else {
        return (StatusCode::BAD_REQUEST, "Missing image field").into_response();
    };

    let Some(metadata) = metadata else {
        return (StatusCode::BAD_REQUEST, "Missing metadata field").into_response();
    };

    tracing::info!(
        revision = %metadata.revision,
        repo = %metadata.repo_name,
        "Received upload, spawning async processor"
    );

    // Spawn async processing task
    let tx = state.tx.clone();
    let revision_cache = state.revision_cache.clone();
    tokio::spawn(async move {
        if let Err(e) = process_image_async(image_bytes, metadata, tx, revision_cache).await {
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
    tx: broadcast::Sender<String>,
    revision_cache: Arc<RwLock<HashSet<String>>>,
) -> Result<()> {
    tracing::info!(revision = %metadata.revision, force = metadata.force, "Starting async image processing");

    // Load config
    let config = config::Config::load()?;

    // Check if revision already exists (unless force flag is set)
    if !metadata.force {
        let cache = revision_cache.read().await;
        if cache.contains(&metadata.revision) {
            tracing::info!(revision = %metadata.revision, "Revision already exists, skipping upload");
            return Ok(());
        }
    }

    // Decode image
    let image = image::load_from_memory(&image_bytes)?;
    tracing::debug!("Decoded image");

    // Get server config for processing
    let server_config = config.server.clone().unwrap_or_default();

    // Background replacement
    let processed_image = image_processor::replace_background(&server_config, image)?;
    tracing::info!("Background replaced");

    // Create commit metadata
    let commit_metadata = git::CommitMetadata {
        path: PathBuf::new(),
        revision: metadata.revision.clone(),
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

    // Apply chyron if enabled in server config
    let final_image = if server_config.burned_in_chyron {
        let chyron_config = config.burned_in_chyron.clone().unwrap_or_default();
        let image_with_chyron =
            image_processor::burn_in_chyron(&chyron_config, processed_image, &commit_metadata)?;
        tracing::debug!("Burned in chyron");
        image_with_chyron
    } else {
        tracing::debug!("Chyron disabled");
        processed_image
    };

    // Get output path
    let output_path = get_output_path(&server_config, &metadata.repo_name, &metadata.revision)?;

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
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    tracing::info!(path = %output_path.display(), "Saved lolcommit with metadata");

    // Add revision to cache
    {
        let mut cache = revision_cache.write().await;
        cache.insert(metadata.revision.clone());
        tracing::debug!(revision = %metadata.revision, "Added revision to cache");
    }

    // Broadcast new image event to SSE clients
    let _ = tx.send("new_image".to_string());
    tracing::debug!("Broadcasted new_image event to SSE clients");

    Ok(())
}

fn get_output_path(
    config: &config::ServerConfig,
    repo_name: &str,
    commit_sha: &str,
) -> Result<PathBuf> {
    let images_dir = PathBuf::from(&config.images_dir);

    // Ensure directory exists
    std::fs::create_dir_all(&images_dir)?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{}-{}-{}.png", repo_name, timestamp, commit_sha);

    let output_path = images_dir.join(filename);

    Ok(output_path)
}
