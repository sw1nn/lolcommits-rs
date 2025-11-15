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
    let config = config::Config::load()?;
    tracing::debug!(?config, "Loaded configuration");

    let repo_name = git::get_repo_name()?;
    let branch_name = git::get_branch_name()?;
    let diff_stat = git::get_diff_shortstat()?;
    tracing::info!(repo_name = %repo_name, branch = %branch_name, diff_stat = %diff_stat, "Got git info");

    let image = camera::capture_image(&config.camera_device)?;
    tracing::info!("Captured image from webcam");

    let processed_image = image_processor::replace_background(image, &config)?;
    tracing::info!("Background replaced");

    let commit_type = parse_commit_type(&args.message);
    let first_line = args.message.lines().next().unwrap_or(&args.message);
    let message_without_prefix = strip_commit_prefix(first_line);
    let scope = parse_commit_scope(first_line);

    let final_image = image_processor::overlay_chyron(
        processed_image,
        &message_without_prefix,
        &commit_type,
        &scope,
        &repo_name,
        &diff_stat,
        &args.sha,
        &config,
    )?;
    tracing::info!(commit_type = %commit_type, "Overlaid chyron with stats");

    let output_path = get_output_path(&repo_name, &args.sha)?;

    // Prepare metadata
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let metadata = image_metadata::CommitMetadata {
        commit_sha: args.sha.clone(),
        commit_message: message_without_prefix.clone(),
        commit_type: commit_type.clone(),
        commit_scope: scope.clone(),
        timestamp,
        repo_name: repo_name.clone(),
        branch_name: branch_name.clone(),
        diff_stats: diff_stat.clone(),
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
