use crate::{camera, config, error::{Error, Result}, git};
use serde::Serialize;
use std::io::Cursor;

pub struct CaptureArgs {
    pub sha: String,
    pub chyron: bool,
    pub no_chyron: bool,
}

#[derive(Debug, Serialize)]
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

pub fn capture_lolcommit(args: CaptureArgs, mut config: config::Config) -> Result<()> {
    // Override chyron setting if CLI flags are provided
    if args.chyron {
        config.general.enable_chyron = true;
        tracing::debug!("Chyron enabled via --chyron flag");
    } else if args.no_chyron {
        config.general.enable_chyron = false;
        tracing::debug!("Chyron disabled via --no-chyron flag");
    }

    let repo = git::open_repo()?;

    let message = git::get_commit_message(&repo, &args.sha)?;
    tracing::info!(message = %message, sha = %args.sha, "Starting lolcommits");

    let repo_name = git::get_repo_name(&repo)?;
    let branch_name = git::get_branch_name(&repo)?;
    let stats = git::get_diff_stats(&args.sha)?;

    tracing::info!(
        repo_name = %repo_name,
        branch = %branch_name,
        files_changed = stats.files_changed,
        insertions = stats.insertions,
        deletions = stats.deletions,
        "Got git info"
    );

    // Capture image from webcam
    let image = match camera::capture_image(&config.client.camera_device) {
        Ok(img) => {
            tracing::info!("Captured image from webcam");
            img
        }
        Err(Error::CameraBusy { device }) => {
            tracing::warn!(
                device,
                "Camera is currently in use, skipping lolcommit capture"
            );
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    // Parse commit message
    let commit_type = git::parse_commit_type(&message);
    let first_line = message.lines().next().unwrap_or(&message);
    let message_without_prefix = git::strip_commit_prefix(first_line);
    let scope = git::parse_commit_scope(first_line);
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Create metadata for upload
    let metadata = UploadMetadata {
        sha: args.sha.clone(),
        message: message_without_prefix,
        commit_type,
        scope,
        timestamp,
        repo_name,
        branch_name,
        files_changed: stats.files_changed,
        insertions: stats.insertions,
        deletions: stats.deletions,
        enable_chyron: config.general.enable_chyron,
    };

    // Encode image to PNG bytes
    let mut png_bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    tracing::debug!(bytes = png_bytes.len(), "Encoded image to PNG");

    // Upload to server
    upload_to_server(&config, png_bytes, metadata)?;

    Ok(())
}

fn upload_to_server(
    config: &config::Config,
    image_bytes: Vec<u8>,
    metadata: UploadMetadata,
) -> Result<()> {
    let url = format!("{}/api/upload", config.client.server_url);
    tracing::info!(url = %url, "Uploading to server");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.client.server_upload_timeout_secs,
        ))
        .build()?;

    let metadata_json = serde_json::to_string(&metadata)?;

    let form = reqwest::blocking::multipart::Form::new()
        .part(
            "metadata",
            reqwest::blocking::multipart::Part::text(metadata_json).mime_str("application/json")?,
        )
        .part(
            "image",
            reqwest::blocking::multipart::Part::bytes(image_bytes)
                .file_name("image.png")
                .mime_str("image/png")?,
        );

    let response = client.post(&url).multipart(form).send().map_err(|e| {
        tracing::error!(error = %e, "Failed to connect to server");
        Error::ServerConnectionFailed {
            url: url.clone(),
            source: e,
        }
    })?;

    if response.status() == reqwest::StatusCode::ACCEPTED {
        tracing::info!("Upload accepted, server processing in background");
        Ok(())
    } else {
        let status = response.status();
        let error_text = response
            .text()
            .unwrap_or_else(|_| "Unknown error".to_string());
        tracing::error!(status = %status, error = %error_text, "Upload failed");
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Server returned {}: {}", status, error_text),
        )
        .into())
    }
}
