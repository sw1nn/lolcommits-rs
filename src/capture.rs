use std::path::PathBuf;

use crate::{camera, config, error::Result, git, image_metadata, image_processor};

pub struct CaptureArgs {
    pub message: String,
    pub sha: String,
    pub chyron: bool,
    pub no_chyron: bool,
}

pub fn capture_lolcommit(args: CaptureArgs, mut config: config::Config) -> Result<PathBuf> {
    tracing::info!(message = %args.message, sha = %args.sha, "Starting lolcommits");

    // Override chyron setting if CLI flags are provided
    if args.chyron {
        config.enable_chyron = true;
        tracing::debug!("Chyron enabled via --chyron flag");
    } else if args.no_chyron {
        config.enable_chyron = false;
        tracing::debug!("Chyron disabled via --no-chyron flag");
    }

    let repo_name = git::get_repo_name()?;
    let branch_name = git::get_branch_name()?;
    let stats = git::get_diff_stats(&args.sha)?;
    tracing::info!(
        repo_name = %repo_name,
        branch = %branch_name,
        files_changed = stats.files_changed,
        insertions = stats.insertions,
        deletions = stats.deletions,
        "Got git info"
    );

    let image = camera::capture_image(&config.camera_device)?;
    tracing::info!("Captured image from webcam");

    let processed_image = image_processor::replace_background(image, &config)?;
    tracing::info!("Background replaced");

    let commit_type = git::parse_commit_type(&args.message);
    let first_line = args.message.lines().next().unwrap_or(&args.message);
    let message_without_prefix = git::strip_commit_prefix(first_line);
    let scope = git::parse_commit_scope(first_line);
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    // Create unified metadata struct
    let metadata = git::CommitMetadata {
        path: PathBuf::new(), // Not used for creating images
        sha: args.sha.clone(),
        message: message_without_prefix,
        commit_type: commit_type.clone(),
        scope,
        timestamp,
        repo_name: repo_name.clone(),
        branch_name,
        stats,
    };

    let final_image = if config.enable_chyron {
        let image_with_chyron =
            image_processor::overlay_chyron(processed_image, &metadata, &config)?;
        tracing::debug!(commit_type = %commit_type, "Overlaid chyron with stats");
        image_with_chyron
    } else {
        tracing::debug!("Chyron disabled, skipping overlay");
        processed_image
    };

    let output_path = get_output_path(&repo_name, &args.sha)?;

    // Save with embedded metadata
    image_metadata::save_png_with_metadata(&final_image, &output_path, &metadata)?;

    tracing::info!(path = %output_path.display(), "Saved lolcommit with metadata");

    Ok(output_path)
}

fn get_output_path(repo_name: &str, commit_sha: &str) -> Result<PathBuf> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("lolcommits")?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{}-{}-{}.png", repo_name, timestamp, commit_sha);

    let output_path = xdg_dirs.place_data_file(filename)?;

    Ok(output_path)
}
