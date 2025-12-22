use crate::error::Result;
use crate::git::{CommitMetadata, DiffStats};
use image::DynamicImage;
use png::Encoder;
use std::collections::HashMap;
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

    // Add metadata as iTXt chunks (UTF-8 safe, unlike tEXt which is Latin-1 only)
    encoder.add_itxt_chunk("lolcommit:revision".to_string(), metadata.revision.clone())?;
    encoder.add_itxt_chunk("lolcommit:message".to_string(), metadata.message.clone())?;
    encoder.add_itxt_chunk("lolcommit:type".to_string(), metadata.commit_type.clone())?;

    if !metadata.scope.is_empty() {
        encoder.add_itxt_chunk("lolcommit:scope".to_string(), metadata.scope.clone())?;
    }

    encoder.add_itxt_chunk(
        "lolcommit:timestamp".to_string(),
        metadata.timestamp.clone(),
    )?;
    encoder.add_itxt_chunk("lolcommit:repo".to_string(), metadata.repo_name.clone())?;
    encoder.add_itxt_chunk("lolcommit:branch".to_string(), metadata.branch_name.clone())?;
    encoder.add_itxt_chunk("lolcommit:diff".to_string(), metadata.diff_stats_string())?;
    encoder.add_itxt_chunk(
        "lolcommit:files_changed".to_string(),
        metadata.stats.files_changed.to_string(),
    )?;
    encoder.add_itxt_chunk(
        "lolcommit:insertions".to_string(),
        metadata.stats.insertions.to_string(),
    )?;
    encoder.add_itxt_chunk(
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

    // Build HashMaps for O(1) lookup - tEXt first, then iTXt overwrites (iTXt takes priority)
    let mut chunks: HashMap<&str, String> = info
        .uncompressed_latin1_text
        .iter()
        .map(|chunk| (chunk.keyword.as_str(), chunk.text.clone()))
        .collect();

    for chunk in &info.utf8_text {
        if let Ok(text) = chunk.get_text() {
            chunks.insert(&chunk.keyword, text);
        }
    }

    tracing::debug!(?chunks, "Loaded PNG metadata chunks");

    let revision = chunks.remove("lolcommit:revision").unwrap_or_default();
    let message = chunks.remove("lolcommit:message").unwrap_or_default();
    let commit_type = chunks.remove("lolcommit:type").unwrap_or_default();
    let scope = chunks.remove("lolcommit:scope").unwrap_or_default();
    let timestamp = chunks.remove("lolcommit:timestamp").unwrap_or_default();
    let repo_name = chunks.remove("lolcommit:repo").unwrap_or_default();
    let branch_name = chunks.remove("lolcommit:branch").unwrap_or_default();
    let files_changed = chunks
        .remove("lolcommit:files_changed")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let insertions = chunks
        .remove("lolcommit:insertions")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let deletions = chunks
        .remove("lolcommit:deletions")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let found_any = !revision.is_empty() || !message.is_empty() || !commit_type.is_empty();

    if found_any {
        Ok(Some(CommitMetadata {
            path: std::path::PathBuf::new(), // Will be set by caller
            revision,
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

    let revision = parts[0].to_string();
    let time_part = parts[1];
    let repo_name = parts[2].to_string();

    // Parse timestamp for display
    let timestamp =
        parse_timestamp(time_part).unwrap_or_else(|| format!("{}-{}", repo_name, time_part));

    Some(CommitMetadata {
        path: path.to_path_buf(),
        revision,
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
