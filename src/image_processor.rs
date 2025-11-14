use crate::error::Result;
use ab_glyph::{FontRef, PxScale};
use image::{DynamicImage, Rgba};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect as ImageRect;
use opencv::core::{Mat, Size, Vec3b, AlgorithmHint, BORDER_DEFAULT, Scalar, CV_8UC3, CV_32F};
use opencv::imgproc::{gaussian_blur, resize, INTER_LINEAR, cvt_color, COLOR_RGB2BGR, COLOR_BGR2RGB};
use opencv::dnn::{read_net_from_onnx, DNN_BACKEND_OPENCV, DNN_TARGET_CPU};
use opencv::prelude::*;
use crate::segmentation;

const FONT_DATA: &[u8] = include_bytes!("/usr/share/fonts/TTF/InputSansCompressedNerdFont-Bold.ttf");

pub fn blur_background_simple_test(image: DynamicImage) -> Result<DynamicImage> {
    // TEMPORARY: Simple blur without segmentation to test color
    let rgb_image = image.to_rgb8();
    let (width, height) = rgb_image.dimensions();
    let image_data = rgb_image.into_raw();

    let vec3b_data: Vec<Vec3b> = image_data
        .chunks_exact(3)
        .map(|chunk| Vec3b::from([chunk[0], chunk[1], chunk[2]]))
        .collect();

    let temp_mat = Mat::from_slice(&vec3b_data)?;
    let rgb_mat = temp_mat.reshape(3, height as i32)?;

    // Convert RGB to BGR
    let mut bgr_mat = Mat::default();
    cvt_color(&rgb_mat, &mut bgr_mat, COLOR_RGB2BGR, 0, AlgorithmHint::ALGO_HINT_DEFAULT)?;

    // Simple blur
    let mut blurred_bgr = Mat::default();
    gaussian_blur(&bgr_mat, &mut blurred_bgr, Size::new(51, 51), 0.0, 0.0, BORDER_DEFAULT, AlgorithmHint::ALGO_HINT_DEFAULT)?;

    // Convert back to RGB
    let mut blurred_rgb = Mat::default();
    cvt_color(&blurred_bgr, &mut blurred_rgb, COLOR_BGR2RGB, 0, AlgorithmHint::ALGO_HINT_DEFAULT)?;

    let result_vec: Vec<u8> = blurred_rgb.data_bytes()?.to_vec();
    let result_image = image::RgbImage::from_raw(width, height, result_vec)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "Failed to create image"))?;

    Ok(DynamicImage::ImageRgb8(result_image))
}

pub fn blur_background_disabled(image: DynamicImage) -> Result<DynamicImage> {
    // TEMPORARY: Just pass through to test color
    Ok(image)
}

