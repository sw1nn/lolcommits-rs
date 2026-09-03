use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Multipart, State},
    http::{HeaderValue, StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize, Serializer};
use std::collections::HashSet;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore, broadcast};
use tower_http::{
    sensitive_headers::SetSensitiveRequestHeadersLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::{DefaultMakeSpan, TraceLayer},
};

use crate::{
    auth::{AuthenticatedUser, Authenticator},
    config,
    error::{Error, Result},
    git, image_metadata, image_processor,
};

/// Cache lifetime for `/static` assets: one year, the RFC 9111 practical
/// maximum.
const STATIC_CACHE_CONTROL: &str = "public, max-age=31536000";

struct SseConnectionGuard;

impl Drop for SseConnectionGuard {
    fn drop(&mut self) {
        crate::metrics::decrement_sse_connections();
    }
}

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
    authenticator: Arc<Authenticator>,
    upload_semaphore: Arc<Semaphore>,
}

impl axum::extract::FromRef<AppState> for Arc<Authenticator> {
    fn from_ref(state: &AppState) -> Self {
        state.authenticator.clone()
    }
}

pub fn create_router(
    data_home: std::path::PathBuf,
    static_root: std::path::PathBuf,
    metrics_handle: metrics_exporter_prometheus::PrometheusHandle,
    authenticator: Arc<Authenticator>,
    max_concurrent_uploads: usize,
) -> Router {
    // Create broadcast channel for SSE events (capacity of 100 events)
    let (tx, _rx) = broadcast::channel(100);

    // Initialize revision cache from existing images
    let (revision_cache, initial_cache_size) = match initialize_revision_cache() {
        Ok(cache) => {
            let len = cache.len();
            tracing::info!(count = len, "Initialized revision cache");
            (Arc::new(RwLock::new(cache)), len)
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to initialize revision cache, starting with empty cache");
            (Arc::new(RwLock::new(HashSet::new())), 0)
        }
    };

    crate::metrics::set_images_total(initial_cache_size);
    crate::metrics::set_revision_cache_size(initial_cache_size);

    // A value of 0 would refuse every upload, so clamp to at least one slot.
    let permits = max_concurrent_uploads.max(1);
    if permits != max_concurrent_uploads {
        tracing::warn!("max_concurrent_uploads was 0; clamping to 1");
    }
    tracing::info!(max_concurrent_uploads = permits, "Upload concurrency limit");

    let state = AppState {
        tx,
        revision_cache,
        authenticator,
        upload_semaphore: Arc::new(Semaphore::new(permits)),
    };

    let app_routes = Router::new()
        .merge(static_router(&static_root))
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
        // Layers added later wrap earlier ones, so this runs before the trace
        // layer records headers. Without it, `include_headers(true)` writes the
        // caller's access token into the span whenever tower_http logs at debug.
        .layer(SetSensitiveRequestHeadersLayer::new([
            axum::http::header::AUTHORIZATION,
        ]))
        .layer(axum::middleware::from_fn(
            crate::metrics::http_metrics_layer,
        ))
        .with_state(state);

    // Build metrics route (outside middleware so scraping doesn't inflate counts)
    let metrics_routes = Router::new().route(
        "/metrics",
        get(move || std::future::ready(metrics_handle.render())),
    );

    app_routes.merge(metrics_routes)
}

/// Serves the gallery page and its assets from `static_root` on disk.
///
/// `/static` keeps the year-long cache lifetime the embedded background image
/// had. Filenames are not versioned, so a changed asset only reaches browsers
/// that already hold one if it is given a new name. `index.html` is left
/// uncached, which is what lets such a rename take effect.
fn static_router<P, S>(static_root: P) -> Router<S>
where
    P: AsRef<Path>,
    S: Clone + Send + Sync + 'static,
{
    let static_root = static_root.as_ref();

    let assets = Router::new()
        .fallback_service(ServeDir::new(static_root))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static(STATIC_CACHE_CONTROL),
        ));

    Router::new()
        .route_service("/", ServeFile::new(static_root.join("index.html")))
        .nest_service("/static", assets)
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
        crate::metrics::increment_sse_connections();
        let _guard = SseConnectionGuard;
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

/// Reserve a slot for background image processing. The returned permit must be
/// held for the lifetime of the processing task; the slot is freed when the
/// permit is dropped. When every slot is in use, returns `Err` with the status
/// to shed the request under (429) so callers apply backpressure instead of
/// enqueuing unbounded work.
fn reserve_upload_slot(
    semaphore: &Arc<Semaphore>,
) -> std::result::Result<OwnedSemaphorePermit, StatusCode> {
    Arc::clone(semaphore).try_acquire_owned().map_err(|_| {
        tracing::warn!("Upload rejected: max concurrent uploads reached");
        crate::metrics::record_upload("overloaded");
        StatusCode::TOO_MANY_REQUESTS
    })
}

