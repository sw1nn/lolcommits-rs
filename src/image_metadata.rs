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
    encoder.add_itxt_chunk("lolcommit:Revision".to_string(), metadata.revision.clone())?;
    encoder.add_itxt_chunk("lolcommit:Message".to_string(), metadata.message.clone())?;
    encoder.add_itxt_chunk("lolcommit:Type".to_string(), metadata.commit_type.clone())?;

    if !metadata.scope.is_empty() {
        encoder.add_itxt_chunk("lolcommit:Scope".to_string(), metadata.scope.clone())?;
    }

    encoder.add_itxt_chunk(
        "lolcommit:Timestamp".to_string(),
        metadata.timestamp.clone(),
    )?;
    encoder.add_itxt_chunk("lolcommit:Repo".to_string(), metadata.repo_name.clone())?;
    encoder.add_itxt_chunk("lolcommit:Branch".to_string(), metadata.branch_name.clone())?;
    encoder.add_itxt_chunk("lolcommit:Diff".to_string(), metadata.diff_stats_string())?;
    encoder.add_itxt_chunk(
        "lolcommit:Files_changed".to_string(),
        metadata.stats.files_changed.to_string(),
    )?;
    encoder.add_itxt_chunk(
        "lolcommit:Insertions".to_string(),
        metadata.stats.insertions.to_string(),
    )?;
    encoder.add_itxt_chunk(
        "lolcommit:Deletions".to_string(),
        metadata.stats.deletions.to_string(),
    )?;

    let mut writer = encoder.write_header()?;
    writer.write_image_data(&rgb_image)?;

    Ok(())
}

/// Remove a key from the map, trying the new capitalized key first,
/// then falling back to the old lowercase key.
fn remove_key(chunks: &mut HashMap<&str, String>, new_key: &str, old_key: &str) -> String {
    chunks
        .remove(new_key)
        .or_else(|| chunks.remove(old_key))
        .unwrap_or_default()
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

    let revision = remove_key(&mut chunks, "lolcommit:Revision", "lolcommit:revision");
    let message = remove_key(&mut chunks, "lolcommit:Message", "lolcommit:message");
    let commit_type = remove_key(&mut chunks, "lolcommit:Type", "lolcommit:type");
    let scope = remove_key(&mut chunks, "lolcommit:Scope", "lolcommit:scope");
    let timestamp = remove_key(&mut chunks, "lolcommit:Timestamp", "lolcommit:timestamp");
    let repo_name = remove_key(&mut chunks, "lolcommit:Repo", "lolcommit:repo");
    let branch_name = remove_key(&mut chunks, "lolcommit:Branch", "lolcommit:branch");
    let files_changed = remove_key(
        &mut chunks,
        "lolcommit:Files_changed",
        "lolcommit:files_changed",
    )
    .parse()
    .unwrap_or(0);
    let insertions = remove_key(&mut chunks, "lolcommit:Insertions", "lolcommit:insertions")
        .parse()
        .unwrap_or(0);
    let deletions = remove_key(&mut chunks, "lolcommit:Deletions", "lolcommit:deletions")
        .parse()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::git::DiffStats;

    #[test]
    fn test_round_trip_new_keys() -> Result {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("test.png");

        let image = image::DynamicImage::ImageRgba8(image::RgbaImage::new(1, 1));
        let metadata = CommitMetadata {
            path: std::path::PathBuf::new(),
            revision: "abc1234".to_owned(),
            message: "feat: add something".to_owned(),
            commit_type: "feat".to_owned(),
            scope: "core".to_owned(),
            timestamp: "2024-01-15 12:34:56".to_owned(),
            repo_name: "my-repo".to_owned(),
            branch_name: "main".to_owned(),
            stats: DiffStats {
                files_changed: 3,
                insertions: 42,
                deletions: 7,
            },
        };

        save_png_with_metadata(&image, &path, &metadata)?;
        let read_back = read_png_metadata(&path)?;

        let read_back = read_back.expect("metadata should be present");
        assert_eq!(read_back.revision, metadata.revision);
        assert_eq!(read_back.message, metadata.message);
        assert_eq!(read_back.commit_type, metadata.commit_type);
        assert_eq!(read_back.scope, metadata.scope);
        assert_eq!(read_back.timestamp, metadata.timestamp);
        assert_eq!(read_back.repo_name, metadata.repo_name);
        assert_eq!(read_back.branch_name, metadata.branch_name);
        assert_eq!(read_back.stats.files_changed, metadata.stats.files_changed);
        assert_eq!(read_back.stats.insertions, metadata.stats.insertions);
        assert_eq!(read_back.stats.deletions, metadata.stats.deletions);

        Ok(())
    }

    #[test]
    fn test_reads_old_lowercase_keys() -> Result {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("old_style.png");

        // Write a PNG with old lowercase keys directly
        let file = File::create(&path)?;
        let writer = BufWriter::new(file);
        let mut encoder = Encoder::new(writer, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.add_itxt_chunk("lolcommit:revision".to_owned(), "def5678".to_owned())?;
        encoder.add_itxt_chunk(
            "lolcommit:message".to_owned(),
            "fix: old style commit".to_owned(),
        )?;
        encoder.add_itxt_chunk("lolcommit:type".to_owned(), "fix".to_owned())?;
        encoder.add_itxt_chunk("lolcommit:repo".to_owned(), "old-repo".to_owned())?;
        encoder.add_itxt_chunk("lolcommit:branch".to_owned(), "develop".to_owned())?;
        encoder.add_itxt_chunk(
            "lolcommit:timestamp".to_owned(),
            "2023-06-01 08:00:00".to_owned(),
        )?;
        encoder.add_itxt_chunk("lolcommit:files_changed".to_owned(), "5".to_owned())?;
        encoder.add_itxt_chunk("lolcommit:insertions".to_owned(), "20".to_owned())?;
        encoder.add_itxt_chunk("lolcommit:deletions".to_owned(), "3".to_owned())?;
        let mut png_writer = encoder.write_header()?;
        png_writer.write_image_data(&[0u8; 4])?;
        drop(png_writer);

        let read_back = read_png_metadata(&path)?;
        let read_back = read_back.expect("metadata should be present for old-style keys");

        assert_eq!(read_back.revision, "def5678");
        assert_eq!(read_back.message, "fix: old style commit");
        assert_eq!(read_back.commit_type, "fix");
        assert_eq!(read_back.repo_name, "old-repo");
        assert_eq!(read_back.branch_name, "develop");
        assert_eq!(read_back.timestamp, "2023-06-01 08:00:00");
        assert_eq!(read_back.stats.files_changed, 5);
        assert_eq!(read_back.stats.insertions, 20);
        assert_eq!(read_back.stats.deletions, 3);

        Ok(())
    }
}
