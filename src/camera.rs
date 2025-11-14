use crate::error::Result;
use image::DynamicImage;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;

pub fn capture_image() -> Result<DynamicImage> {
    tracing::debug!("Initializing camera");

    let index = CameraIndex::Index(0);
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);

    let mut camera = Camera::new(index, requested)?;

    tracing::debug!("Opening camera stream");
    camera.open_stream()?;

    tracing::debug!("Capturing frame");
    let frame = camera.frame()?;

    tracing::debug!("Converting frame to image");
    let decoded = frame.decode_image::<RgbFormat>()?;

    Ok(DynamicImage::ImageRgb8(decoded))
}
