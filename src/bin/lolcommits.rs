use clap::Parser;
use std::path::PathBuf;

use sw1nn_lolcommits_rs::{camera, config, error, git, image_metadata, image_processor};

use error::Result;

#[derive(Parser, Debug)]
#[command(name = "lolcommits")]
#[command(about = "Take a snapshot with your webcam when you commit")]
struct Args {
    #[arg(help = "The commit message")]
    message: String,

    #[arg(help = "The commit SHA")]
    sha: String,

    #[arg(long, action = clap::ArgAction::SetTrue, help = "Enable chyron overlay (overrides config)")]
    chyron: bool,

    #[arg(long, action = clap::ArgAction::SetTrue, help = "Disable chyron overlay (overrides config)")]
    no_chyron: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    tracing::info!(message = %args.message, sha = %args.sha, "Starting lolcommits");

    // Load configuration
    let mut config = config::Config::load()?;
    tracing::debug!(?config, "Loaded configuration");

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
    let (files_changed, insertions, deletions) = git::get_diff_stats(&args.sha)?;
    tracing::info!(
        repo_name = %repo_name,
        branch = %branch_name,
        files_changed = files_changed,
        insertions = insertions,
        deletions = deletions,
        "Got git info"
    );

    let image = camera::capture_image(&config.camera_device)?;
    tracing::info!("Captured image from webcam");

    let processed_image = image_processor::replace_background(image, &config)?;
    tracing::info!("Background replaced");

    let commit_type = parse_commit_type(&args.message);
    let first_line = args.message.lines().next().unwrap_or(&args.message);
    let message_without_prefix = strip_commit_prefix(first_line);
    let scope = parse_commit_scope(first_line);

    let final_image = if config.enable_chyron {
        let image_with_chyron = image_processor::overlay_chyron(
            processed_image,
            &message_without_prefix,
            &commit_type,
            &scope,
            &repo_name,
            files_changed,
            insertions,
            deletions,
            &args.sha,
            &config,
        )?;
        tracing::debug!(commit_type = %commit_type, "Overlaid chyron with stats");
        image_with_chyron
    } else {
        tracing::debug!("Chyron disabled, skipping overlay");
        processed_image
    };

    let output_path = get_output_path(&repo_name, &args.sha)?;

    // Prepare metadata
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    // Format diff stats for metadata storage (human-readable format)
    let diff_stats = if files_changed > 0 {
        let mut parts = vec![format!("{} file{} changed", files_changed, if files_changed == 1 { "" } else { "s" })];
        if insertions > 0 {
            parts.push(format!("{} insertion{}(+)", insertions, if insertions == 1 { "" } else { "s" }));
        }
        if deletions > 0 {
            parts.push(format!("{} deletion{}(-)", deletions, if deletions == 1 { "" } else { "s" }));
        }
        parts.join(", ")
    } else {
        String::new()
    };

    let metadata = image_metadata::CommitMetadata {
        commit_sha: args.sha.clone(),
        commit_message: message_without_prefix.clone(),
        commit_type: commit_type.clone(),
        commit_scope: scope.clone(),
        timestamp,
        repo_name: repo_name.clone(),
        branch_name: branch_name.clone(),
        diff_stats,
        files_changed,
        insertions,
        deletions,
    };

    // Save with embedded metadata
    image_metadata::save_png_with_metadata(&final_image, &output_path, metadata)?;

    tracing::info!(path = %output_path.display(), "Saved lolcommit with metadata");
    println!("Saved lolcommit to: {}", output_path.display());

    Ok(())
}

fn parse_commit_type(message: &str) -> String {
    let first_line = message.lines().next().unwrap_or(message);

    if let Some(colon_pos) = first_line.find(':') {
        let prefix = &first_line[..colon_pos];

        if let Some(paren_pos) = prefix.find('(') {
            prefix[..paren_pos].trim().to_string()
        } else {
            prefix.trim().to_string()
        }
    } else {
        "commit".to_string()
    }
}

fn strip_commit_prefix(message: &str) -> String {
    if let Some(colon_pos) = message.find(':') {
        message[colon_pos + 1..].trim().to_string()
    } else {
        message.to_string()
    }
}

fn parse_commit_scope(message: &str) -> String {
    if let Some(colon_pos) = message.find(':') {
        let prefix = &message[..colon_pos];

        if let Some(open_paren) = prefix.find('(')
            && let Some(close_paren) = prefix.find(')')
        {
            return prefix[open_paren + 1..close_paren].trim().to_string();
        }
    }

    String::new()
}

fn get_output_path(repo_name: &str, commit_sha: &str) -> Result<PathBuf> {
    let xdg_dirs = xdg::BaseDirectories::with_prefix("lolcommits")?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{}-{}-{}.png", repo_name, timestamp, commit_sha);

    let output_path = xdg_dirs.place_data_file(filename)?;

    Ok(output_path)
}
