use crate::config::Config;
use crate::error::Result;
use crate::segmentation;
use ab_glyph::{FontRef, PxScale};
use image::{DynamicImage, Rgba};
use imageproc::drawing::draw_text_mut;
use opencv::core::{AlgorithmHint, CV_32F, Mat, Scalar, Size, Vec3b};
use opencv::dnn::{DNN_BACKEND_OPENCV, DNN_TARGET_CPU, read_net_from_onnx};
use opencv::imgproc::{
    COLOR_BGR2RGB, COLOR_RGB2BGR, INTER_LINEAR, cvt_color, resize,
};
use opencv::prelude::*;
use std::path::Path;

pub fn replace_background(image: DynamicImage, config: &Config) -> Result<DynamicImage> {
    let rgb_image = image.to_rgb8();
    let (width, height) = rgb_image.dimensions();
    let image_data = rgb_image.into_raw();

    // Convert Vec<u8> to Vec<Vec3b> for opencv - keep RGB order initially
    let vec3b_data: Vec<Vec3b> = image_data
        .chunks_exact(3)
        .map(|chunk| Vec3b::from([chunk[0], chunk[1], chunk[2]])) // R, G, B as-is
        .collect();

    let temp_mat = Mat::from_slice(&vec3b_data)?;
    let rgb_mat = temp_mat.reshape(3, height as i32)?;

    tracing::debug!(
        "Input RGB mat size: {:?}, type: {}",
        rgb_mat.size()?,
        rgb_mat.typ()
    );

    // Use OpenCV's cvt_color to properly convert RGB to BGR
    let mut bgr_mat = Mat::default();
    cvt_color(
        &rgb_mat,
        &mut bgr_mat,
        COLOR_RGB2BGR,
        0,
        AlgorithmHint::ALGO_HINT_DEFAULT,
    )?;

    tracing::debug!("After RGB->BGR conversion, mat type: {}", bgr_mat.typ());

    // Get segmentation model
    let model_path = segmentation::get_model_path()?;
    tracing::debug!(path = %model_path.display(), "Loading segmentation model");

    let mut net = read_net_from_onnx(model_path.to_str().unwrap())?;
    net.set_preferable_backend(DNN_BACKEND_OPENCV)?;
    net.set_preferable_target(DNN_TARGET_CPU)?;

    // Prepare input: resize to 320x320 and normalize for U2Net
    let mut resized = Mat::default();
    resize(
        &bgr_mat,
        &mut resized,
        Size::new(320, 320),
        0.0,
        0.0,
        INTER_LINEAR,
    )?;

    let mut input_float = Mat::default();
    resized.convert_to(&mut input_float, CV_32F, 1.0 / 255.0, 0.0)?;

    // Create blob from image - swap BGR to RGB for model
    let blob = opencv::dnn::blob_from_image(
        &input_float,
        1.0,
        Size::new(320, 320),
        Scalar::default(),
        true, // swapRB: true to convert BGR to RGB for model
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
        return Err(std::io::Error::other("No output from model").into());
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
    resize(
        &mask_320,
        &mut mask_full,
        Size::new(width as i32, height as i32),
        0.0,
        0.0,
        INTER_LINEAR,
    )?;

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

    // Calculate center of mass of the mask to find person's center
    let mut sum_x = 0.0_f32;
    let mut sum_y = 0.0_f32;
    let mut total_weight = 0.0_f32;

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) as usize;
            let weight = mask_values[idx];
            if weight > 0.1 {
                // Only consider pixels that are likely person
                sum_x += x as f32 * weight;
                sum_y += y as f32 * weight;
                total_weight += weight;
            }
        }
    }

    let (person_center_x, person_center_y) = if total_weight > 0.0 {
        (sum_x / total_weight, sum_y / total_weight)
    } else {
        (width as f32 / 2.0, height as f32 / 2.0) // Default to image center if no person detected
    };

    let image_center_x = width as f32 / 2.0;
    let image_center_y = height as f32 / 2.0;

    let offset_x = (image_center_x - person_center_x) as i32;
    let offset_y = (image_center_y - person_center_y) as i32;

    tracing::debug!(
        person_center_x = person_center_x,
        person_center_y = person_center_y,
        offset_x = offset_x,
        offset_y = offset_y,
        "Calculated person center and offset"
    );

    // Convert BGR back to RGB
    let mut rgb_mat = Mat::default();
    cvt_color(
        &bgr_mat,
        &mut rgb_mat,
        COLOR_BGR2RGB,
        0,
        AlgorithmHint::ALGO_HINT_DEFAULT,
    )?;
    let rgb_bytes: Vec<u8> = rgb_mat.data_bytes()?.to_vec();

    // Load background image using image crate
    let bg_image_path = Path::new(&config.background_path);
    tracing::debug!(path = %bg_image_path.display(), "Loading background image");
    let bg_dynamic = image::open(bg_image_path)?;
    let bg_resized = bg_dynamic.resize_exact(width, height, image::imageops::FilterType::Lanczos3);
    let bg_rgb = bg_resized.to_rgb8();
    let bg_bytes = bg_rgb.as_raw();

    // Composite: foreground * alpha + background * (1 - alpha) with translation
    let mut result_data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let dest_idx = (y * width + x) as usize;

            // Calculate source position with offset
            let src_x = x as i32 - offset_x;
            let src_y = y as i32 - offset_y;

            // Check if source position is within bounds
            if src_x >= 0 && src_x < width as i32 && src_y >= 0 && src_y < height as i32 {
                let src_idx = (src_y as u32 * width + src_x as u32) as usize;
                let alpha = mask_values[src_idx]; // 0-1 range
                let inv_alpha = 1.0 - alpha;

                let fg_r = rgb_bytes[src_idx * 3] as f32;
                let fg_g = rgb_bytes[src_idx * 3 + 1] as f32;
                let fg_b = rgb_bytes[src_idx * 3 + 2] as f32;

                let bg_r = bg_bytes[dest_idx * 3] as f32;
                let bg_g = bg_bytes[dest_idx * 3 + 1] as f32;
                let bg_b = bg_bytes[dest_idx * 3 + 2] as f32;

                result_data.push((fg_r * alpha + bg_r * inv_alpha) as u8);
                result_data.push((fg_g * alpha + bg_g * inv_alpha) as u8);
                result_data.push((fg_b * alpha + bg_b * inv_alpha) as u8);
            } else {
                // Out of bounds, use background only
                let bg_r = bg_bytes[dest_idx * 3];
                let bg_g = bg_bytes[dest_idx * 3 + 1];
                let bg_b = bg_bytes[dest_idx * 3 + 2];

                result_data.push(bg_r);
                result_data.push(bg_g);
                result_data.push(bg_b);
            }
        }
    }

    let result_image = image::RgbImage::from_raw(width, height, result_data).ok_or_else(|| {
        std::io::Error::other(
            "Failed to create composited image",
        )
    })?;

    Ok(DynamicImage::ImageRgb8(result_image))
}

