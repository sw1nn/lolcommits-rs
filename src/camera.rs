use crate::error::{Error, Result};
use image::DynamicImage;
use nokhwa::Camera;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use std::path::Path;

fn parse_camera_device(device: &str) -> Result<CameraIndex> {
    if device.chars().all(|c| c.is_ascii_digit()) {
        let index = device.parse().unwrap_or(0);
        tracing::debug!(index, "Using numeric camera index");
        return Ok(CameraIndex::Index(index));
    }

    if device.starts_with('/') {
        let path = Path::new(device);

        let resolved_path = if path.is_symlink() {
            tracing::debug!(symlink = device, "Resolving symlink");
            std::fs::read_link(path).map_err(|e| Error::CameraError {
                message: e.to_string(),
                path: path.to_path_buf(),
            })?
        } else {
            path.to_path_buf()
        };

        tracing::debug!(resolved = %resolved_path.display(), "Resolved device path");

        if let Some(filename) = resolved_path.file_name() {
            let filename_str = filename.to_string_lossy();
            if let Some(index_str) = filename_str.strip_prefix("video") {
                if let Ok(index) = index_str.parse::<u32>() {
                    tracing::debug!(index, "Extracted index from device path");
                    return Ok(CameraIndex::Index(index));
                }
            }
        }

        return Err(Error::CameraError {
            message: "Could not extract video device index from path".to_string(),
            path: path.to_path_buf(),
        });
    }

    tracing::debug!(device, "Using device string for network camera");
    Ok(CameraIndex::String(device.to_string()))
}

pub fn capture_image(device: &str) -> Result<DynamicImage> {
    tracing::debug!(device, "Initializing camera");

    let index = parse_camera_device(device)?;
    let requested =
        RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);

    let mut camera = Camera::new(index, requested)?;

    tracing::debug!("Opening camera stream");
    camera.open_stream()?;

    tracing::debug!("Capturing frame");
    let frame = camera.frame()?;

    tracing::debug!("Converting frame to image");
    let decoded = frame.decode_image::<RgbFormat>()?;

    Ok(DynamicImage::ImageRgb8(decoded))
}
