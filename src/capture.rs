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
//!   INFO level and show a desktop notification, unless `desktop_notifications` is off. A
//!   notification that cannot be shown is logged and ignored: the upload already succeeded.
//! - **Not logged in** (no stored credentials, or the issuer refuses the refresh): Exit with
//!   error, telling the user to run `lolcommits-ctl login`. A failed refresh that was not a
//!   refusal — a network or DNS failure — is reported as itself instead.

use crate::{
    camera,
    config::{self, AuthConfig},
    error::{Error, Result},
    git, notify, oidc,
    oidc::TokenSet,
    secret::Secret,
    token_store,
};
use reqwest::blocking;
use serde::Serialize;
use std::io::Cursor;

/// Refresh an access token this many seconds before it actually expires, so a
/// slow upload cannot start with a token that dies mid-request.
const TOKEN_EXPIRY_LEEWAY_SECS: i64 = 60;

pub struct CaptureArgs {
    pub revision: String,
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
    force: bool,
}

pub fn capture_lolcommit(config: config::Config, args: CaptureArgs) -> Result<()> {
    // Get client config, defaulting if not present in config file
    let client_config = config.client.clone().unwrap_or_default();
    let auth_config = config.auth.clone().unwrap_or_default();

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
    let timestamp = chrono::Local::now()
        .format(crate::TIMESTAMP_FORMAT)
        .to_string();

    // Create metadata for upload
    let metadata = UploadMetadata {
        revision: revision.clone(),
        message: message.clone(),
        commit_type,
        scope,
        timestamp,
        repo_name: repo_name.clone(),
        branch_name,
        files_changed: stats.files_changed,
        insertions: stats.insertions,
        deletions: stats.deletions,
        force: args.force,
    };

    // Encode image to PNG bytes
    let mut png_bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    tracing::debug!(bytes = png_bytes.len(), "Encoded image to PNG");

    // Upload to server
    upload_to_server(&client_config, &auth_config, png_bytes, metadata)?;

    notify::upload_succeeded(
        &client_config,
        &notify::Upload {
            repo_name: &repo_name,
            revision: &revision,
            message: &message,
        },
    );

    Ok(())
}

fn upload_to_server(
    config: &config::ClientConfig,
    auth: &AuthConfig,
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

    let mut tokens = token_store::load(auth)?.ok_or(Error::NotLoggedIn)?;
    if tokens.is_expired(TOKEN_EXPIRY_LEEWAY_SECS) {
        tokens = refresh_and_store(&client, auth, &tokens)?;
    }

    let mut response = send_upload(
        &client,
        &url,
        &tokens.access_token,
        &image_bytes,
        &metadata_json,
    )?;

    // The daemon may reject a token this client still believes is valid (clock
    // skew, a revoked session). Refresh once, then give up.
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        tracing::debug!("Upload rejected as unauthorized, refreshing access token");
        let refreshed = refresh_and_store(&client, auth, &tokens)?;
        response = send_upload(
            &client,
            &url,
            &refreshed.access_token,
            &image_bytes,
            &metadata_json,
        )?;
    }

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

/// `multipart::Form` is not `Clone`, so each attempt needs its own.
fn upload_form(image_bytes: &[u8], metadata_json: &str) -> Result<blocking::multipart::Form> {
    Ok(blocking::multipart::Form::new()
        .part(
            "metadata",
            blocking::multipart::Part::text(metadata_json.to_owned())
                .mime_str("application/json")?,
        )
        .part(
            "image",
            blocking::multipart::Part::bytes(image_bytes.to_vec())
                .file_name("image.png")
                .mime_str("image/png")?,
        ))
}

fn send_upload(
    client: &blocking::Client,
    url: &str,
    access_token: &Secret,
    image_bytes: &[u8],
    metadata_json: &str,
) -> Result<blocking::Response> {
    client
        .post(url)
        .bearer_auth(access_token.expose())
        .multipart(upload_form(image_bytes, metadata_json)?)
        .send()
        .map_err(|source| Error::ServerConnectionFailed {
            url: url.to_owned(),
            source,
        })
}

fn refresh_and_store(
    client: &blocking::Client,
    auth: &AuthConfig,
    tokens: &TokenSet,
) -> Result<TokenSet> {
    let refresh_token = tokens.refresh_token.as_ref().ok_or(Error::NotLoggedIn)?;

    let refreshed = oidc::refresh(client, auth, refresh_token)
        .inspect_err(|error| tracing::debug!(%error, "Access token refresh failed"))
        .map_err(session_expiry)?;

    token_store::save(auth, &refreshed)?;
    Ok(refreshed)
}

/// A refresh the issuer actively refused means the session is gone as far as
/// the user is concerned, so report the one useful next step. Anything else —
/// DNS, a dead network, a 502 from a proxy — is reported as itself, because
/// telling the user to log in again would send them into the same failure.
fn session_expiry(error: Error) -> Error {
    match error {
        Error::TokenEndpointError { .. } | Error::TokenRequestFailed { .. } => Error::NotLoggedIn,
        other => other,
    }
}
