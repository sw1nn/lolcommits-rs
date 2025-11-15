use crate::error::Result;
use image::DynamicImage;
use png::Encoder;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub struct CommitMetadata {
    pub commit_sha: String,
    pub commit_message: String,
    pub commit_type: String,
    pub commit_scope: String,
    pub timestamp: String,
    pub repo_name: String,
    pub branch_name: String,
    pub diff_stats: String,
    pub files_changed: u32,
    pub insertions: u32,
    pub deletions: u32,
}

pub fn save_png_with_metadata<P: AsRef<Path>>(
    image: &DynamicImage,
    path: P,
    metadata: CommitMetadata,
) -> Result {
    let file = File::create(path.as_ref())?;
    let writer = BufWriter::new(file);

    let rgb_image = image.to_rgba8();
    let (width, height) = rgb_image.dimensions();

    let mut encoder = Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    // Add metadata as tEXt chunks
    encoder.add_text_chunk("lolcommit:sha".to_string(), metadata.commit_sha)?;
    encoder.add_text_chunk("lolcommit:message".to_string(), metadata.commit_message)?;
    encoder.add_text_chunk("lolcommit:type".to_string(), metadata.commit_type)?;

    if !metadata.commit_scope.is_empty() {
        encoder.add_text_chunk("lolcommit:scope".to_string(), metadata.commit_scope)?;
    }

    encoder.add_text_chunk("lolcommit:timestamp".to_string(), metadata.timestamp)?;
    encoder.add_text_chunk("lolcommit:repo".to_string(), metadata.repo_name)?;
    encoder.add_text_chunk("lolcommit:branch".to_string(), metadata.branch_name)?;
    encoder.add_text_chunk("lolcommit:diff".to_string(), metadata.diff_stats)?;
    encoder.add_text_chunk("lolcommit:files_changed".to_string(), metadata.files_changed.to_string())?;
    encoder.add_text_chunk("lolcommit:insertions".to_string(), metadata.insertions.to_string())?;
    encoder.add_text_chunk("lolcommit:deletions".to_string(), metadata.deletions.to_string())?;

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

    let mut metadata = CommitMetadata {
        commit_sha: String::new(),
        commit_message: String::new(),
        commit_type: String::new(),
        commit_scope: String::new(),
        timestamp: String::new(),
        repo_name: String::new(),
        branch_name: String::new(),
        diff_stats: String::new(),
        files_changed: 0,
        insertions: 0,
        deletions: 0,
    };

    let mut found_any = false;

    for chunk in text_chunks {
        match chunk.keyword.as_str() {
            "lolcommit:sha" => {
                metadata.commit_sha = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:message" => {
                metadata.commit_message = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:type" => {
                metadata.commit_type = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:scope" => {
                metadata.commit_scope = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:timestamp" => {
                metadata.timestamp = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:repo" => {
                metadata.repo_name = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:branch" => {
                metadata.branch_name = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:diff" => {
                metadata.diff_stats = chunk.text.clone();
                found_any = true;
            }
            "lolcommit:files_changed" => {
                metadata.files_changed = chunk.text.parse().unwrap_or(0);
                found_any = true;
            }
            "lolcommit:insertions" => {
                metadata.insertions = chunk.text.parse().unwrap_or(0);
                found_any = true;
            }
            "lolcommit:deletions" => {
                metadata.deletions = chunk.text.parse().unwrap_or(0);
                found_any = true;
            }
            _ => {}
        }
    }

    if found_any {
        Ok(Some(metadata))
    } else {
        Ok(None)
    }
}
