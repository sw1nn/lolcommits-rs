use axum::{
    Json, Router,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde::{Serialize, Serializer};
use tower_http::{
    services::ServeDir,
    trace::{DefaultMakeSpan, TraceLayer},
};
use xdg::BaseDirectories;

use crate::{config, git::CommitMetadata, image_metadata};

#[derive(Debug, Serialize)]
struct ConfigResponse {
    gallery_title: String,
}

#[derive(Debug)]
pub struct ImageMetadata(CommitMetadata);

impl std::ops::Deref for ImageMetadata {
    type Target = CommitMetadata;

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
        .nest_service("/images", ServeDir::new(&data_home))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("static/index.html"))
}

async fn list_images() -> Response {
    match get_image_list() {
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
    }
}

async fn get_config() -> Response {
    match config::Config::load() {
        Ok(cfg) => Json(ConfigResponse {
            gallery_title: cfg.gallery_title,
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

fn get_image_list() -> Result<Vec<CommitMetadata>, Box<dyn std::error::Error>> {
    let xdg_dirs = BaseDirectories::with_prefix("lolcommits")?;
    let data_home = xdg_dirs.get_data_home();

    let mut images: Vec<CommitMetadata> = std::fs::read_dir(&data_home)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|s| s.to_str()) == Some("png"))
        .filter_map(|path| image_metadata::parse_image_file(&path))
        .collect();

    // Sort by timestamp descending (newest first)
    images.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    Ok(images)
}