pub fn blur_background(image: DynamicImage) -> Result<DynamicImage> {
    let rgb_image = image.to_rgb8();
    let (width, height) = rgb_image.dimensions();
    let image_data = rgb_image.into_raw();

    // Convert Vec<u8> to Vec<Vec3b> for opencv - keep RGB order initially
    let vec3b_data: Vec<Vec3b> = image_data
        .chunks_exact(3)
        .map(|chunk| Vec3b::from([chunk[0], chunk[1], chunk[2]]))  // R, G, B as-is
        .collect();

    let temp_mat = Mat::from_slice(&vec3b_data)?;
    let rgb_mat = temp_mat.reshape(3, height as i32)?;

    tracing::debug!("Input RGB mat size: {:?}, type: {}", rgb_mat.size()?, rgb_mat.typ());

    // Use OpenCV's cvt_color to properly convert RGB to BGR
    let mut bgr_mat = Mat::default();
    cvt_color(&rgb_mat, &mut bgr_mat, COLOR_RGB2BGR, 0, AlgorithmHint::ALGO_HINT_DEFAULT)?;

    tracing::debug!("After RGB->BGR conversion, mat type: {}", bgr_mat.typ());

    // Get segmentation model
    let model_path = segmentation::get_model_path()?;
    tracing::debug!(path = %model_path.display(), "Loading segmentation model");

    let mut net = read_net_from_onnx(model_path.to_str().unwrap())?;
    net.set_preferable_backend(DNN_BACKEND_OPENCV)?;
    net.set_preferable_target(DNN_TARGET_CPU)?;

    // Prepare input: resize to 320x320 and normalize for U2Net
    let mut resized = Mat::default();
    resize(&bgr_mat, &mut resized, Size::new(320, 320), 0.0, 0.0, INTER_LINEAR)?;

    let mut input_float = Mat::default();
    resized.convert_to(&mut input_float, CV_32F, 1.0 / 255.0, 0.0)?;

    // Create blob from image - swap BGR to RGB for model
    let blob = opencv::dnn::blob_from_image(
        &input_float,
        1.0,
        Size::new(320, 320),
        Scalar::default(),
        true,  // swapRB: true to convert BGR to RGB for model
        false,
        CV_32F,
    )?;

    net.set_input(&blob, "", 1.0, Scalar::default())?;

    // Run inference
    tracing::debug!("Running segmentation inference");

    // Get all outputs (U2Net has 7 outputs, first one is the main mask)
    let output_names = net.get_unconnected_out_layers_names()?;
    let mut outputs = opencv::core::Vector::<Mat>::new();

    for output_name in output_names.iter() {
        let mut output = Mat::default();
        net.forward_layer(&mut output, &output_name)?;
        outputs.push(output);
    }

    if outputs.is_empty() {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, "No output from model").into());
    }

    // Use the first output (main segmentation mask) - shape is [1, 1, 320, 320]
    let output = outputs.get(0)?;
    tracing::debug!(shape = ?output.mat_size(), "Output shape");

    // The output is [1, 1, 320, 320], we need to extract the 320x320 data
    // Use data_bytes to get raw bytes, then convert to f32
    let output_bytes = output.data_bytes()?;
    let mut data_vec = Vec::with_capacity(320 * 320);
    for i in 0..(320 * 320) {
        let idx = i * 4;
        let val = f32::from_le_bytes([
            output_bytes[idx],
            output_bytes[idx + 1],
            output_bytes[idx + 2],
            output_bytes[idx + 3],
        ]);
        data_vec.push(val);
    }
    let mask_320 = Mat::new_rows_cols_with_data(320, 320, &data_vec)?.try_clone()?;

    tracing::debug!("Mask 320 type: {}, min/max checking", mask_320.typ());

    // Resize mask back to original size
    let mut mask_full = Mat::default();
    resize(&mask_320, &mut mask_full, Size::new(width as i32, height as i32), 0.0, 0.0, INTER_LINEAR)?;

    // Check mask range - U2Net outputs 0-1 range already
    tracing::debug!("Mask full type: {}", mask_full.typ());

    // Create blurred version
    let mut blurred_full = Mat::default();
    gaussian_blur(
        &bgr_mat,
        &mut blurred_full,
        Size::new(51, 51),
        0.0,
        0.0,
        BORDER_DEFAULT,
        AlgorithmHint::ALGO_HINT_DEFAULT,
    )?;

    // Convert images to float for blending (normalize to 0-1 range)
    let mut bgr_float = Mat::default();
    bgr_mat.convert_to(&mut bgr_float, CV_32F, 1.0 / 255.0, 0.0)?;

    let mut blurred_float = Mat::default();
    blurred_full.convert_to(&mut blurred_float, CV_32F, 1.0 / 255.0, 0.0)?;

    tracing::debug!(bgr_float_shape = ?bgr_float.mat_size(), mask_full_shape = ?mask_full.mat_size(), "Pre-conversion shapes");

    // Expand mask to 3 channels - manually replicate to avoid BGR/RGB confusion
    let mut mask_3ch = Mat::default();
    let mut mask_channels = opencv::core::Vector::<Mat>::new();
    mask_channels.push(mask_full.clone());
    mask_channels.push(mask_full.clone());
    mask_channels.push(mask_full);
    opencv::core::merge(&mask_channels, &mut mask_3ch)?;

    tracing::debug!(mask_3ch_shape = ?mask_3ch.mat_size(), "Mask 3ch shape");

    // TEMP: Use black background for testing
    let mut bg_bgr_mat = Mat::new_rows_cols_with_default(
        height as i32,
        width as i32,
        CV_8UC3,
        Scalar::all(0.0)
    )?;

    // Convert background to float (normalize to 0-1 range)
    let mut bg_float = Mat::default();
    bg_bgr_mat.convert_to(&mut bg_float, CV_32F, 1.0 / 255.0, 0.0)?;

    // Create inverse mask (ones mat needs to match mask_3ch dimensions)
    let mut inv_mask_3ch = Mat::default();
    let ones_mat = Mat::ones(mask_3ch.rows(), mask_3ch.cols(), mask_3ch.typ())?;
    opencv::core::subtract(&ones_mat.to_mat()?, &mask_3ch, &mut inv_mask_3ch, &opencv::core::no_array(), -1)?;

    // Blend: original * mask + background * (1 - mask)
    let mut original_masked = Mat::default();
    opencv::core::multiply(&bgr_float, &mask_3ch, &mut original_masked, 1.0, -1)?;

    let mut background_masked = Mat::default();
    opencv::core::multiply(&bg_float, &inv_mask_3ch, &mut background_masked, 1.0, -1)?;

    let mut combined_float = Mat::default();
    opencv::core::add(&original_masked, &background_masked, &mut combined_float, &opencv::core::no_array(), -1)?;

    // Convert back to uint8 (denormalize from 0-1 to 0-255)
    let mut combined_bgr = Mat::default();
    combined_float.convert_to(&mut combined_bgr, CV_8UC3, 255.0, 0.0)?;

    // Convert BGR back to RGB for output using OpenCV's cvt_color
    let mut combined_rgb = Mat::default();
    cvt_color(&combined_bgr, &mut combined_rgb, COLOR_BGR2RGB, 0, AlgorithmHint::ALGO_HINT_DEFAULT)?;

    let result_vec: Vec<u8> = combined_rgb.data_bytes()?.to_vec();

    let result_image = image::RgbImage::from_raw(width, height, result_vec)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "Failed to create image from blurred data"))?;

    Ok(DynamicImage::ImageRgb8(result_image))
}

