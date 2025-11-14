use crate::error::Result;
use directories::BaseDirs;
use std::fs;
use std::path::PathBuf;

// Using U2Net model for background segmentation
// This model is well-tested with OpenCV DNN and provides good results
const MODEL_URL: &str = "https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2net.onnx";
const MODEL_FILENAME: &str = "u2net.onnx";

pub fn get_model_path() -> Result<PathBuf> {
    let base_dirs = BaseDirs::new()
        .ok_or(crate::error::LolcommitsError::NoHomeDirectory)?;

    let models_dir = base_dirs.data_local_dir().join("lolcommits-rs").join("models");
    fs::create_dir_all(&models_dir)?;

    let model_path = models_dir.join(MODEL_FILENAME);

    if !model_path.exists() {
        tracing::info!("Downloading segmentation model (this happens once)...");
        download_model(&model_path)?;
        tracing::info!("Model downloaded successfully");
    }

    Ok(model_path)
}

fn download_model(path: &PathBuf) -> Result<()> {
    let response = reqwest::blocking::get(MODEL_URL)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to download model: {}", e)))?;

    let bytes = response.bytes()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to read response: {}", e)))?;

    fs::write(path, bytes)?;

    Ok(())
}
