use clap::Parser;
use std::path::PathBuf;

mod error;
mod git;
mod camera;
mod image_processor;
mod segmentation;

use error::Result;

#[derive(Parser, Debug)]
#[command(name = "lolcommits-rs")]
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
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    let args = Args::parse();

    tracing::info!(message = %args.message, sha = %args.sha, "Starting lolcommits-rs");

    let repo_name = git::get_repo_name()?;
    tracing::info!(repo_name = %repo_name, "Got git info");

    let image = camera::capture_image()?;
    tracing::info!("Captured image from webcam");

    // TEMPORARY: Test full blur function (with early return)
    let blurred_image = image_processor::blur_background(image)?;
    tracing::info!("Full blur function test");

    let commit_type = parse_commit_type(&args.message);
    let first_line = args.message.lines().next().unwrap_or(&args.message);

    let processed_image = image_processor::overlay_chyron(
        blurred_image,
        first_line,
        &commit_type,
        &args.sha,
        &repo_name
    )?;
    tracing::info!(commit_type = %commit_type, "Overlaid chyron on image");

    let output_path = get_output_path(&repo_name, &args.sha)?;
    processed_image.save(&output_path)?;

    tracing::info!(path = %output_path.display(), "Saved lolcommit");
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

fn get_output_path(repo_name: &str, commit_sha: &str) -> Result<PathBuf> {
    let base_dir = directories::BaseDirs::new()
        .ok_or(error::LolcommitsError::NoHomeDirectory)?;

    let data_dir = base_dir.data_local_dir().join("lolcommits-rs");
    std::fs::create_dir_all(&data_dir)?;

    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let filename = format!("{}-{}-{}.png", repo_name, timestamp, commit_sha);

    Ok(data_dir.join(filename))
}