pub fn overlay_chyron(
    image: DynamicImage,
    message: &str,
    commit_type: &str,
    scope: &str,
    repo_name: &str,
    stats: &str,
    sha: &str,
    config: &Config,
) -> Result<DynamicImage> {
    // Load font from configured path - need to leak for FontRef lifetime
    let font_data = std::fs::read(&config.font_path).map_err(|e| {
        std::io::Error::other(format!("Failed to read font from {}: {}", config.font_path, e))
    })?;

    let font_data_static: &'static [u8] = Box::leak(font_data.into_boxed_slice());
    let font = FontRef::try_from_slice(font_data_static).map_err(|e| {
        std::io::Error::other(format!("Failed to parse font: {}", e))
    })?;

    // Work directly with RGBA if already RGBA, otherwise convert
    let mut rgba_image = match image {
        DynamicImage::ImageRgba8(img) => img,
        other => other.to_rgba8(),
    };
    let (width, height) = rgba_image.dimensions();

    let chyron_height = 80;
    let y_start = height - chyron_height;

    // Manually apply semi-transparent black with proper alpha blending
    let overlay_alpha = config.chyron_opacity;
    for y in y_start..height {
        for x in 0..width {
            let pixel = rgba_image.get_pixel_mut(x, y);
            let [r, g, b, a] = pixel.0;

            // Blend: result = overlay * overlay_alpha + background * (1 - overlay_alpha)
            pixel.0 = [
                (0.0 * overlay_alpha + r as f32 * (1.0 - overlay_alpha)) as u8,
                (0.0 * overlay_alpha + g as f32 * (1.0 - overlay_alpha)) as u8,
                (0.0 * overlay_alpha + b as f32 * (1.0 - overlay_alpha)) as u8,
                a, // Keep original alpha
            ];
        }
    }

    let white = Rgba([255u8, 255u8, 255u8, 255u8]);
    let yellow = Rgba([255u8, 255u8, 0u8, 255u8]);
    let grey = Rgba([180u8, 180u8, 180u8, 255u8]);

    let title_scale = PxScale::from(config.title_font_size);
    let info_scale = PxScale::from(config.info_font_size);

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
        grey,
        15,
        info_y,
        info_scale,
        &font,
        &info_text,
    );

    // Calculate stats width first to determine left-aligned starting position
    let stats_start_x = if !stats.is_empty() {
        let parts: Vec<&str> = stats.split(',').map(|s| s.trim()).collect();
        let mut total_width = 0;

        for part in parts.iter() {
            if (part.contains("deletion") || part.contains("insertion"))
                && let Some(space_pos) = part.find(' ') {
                    let num = &part[..space_pos];
                    total_width += (num.len() as f32 * 10.0) as i32; // number width
                    total_width += 5; // gap after number
                    total_width += 10; // +/- symbol
                    total_width += 15; // gap before next item
                }
        }
        (width as i32) - 30 - total_width
    } else {
        (width as i32) - 150 // default position if no stats
    };

    // Draw SHA on the right side of the title line, left-aligned with stats
    if !sha.is_empty() {
        let sha_short = if sha.len() > 7 { &sha[..7] } else { sha };
        draw_text_mut(
            &mut rgba_image,
            yellow,
            stats_start_x,
            title_y,
            title_scale,
            &font,
            sha_short,
        );
    }

    // Draw colorized stats on the right side, left-aligned with SHA
    if !stats.is_empty() {
        let green = Rgba([0u8, 255u8, 0u8, 255u8]);
        let red = Rgba([255u8, 0u8, 0u8, 255u8]);

        let mut x_offset = stats_start_x;

        // Parse stats: "N file(s) changed, M insertion(s)(+), K deletion(s)(-)"
        let parts: Vec<&str> = stats.split(',').map(|s| s.trim()).collect();

        // Process in forward order for left-to-right drawing
        for part in parts.iter() {
            if part.contains("insertion") {
                // Extract number and draw in green
                if let Some(space_pos) = part.find(' ') {
                    let num = &part[..space_pos];

                    // Draw "+"
                    draw_text_mut(
                        &mut rgba_image,
                        green,
                        x_offset,
                        info_y,
                        info_scale,
                        &font,
                        "+",
                    );
                    x_offset += 10;

                    // Draw number
                    draw_text_mut(
                        &mut rgba_image,
                        green,
                        x_offset,
                        info_y,
                        info_scale,
                        &font,
                        num,
                    );
                    let text_width = (num.len() as f32 * 10.0) as i32;
                    x_offset += text_width;
                    x_offset += 20; // gap before next item
                }
            } else if part.contains("deletion") {
                // Extract number and draw in red
                if let Some(space_pos) = part.find(' ') {
                    let num = &part[..space_pos];

                    // Draw "-"
                    draw_text_mut(
                        &mut rgba_image,
                        red,
                        x_offset,
                        info_y,
                        info_scale,
                        &font,
                        "-",
                    );
                    x_offset += 10;

                    // Draw number
                    draw_text_mut(
                        &mut rgba_image,
                        red,
                        x_offset,
                        info_y,
                        info_scale,
                        &font,
                        num,
                    );
                    let text_width = (num.len() as f32 * 10.0) as i32;
                    x_offset += text_width;
                    x_offset += 20; // gap before next item
                }
            }
        }
    }

    Ok(DynamicImage::ImageRgba8(rgba_image))
}
