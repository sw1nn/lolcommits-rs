use crate::error::Result;
use std::fs;
use std::path::PathBuf;
use xdg::BaseDirectories;

// Using U2Net model for background segmentation
// This model is well-tested with OpenCV DNN and provides good results
const MODEL_URL: &str = "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2net.onnx";
const MODEL_FILENAME: &str = "u2net.onnx";

pub fn get_model_path() -> Result<PathBuf> {
    let xdg_dirs = BaseDirectories::with_prefix("lolcommits-rs").map_err(|e| {
        crate::error::LolcommitsError::ConfigError {
            message: format!("Failed to get XDG base directories: {}", e),
        }
    })?;

    let model_path = xdg_dirs.place_cache_file(MODEL_FILENAME).map_err(|e| {
        crate::error::LolcommitsError::ConfigError {
            message: format!("Failed to create cache directory: {}", e),
        }
    })?;

    if !model_path.exists() {
        tracing::info!("Downloading segmentation model (this happens once)...");
        download_model(&model_path)?;
        tracing::info!("Model downloaded successfully");
    }

    Ok(model_path)
}

fn download_model(path: &PathBuf) -> Result<()> {
    let response = reqwest::blocking::get(MODEL_URL)
        .map_err(|e| std::io::Error::other(format!("Failed to download model: {}", e)))?;

    let bytes = response
        .bytes()
        .map_err(|e| std::io::Error::other(format!("Failed to read response: {}", e)))?;

    fs::write(path, bytes)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_model_path_creates_directory() {
        // Test that get_model_path successfully creates a path
        // Note: This will actually create the XDG cache directory if it doesn't exist
        let result = get_model_path();
        assert!(result.is_ok());

        let path = result.unwrap();
        // Should end with the model filename
        assert!(path.to_string_lossy().ends_with(MODEL_FILENAME));

        // Parent directory should exist (created by place_cache_file)
        assert!(path.parent().unwrap().exists());
    }

    #[test]
    fn test_model_path_uses_xdg_cache() {
        // Verify that the model path is in the XDG cache directory
        let result = get_model_path();
        assert!(result.is_ok());

        let path = result.unwrap();
        let path_str = path.to_string_lossy();

        // Should contain "cache" and "lolcommits-rs" in the path
        assert!(path_str.contains("cache"));
        assert!(path_str.contains("lolcommits-rs"));
    }
}
