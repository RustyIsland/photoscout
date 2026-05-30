use eframe::egui;
use fast_image_resize::images::Image as FirImage;
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::{imageops::FilterType as ImageFilterType, RgbaImage};

pub(super) fn make_preview_thumbnail(source: &RgbaImage, max_edge: u32) -> Result<egui::ColorImage, String> {
    let (width, height) = fit_dimensions(source.width(), source.height(), max_edge);
    let resized = image::imageops::resize(source, width, height, ImageFilterType::Nearest);
    rgba_to_color_image(&resized)
}

pub(super) fn make_final_thumbnail_fast(source: &RgbaImage, max_edge: u32) -> Result<egui::ColorImage, String> {
    let (width, height) = fit_dimensions(source.width(), source.height(), max_edge);

    let src_image = FirImage::from_vec_u8(
        source.width(),
        source.height(),
        source.clone().into_raw(),
        PixelType::U8x4,
    )
    .map_err(|error| error.to_string())?;

    let mut dst_image = FirImage::new(width, height, PixelType::U8x4);
    let options = ResizeOptions::new()
        .resize_alg(ResizeAlg::Convolution(FilterType::Bilinear))
        .use_alpha(true);

    let mut resizer = Resizer::new();
    resizer
        .resize(&src_image, &mut dst_image, &options)
        .map_err(|error| error.to_string())?;

    let output = RgbaImage::from_raw(width, height, dst_image.into_vec())
        .ok_or_else(|| "failed to build resized thumbnail buffer".to_string())?;

    rgba_to_color_image(&output)
}

fn rgba_to_color_image(image: &RgbaImage) -> Result<egui::ColorImage, String> {
    let size = [image.width() as usize, image.height() as usize];
    let pixels = image.as_flat_samples();
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice()))
}

fn fit_dimensions(width: u32, height: u32, max_edge: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (max_edge, max_edge);
    }

    let scale = (max_edge as f32 / width as f32).min(max_edge as f32 / height as f32);
    let new_width = ((width as f32 * scale).round() as u32).max(1);
    let new_height = ((height as f32 * scale).round() as u32).max(1);
    (new_width, new_height)
}
