//! Lolcommit capture and upload functionality.
//!
//! This module handles capturing webcam images and uploading them to the lolcommitsd server.
//!
//! # Error Handling Requirements
//!
//! The following rules govern error handling for the upload client:
//!
//! - **Camera not available** (device does not exist): Exit with error.
//! - **Camera busy** (device exists but in use): Exit with error, unless `--quiet` is passed.
//!   With `--quiet`, log "camera busy" at INFO level and exit with return code 0.
//! - **RUST_LOG**: When set, all logging should output at the appropriate level.
//! - **Connection failure** (camera capture succeeds but cannot connect to server): Exit with error.
//! - **Upload error** (camera capture succeeds, connection succeeds, but server returns 4xx/5xx):
//!   Log the error and exit with error.
//! - **Upload success** (camera capture succeeds, server returns 2xx): Log the response body at
//!   INFO level.

use crate::{
    camera, config,
    error::{Error, Result},
    git,
};
use serde::Serialize;
use std::io::Cursor;

pub struct CaptureArgs {
    pub revision: String,
    pub chyron: bool,
    pub no_chyron: bool,
    pub force: bool,
}

#[derive(Debug, Serialize)]
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
    burned_in_chyron: bool,
    force: bool,
}

pub fn capture_lolcommit(config: config::Config, args: CaptureArgs) -> Result<()> {
    // Get client config, defaulting if not present in config file
    let client_config = config.client.clone().unwrap_or_default();

    // Get burned_in_chyron setting, with CLI flags taking precedence
    let burned_in_chyron = if args.chyron {
        tracing::debug!("Chyron enabled via --chyron flag");
        true
    } else if args.no_chyron {
        tracing::debug!("Chyron disabled via --no-chyron flag");
        false
    } else {
        config
            .burned_in_chyron
            .as_ref()
            .map(|c| c.burned_in_chyron)
            .unwrap_or(true)
    };

    let repo = git::open_repo()?;

    // Resolve revision to full SHA
    let revision = git::resolve_revision(&repo, &args.revision)?;
    tracing::debug!(input = %args.revision, revision = %revision, "Resolved revision");

    let message = git::get_commit_message(&repo, &revision)?;
    tracing::info!(message = %message, revision = %revision, "Starting lolcommits");

    let repo_name = git::get_repo_name(&repo)?;
    let branch_name = git::get_branch_name(&repo)?;
    let stats = git::get_diff_stats(&revision)?;

    tracing::info!(
        repo_name = %repo_name,
        branch = %branch_name,
        files_changed = stats.files_changed,
        insertions = stats.insertions,
        deletions = stats.deletions,
        "Got git info"
    );

    // Capture image from webcam
    let image = camera::capture_image(&client_config)?;
    tracing::info!("Captured image from webcam");

    // Parse commit message
    let commit_type = git::parse_commit_type(&message);
    let first_line = message.lines().next().unwrap_or(&message);
    let scope = git::parse_commit_scope(first_line);
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Create metadata for upload
    let metadata = UploadMetadata {
        revision: revision.clone(),
        message: message.clone(),
        commit_type,
        scope,
        timestamp,
        repo_name,
        branch_name,
        files_changed: stats.files_changed,
        insertions: stats.insertions,
        deletions: stats.deletions,
        burned_in_chyron,
        force: args.force,
    };

    // Encode image to PNG bytes
    let mut png_bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    tracing::debug!(bytes = png_bytes.len(), "Encoded image to PNG");

    // Upload to server
    upload_to_server(&client_config, png_bytes, metadata)?;

    Ok(())
}

fn upload_to_server(
    config: &config::ClientConfig,
    image_bytes: Vec<u8>,
    metadata: UploadMetadata,
) -> Result<()> {
    let url = format!("{}/api/upload", config.server_url);
    tracing::info!(url = %url, "Uploading to server");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(
            config.server_upload_timeout_secs,
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

    let response =
        client
            .post(&url)
            .multipart(form)
            .send()
            .map_err(|e| Error::ServerConnectionFailed {
                url: url.clone(),
                source: e,
            })?;

    let status = response.status();
    let body = response
        .text()
        .unwrap_or_else(|_| "Unknown response".to_string());

    if status.is_success() {
        let message = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(String::from))
            .unwrap_or_else(|| body.clone());
        tracing::info!(status = %status, message = %message, "Upload successful");
        Ok(())
    } else {
        tracing::error!(status = %status, body = %body, "Upload failed");
        Err(Error::UploadFailed {
            status: status.as_u16(),
            body,
        })
    }
}
