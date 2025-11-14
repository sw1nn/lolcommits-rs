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

    tracing::debug!("Mask full type: {}", mask_full.typ());

    // Extract mask as Vec<f32> (0-1 range from U2Net)
    let mask_bytes = mask_full.data_bytes()?;
    let mask_values: Vec<f32> = (0..(width * height))
        .map(|i| {
            let idx = (i as usize) * 4;
            f32::from_le_bytes([
                mask_bytes[idx],
                mask_bytes[idx + 1],
                mask_bytes[idx + 2],
                mask_bytes[idx + 3],
            ])
        })
        .collect();

    // Convert BGR back to RGB
    let mut rgb_mat = Mat::default();
    cvt_color(&bgr_mat, &mut rgb_mat, COLOR_BGR2RGB, 0, AlgorithmHint::ALGO_HINT_DEFAULT)?;
    let rgb_bytes: Vec<u8> = rgb_mat.data_bytes()?.to_vec();

    // Load background image using image crate
    let bg_image_path = std::path::Path::new("/home/swn/.local/share/backgrounds/GoogleMeetBackground.png");
    let bg_dynamic = image::open(bg_image_path)?;
    let bg_resized = bg_dynamic.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
    let bg_rgb = bg_resized.to_rgb8();
    let bg_bytes = bg_rgb.as_raw();

    // Composite: foreground * alpha + background * (1 - alpha)
    let mut result_data = Vec::with_capacity((width * height * 3) as usize);
    for i in 0..(width * height) as usize {
        let alpha = mask_values[i];  // 0-1 range
        let inv_alpha = 1.0 - alpha;

        let fg_r = rgb_bytes[i * 3] as f32;
        let fg_g = rgb_bytes[i * 3 + 1] as f32;
        let fg_b = rgb_bytes[i * 3 + 2] as f32;

        let bg_r = bg_bytes[i * 3] as f32;
        let bg_g = bg_bytes[i * 3 + 1] as f32;
        let bg_b = bg_bytes[i * 3 + 2] as f32;

        result_data.push((fg_r * alpha + bg_r * inv_alpha) as u8);
        result_data.push((fg_g * alpha + bg_g * inv_alpha) as u8);
        result_data.push((fg_b * alpha + bg_b * inv_alpha) as u8);
    }

    let result_image = image::RgbImage::from_raw(width, height, result_data)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "Failed to create composited image"))?;

    Ok(DynamicImage::ImageRgb8(result_image))
}

pub fn overlay_chyron(
    image: DynamicImage,
    message: &str,
    commit_type: &str,
    scope: &str,
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
    let info_text = if scope.is_empty() {
        format!("{} • {}", commit_type.to_uppercase(), repo_name)
    } else {
        format!("{} • {} • {}", commit_type.to_uppercase(), scope, repo_name)
    };
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
