use crate::config::{CameraDeviceConfig, ClientConfig};
use crate::error::{Error, Result};
use image::DynamicImage;
use nokhwa::Camera;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType};
use std::panic;
use std::path::Path;

fn parse_frame_format(format_str: &str) -> Option<FrameFormat> {
    match format_str.to_uppercase().as_str() {
        "YUYV" | "YUY2" => Some(FrameFormat::YUYV),
        "MJPEG" | "MJPG" => Some(FrameFormat::MJPEG),
        "NV12" => Some(FrameFormat::NV12),
        "GRAY" | "GREY" => Some(FrameFormat::GRAY),
        _ => None,
    }
}

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
            std::fs::read_link(path).map_err(|source| Error::CameraSymlinkResolution {
                path: path.to_path_buf(),
                source,
            })?
        } else {
            path.to_path_buf()
        };

        tracing::debug!(resolved = %resolved_path.display(), "Resolved device path");

        if let Some(filename) = resolved_path.file_name() {
            let filename_str = filename.to_string_lossy();
            if let Some(index_str) = filename_str.strip_prefix("video")
                && let Ok(index) = index_str.parse::<u32>()
            {
                tracing::debug!(index, "Extracted index from device path");
                return Ok(CameraIndex::Index(index));
            }
        }

        return Err(Error::CameraInvalidDevicePath {
            path: path.to_path_buf(),
        });
    }

    tracing::debug!(device, "Using device string for network camera");
    Ok(CameraIndex::String(device.to_string()))
}

fn try_camera_with_device_config(
    index: &CameraIndex,
    device_config: &CameraDeviceConfig,
) -> Option<Result<Camera>> {
    // All four settings must be provided to use explicit config
    let format_str = device_config.format.as_ref()?;
    let width = device_config.width?;
    let height = device_config.height?;
    let fps = device_config.fps?;

    let format = match parse_frame_format(format_str) {
        Some(f) => f,
        None => {
            tracing::warn!(format = format_str, "Unknown camera format in config");
            return Some(Err(Error::UnknownCameraFormat {
                format: format_str.to_string(),
            }));
        }
    };

    tracing::debug!(
        format = format_str,
        width,
        height,
        fps,
        "Using camera format from config"
    );

    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(
        nokhwa::utils::CameraFormat::new(
            nokhwa::utils::Resolution::new(width, height),
            format,
            fps,
        ),
    ));

    Some(Camera::new(index.clone(), requested).map_err(Into::into))
}

fn try_camera_formats(index: &CameraIndex) -> Result<Camera> {
    // Format preferences in order: YUYV is most reliable, MJPEG as fallback
    let format_attempts = [
        ("YUYV 1280x960", FrameFormat::YUYV, 1280, 960, 30),
        ("YUYV 1280x720", FrameFormat::YUYV, 1280, 720, 30),
        ("YUYV 640x480", FrameFormat::YUYV, 640, 480, 30),
        ("MJPEG 1280x720", FrameFormat::MJPEG, 1280, 720, 30),
        ("MJPEG 640x480", FrameFormat::MJPEG, 640, 480, 30),
    ];

    tracing::debug!("Auto-detecting camera format");
    let mut last_error = None;

    for (name, format, width, height, fps) in format_attempts {
        let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(
            nokhwa::utils::CameraFormat::new(
                nokhwa::utils::Resolution::new(width, height),
                format,
                fps,
            ),
        ));

        match Camera::new(index.clone(), requested) {
            Ok(camera) => {
                tracing::debug!(format = name, "Camera initialized with format");
                return Ok(camera);
            }
            Err(e) => {
                tracing::debug!(format = name, error = %e, "Format not available, trying next");
                last_error = Some(e);
            }
        }
    }

    Err(last_error
        .map(Into::into)
        .unwrap_or_else(|| std::io::Error::other("No compatible camera format found").into()))
}

/// Try to capture an image from a single camera device.
fn try_capture_from_device(device_config: &CameraDeviceConfig) -> Result<DynamicImage> {
    tracing::debug!(device = device_config.device, "Trying camera device");

    let index = parse_camera_device(&device_config.device)?;

    // Use device-specific format if all settings provided, otherwise auto-detect
    let mut camera = match try_camera_with_device_config(&index, device_config) {
        Some(result) => result?,
        None => try_camera_formats(&index)?,
    };

    // Log available formats
    if let Ok(formats) = camera.compatible_camera_formats() {
        for fmt in &formats {
            tracing::debug!(
                format = ?fmt.format(),
                resolution = ?fmt.resolution(),
                frame_rate = fmt.frame_rate(),
                "Available camera format"
            );
        }
    }

    // Log the selected format
    let camera_format = camera.camera_format();
    tracing::debug!(
        format = ?camera_format.format(),
        resolution = ?camera_format.resolution(),
        frame_rate = camera_format.frame_rate(),
        "Selected camera format"
    );

    tracing::debug!("Opening camera stream");
    if let Err(e) = camera.open_stream() {
        // Check if the error message indicates the device is busy
        let error_msg = e.to_string().to_lowercase();
        if error_msg.contains("busy") || error_msg.contains("in use") {
            tracing::debug!(device = device_config.device, error = %e, "Camera appears to be busy");
            return Err(Error::CameraBusy {
                device: device_config.device.clone(),
            });
        }
        // For other errors, propagate as before
        return Err(e.into());
    }

    tracing::debug!("Capturing frame");
    let frame = camera.frame()?;
    tracing::debug!(
        source_format = ?frame.source_frame_format(),
        buffer_len = frame.buffer().len(),
        "Frame captured"
    );

    tracing::debug!("Converting frame to image");
    // Use catch_unwind to catch potential panics from decode_image
    let decode_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        frame.decode_image::<RgbFormat>()
    }));

    let decoded = match decode_result {
        Ok(Ok(img)) => {
            tracing::debug!("Frame decoded successfully");
            img
        }
        Ok(Err(e)) => {
            tracing::error!(error = %e, "Failed to decode camera frame");
            return Err(e.into());
        }
        Err(panic_info) => {
            let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic".to_string()
            };
            tracing::error!(panic = %panic_msg, "Panic during frame decode");
            return Err(
                std::io::Error::other(format!("Frame decode panicked: {}", panic_msg)).into(),
            );
        }
    };

    Ok(DynamicImage::ImageRgb8(decoded))
}

/// Capture an image from a camera.
///
/// Tries each camera device in order from config until one successfully captures.
pub fn capture_image(config: &ClientConfig) -> Result<DynamicImage> {
    let devices = &config.camera_devices;
    tracing::debug!(device_count = devices.len(), "Camera devices to try");

    let mut last_error = None;

    for device_config in devices {
        match try_capture_from_device(device_config) {
            Ok(image) => {
                tracing::info!(
                    device = device_config.device,
                    "Successfully captured from camera"
                );
                return Ok(image);
            }
            Err(e) => {
                tracing::debug!(device = device_config.device, error = %e, "Camera failed, trying next");
                last_error = Some(e);
            }
        }
    }

    // All cameras failed, return the last error
    Err(last_error.unwrap_or_else(|| std::io::Error::other("No camera devices configured").into()))
}