pub fn overlay_chyron(
    image: DynamicImage,
    message: &str,
    commit_type: &str,
    sha: &str,
    repo_name: &str,
) -> Result<DynamicImage> {
    let font = FontRef::try_from_slice(FONT_DATA)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to load font: {}", e)))?;

    let mut rgba_image = image.to_rgba8();
    let (width, height) = rgba_image.dimensions();

    let chyron_height = 80;
    let y_start = height - chyron_height;

    let semi_transparent_black = Rgba([0u8, 0u8, 0u8, 200u8]);
    draw_filled_rect_mut(
        &mut rgba_image,
        ImageRect::at(0, y_start as i32).of_size(width, chyron_height),
        semi_transparent_black,
    );

    let white = Rgba([255u8, 255u8, 255u8, 255u8]);
    let yellow = Rgba([255u8, 255u8, 0u8, 255u8]);

    let title_scale = PxScale::from(28.0);
    let info_scale = PxScale::from(18.0);

    let title_y = y_start as i32 + 10;
    draw_text_mut(
        &mut rgba_image,
        white,
        15,
        title_y,
        title_scale,
        &font,
        message,
    );

    let info_y = y_start as i32 + 45;
    let info_text = format!("{} • {} • {}", commit_type.to_uppercase(), sha, repo_name);
    draw_text_mut(
        &mut rgba_image,
        yellow,
        15,
        info_y,
        info_scale,
        &font,
        &info_text,
    );

    Ok(DynamicImage::ImageRgba8(rgba_image))
}
