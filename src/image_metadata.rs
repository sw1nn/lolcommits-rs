use crate::error::Result;
use crate::git::{CommitMetadata, DiffStats};
use image::DynamicImage;
use png::Encoder;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub fn save_png_with_metadata<P: AsRef<Path>>(
    image: &DynamicImage,
    path: P,
    metadata: &CommitMetadata,
) -> Result {
    let file = File::create(path.as_ref())?;
    let writer = BufWriter::new(file);

    let rgb_image = image.to_rgba8();
    let (width, height) = rgb_image.dimensions();

    let mut encoder = Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    // Add metadata as tEXt chunks
    encoder.add_text_chunk("lolcommit:sha".to_string(), metadata.sha.clone())?;
    encoder.add_text_chunk("lolcommit:message".to_string(), metadata.message.clone())?;
    encoder.add_text_chunk("lolcommit:type".to_string(), metadata.commit_type.clone())?;

    if !metadata.scope.is_empty() {
        encoder.add_text_chunk("lolcommit:scope".to_string(), metadata.scope.clone())?;
    }

    encoder.add_text_chunk(
        "lolcommit:timestamp".to_string(),
        metadata.timestamp.clone(),
    )?;
    encoder.add_text_chunk("lolcommit:repo".to_string(), metadata.repo_name.clone())?;
    encoder.add_text_chunk("lolcommit:branch".to_string(), metadata.branch_name.clone())?;
    encoder.add_text_chunk("lolcommit:diff".to_string(), metadata.diff_stats_string())?;
    encoder.add_text_chunk(
        "lolcommit:files_changed".to_string(),
        metadata.stats.files_changed.to_string(),
    )?;
    encoder.add_text_chunk(
        "lolcommit:insertions".to_string(),
        metadata.stats.insertions.to_string(),
    )?;
    encoder.add_text_chunk(
        "lolcommit:deletions".to_string(),
        metadata.stats.deletions.to_string(),
    )?;

    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgb_image)?;

    Ok(())
}

pub fn read_png_metadata<P: AsRef<Path>>(path: P) -> Result<Option<CommitMetadata>> {
    let file = File::open(path.as_ref())?;
    let reader = std::io::BufReader::new(file);
    let decoder = png::Decoder::new(reader);
    let reader = decoder.read_info()?;

    let info = reader.info();
    let text_chunks = &info.uncompressed_latin1_text;

    let mut sha = String::new();
    let mut message = String::new();
    let mut commit_type = String::new();
    let mut scope = String::new();
    let mut timestamp = String::new();
    let mut repo_name = String::new();
    let mut branch_name = String::new();
    let mut files_changed = 0;
    let mut insertions = 0;
    let mut deletions = 0;

    let mut found_any = false;

    for chunk in text_chunks {
        match chunk.keyword.as_str() {
            "lolcommit:sha" => {
                sha = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:message" => {
                message = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:type" => {
                commit_type = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:scope" => {
                scope = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:timestamp" => {
                timestamp = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:repo" => {
                repo_name = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:branch" => {
                branch_name = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:files_changed" => {
                files_changed = chunk.text.parse().unwrap_or(0);
                found_any = true;
            }
            "lolcommit:insertions" => {
                insertions = chunk.text.parse().unwrap_or(0);
                found_any = true;
            }
            "lolcommit:deletions" => {
                deletions = chunk.text.parse().unwrap_or(0);
                found_any = true;
            }
            _ => {}
        }
    }

    if found_any {
        Ok(Some(CommitMetadata {
            path: std::path::PathBuf::new(), // Will be set by caller
            sha,
            message,
            commit_type,
            scope,
            timestamp,
            repo_name,
            branch_name,
            stats: DiffStats {
                files_changed,
                insertions,
                deletions,
            },
        }))
    } else {
        Ok(None)
    }
}

pub fn parse_image_file(path: &Path) -> Option<CommitMetadata> {
    let filename = path.file_name()?.to_str()?;

    // Try to read metadata from PNG file first
    if let Ok(Some(mut metadata)) = read_png_metadata(path) {
        tracing::debug!(filename, "Read metadata from PNG");
        metadata.path = path.to_path_buf();
        return Some(metadata);
    }

    // Fallback: parse filename for old images without metadata
    // Expected format: {repo_name}-{timestamp}-{commit_sha}.png
    // timestamp format: %Y%m%d-%H%M%S
    tracing::debug!(filename, "Falling back to filename parsing");
    let name = filename.strip_suffix(".png")?;
    let parts: Vec<&str> = name.rsplitn(3, '-').collect();

    if parts.len() != 3 {
        return None;
    }

    let sha = parts[0].to_string();
    let time_part = parts[1];
    let repo_name = parts[2].to_string();

    // Parse timestamp for display
    let timestamp =
        parse_timestamp(time_part).unwrap_or_else(|| format!("{}-{}", repo_name, time_part));

    Some(CommitMetadata {
        path: path.to_path_buf(),
        sha,
        message: String::new(),
        commit_type: String::new(),
        scope: String::new(),
        timestamp,
        repo_name,
        branch_name: String::new(),
        stats: DiffStats {
            files_changed: 0,
            insertions: 0,
            deletions: 0,
        },
    })
}

fn parse_timestamp(timestamp: &str) -> Option<String> {
    // Format: YYYYMMDD-HHMMSS
    if timestamp.len() != 15 {
        return None;
    }

    let year = timestamp.get(0..4)?;
    let month = timestamp.get(4..6)?;
    let day = timestamp.get(6..8)?;
    let hour = timestamp.get(9..11)?;
    let minute = timestamp.get(11..13)?;
    let second = timestamp.get(13..15)?;

    let datetime_str = format!("{}-{}-{} {}:{}:{}", year, month, day, hour, minute, second);

    Some(datetime_str)
}