async fn upload_handler(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    mut multipart: Multipart,
) -> Response {
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

    // Reject client-controlled fields that are interpolated into the output
    // filename before doing any work, so they cannot escape images_dir.
    if let Err(e) = validate_path_component(&metadata.repo_name, "repo_name")
        .and_then(|()| validate_path_component(&metadata.revision, "revision"))
    {
        tracing::warn!(error = %e, "Rejecting upload with invalid metadata");
        crate::metrics::record_upload("rejected");
        return (StatusCode::BAD_REQUEST, "Invalid metadata").into_response();
    }

    // Reserve a processing slot before committing to any heavy work. On
    // exhaustion we shed with 429 rather than queueing unbounded background
    // tasks (each of which decodes an image and runs ONNX segmentation).
    let permit = match reserve_upload_slot(&state.upload_semaphore) {
        Ok(permit) => permit,
        Err(status) => {
            return (status, "Server busy: max concurrent uploads reached").into_response();
        }
    };

    tracing::info!(
        revision = %metadata.revision,
        repo = %metadata.repo_name,
        subject = %user.subject,
        user = %user.display_name(),
        "Received upload, spawning async processor"
    );
    crate::metrics::record_upload("accepted");

    // Spawn async processing task. The permit is moved into the task and bound
    // to `_permit` so it lives for the whole task, then drops when the future
    // ends — on success, on error return, on panic (drop runs during unwind),
    // or on task abort / runtime shutdown (the dropped future drops it). The
    // slot is therefore released in every exit path. Note: `let _permit = ...`
    // (not `let _ = ...`, which would drop it immediately).
    let tx = state.tx.clone();
    let revision_cache = state.revision_cache.clone();
    tokio::spawn(async move {
        let _permit = permit;
        if let Err(e) = process_image_async(image_bytes, metadata, tx, revision_cache).await {
            tracing::error!(error = %e, "Failed to process image");
            crate::metrics::record_upload("failed");
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
            crate::metrics::record_upload("duplicate_skipped");
            return Ok(());
        }
    }

    // Decode image
    let _timer = crate::metrics::ScopedTimer::image_processing();
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
    persist_image(temp_file, &output_path)?;
    tracing::info!(path = %output_path.display(), "Saved lolcommit with metadata");
    crate::metrics::record_upload("processed");

    // Add revision to cache
    {
        let mut cache = revision_cache.write().await;
        cache.insert(metadata.revision.clone());
        tracing::debug!(revision = %metadata.revision, "Added revision to cache");
        crate::metrics::set_revision_cache_size(cache.len());
        crate::metrics::increment_images_total();
    }

    // Broadcast new image event to SSE clients
    let _ = tx.send("new_image".to_string());
    tracing::debug!("Broadcasted new_image event to SSE clients");

    Ok(())
}

fn persist_image(temp_file: tempfile::NamedTempFile, output_path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    // NamedTempFile creates files with mode 0600 and persist() is a rename
    // that keeps it, so backups and other group/world readers cannot access
    // the stored image unless the mode is widened before the rename.
    temp_file
        .as_file()
        .set_permissions(std::fs::Permissions::from_mode(0o644))?;
    temp_file
        .persist(output_path)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(())
}

/// Reject a client-supplied value that is interpolated into the output
/// filename. Allows only a conservative filename-safe charset, so the value
/// cannot introduce a path separator, `..`, or an absolute path.
fn validate_path_component(value: &str, field: &'static str) -> Result<()> {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));

    if valid {
        Ok(())
    } else {
        Err(Error::InvalidUploadField { field })
    }
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
    let filename = format!("{repo_name}-{timestamp}-{commit_sha}.png");

    // Defense in depth: the filename must be exactly one normal path component
    // (no separators, no `..`, not absolute) so join() cannot escape
    // images_dir, even if a caller skipped input validation.
    let mut components = std::path::Path::new(&filename).components();
    match (components.next(), components.next()) {
        (Some(std::path::Component::Normal(_)), None) => {}
        _ => return Err(Error::PathTraversal { name: filename }),
    }

    let output_path = images_dir.join(filename);

    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_image_is_world_readable() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir()?;
        let output_path = dir.path().join("image.png");

        let temp_file = tempfile::NamedTempFile::new_in(dir.path())?;
        std::fs::write(temp_file.path(), b"png-bytes")?;

        persist_image(temp_file, &output_path)?;

        let mode = std::fs::metadata(&output_path)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o644, "stored image mode {mode:o} is not 644");
        Ok(())
    }

    fn server_config_with_images_dir(dir: &std::path::Path) -> config::ServerConfig {
        config::ServerConfig {
            images_dir: dir.to_string_lossy().into_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn validate_path_component_accepts_safe_values() -> Result<()> {
        for value in ["my-repo", "repo_1", "abc123def0", "a.b", "0"] {
            validate_path_component(value, "repo_name")?;
        }
        Ok(())
    }

    #[test]
    fn validate_path_component_rejects_traversal_and_separators() {
        for value in [
            "", ".", "..", "../etc", "a/b", "/abs", "a\\b", "a b", "a%2fb", "a\0b",
        ] {
            assert!(
                validate_path_component(value, "repo_name").is_err(),
                "expected {value:?} to be rejected"
            );
        }
    }

    #[test]
    fn get_output_path_confines_valid_input_to_images_dir() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let config = server_config_with_images_dir(dir.path());
        let path = get_output_path(&config, "repo", "abc1234")?;
        assert_eq!(path.parent(), Some(dir.path()));
        Ok(())
    }

    #[test]
    fn get_output_path_rejects_relative_traversal() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let config = server_config_with_images_dir(dir.path());
        let result = get_output_path(&config, "../../etc/evil", "abc1234");
        assert!(matches!(result, Err(Error::PathTraversal { .. })));
        Ok(())
    }

    #[test]
    fn get_output_path_rejects_absolute_repo_name() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let config = server_config_with_images_dir(dir.path());
        let result = get_output_path(&config, "/tmp/evil", "abc1234");
        assert!(matches!(result, Err(Error::PathTraversal { .. })));
        Ok(())
    }

    #[test]
    fn reserve_upload_slot_sheds_with_429_when_exhausted() -> Result<()> {
        let semaphore = Arc::new(Semaphore::new(1));

        // Hold the only permit for the rest of the test.
        let _held = match reserve_upload_slot(&semaphore) {
            Ok(permit) => permit,
            Err(_) => panic!("first reservation should succeed"),
        };

        // No slots left: the next reservation is shed with 429.
        match reserve_upload_slot(&semaphore) {
            Ok(_) => panic!("second reservation should be shed"),
            Err(status) => assert_eq!(status, StatusCode::TOO_MANY_REQUESTS),
        }

        Ok(())
    }

    #[test]
    fn reserve_upload_slot_frees_permit_on_drop() -> Result<()> {
        let semaphore = Arc::new(Semaphore::new(1));

        // Acquire and immediately drop, mirroring a processing task that ended
        // (via success, error, panic, or abort — all end with the permit drop).
        match reserve_upload_slot(&semaphore) {
            Ok(permit) => drop(permit),
            Err(_) => panic!("reservation should succeed"),
        }

        // The slot is available again.
        assert!(
            reserve_upload_slot(&semaphore).is_ok(),
            "slot should be reusable after the permit is dropped"
        );

        Ok(())
    }

    async fn fetch(router: Router, uri: &str) -> Response {
        use tower::ServiceExt;

        router
            .oneshot(
                axum::http::Request::builder()
                    .uri(uri)
                    .body(axum::body::Body::empty())
                    .expect("request builds"),
            )
            .await
            .expect("router is infallible")
    }

    fn static_root_fixture() -> Result<tempfile::TempDir> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("index.html"), "<h1>gallery</h1>")?;
        std::fs::write(dir.path().join("background.webp"), b"webp-bytes")?;
        Ok(dir)
    }

    async fn body_string(response: Response) -> Result<String> {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body collects");
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    #[tokio::test]
    async fn index_is_served_from_the_static_root() -> Result<()> {
        let dir = static_root_fixture()?;

        let response = fetch(static_router(dir.path()), "/").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_string(response).await?, "<h1>gallery</h1>");
        Ok(())
    }

    #[tokio::test]
    async fn assets_are_served_from_the_static_root() -> Result<()> {
        let dir = static_root_fixture()?;

        let response = fetch(static_router(dir.path()), "/static/background.webp").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_string(response).await?, "webp-bytes");
        Ok(())
    }

    #[tokio::test]
    async fn assets_are_cached_long_term() -> Result<()> {
        let dir = static_root_fixture()?;

        let response = fetch(static_router(dir.path()), "/static/background.webp").await;

        assert_eq!(
            response.headers().get(header::CACHE_CONTROL),
            Some(&axum::http::HeaderValue::from_static(STATIC_CACHE_CONTROL))
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_missing_asset_is_not_found() -> Result<()> {
        let dir = static_root_fixture()?;

        let response = fetch(static_router(dir.path()), "/static/nope.webp").await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        Ok(())
    }

    #[tokio::test]
    async fn an_absent_static_root_is_not_found_rather_than_fatal() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let missing = dir.path().join("not-installed");

        let response = fetch(static_router(&missing), "/").await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        Ok(())
    }
}
