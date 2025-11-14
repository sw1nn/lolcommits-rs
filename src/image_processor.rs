use crate::error::Result;
use ab_glyph::{FontRef, PxScale};
use image::{DynamicImage, Rgba};
use imageproc::drawing::{draw_filled_rect_mut, draw_text_mut};
use imageproc::rect::Rect;

const FONT_DATA: &[u8] = include_bytes!("/usr/share/fonts/TTF/InputSansCompressedNerdFont-Bold.ttf");

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
        Rect::at(0, y_start as i32).of_size(width, chyron_height),
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
