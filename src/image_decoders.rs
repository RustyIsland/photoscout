use image::RgbaImage;
use std::fs;
use std::path::Path;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodePurpose {
    Preview,
    Final,
}

impl DecodePurpose {
    pub fn label(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Final => "final",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderKind {
    ZuneJpeg,
    ImageCrateGeneric,
    ImageCrateFallback,
}

impl DecoderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ZuneJpeg => "zune_jpeg",
            Self::ImageCrateGeneric => "image_crate_generic",
            Self::ImageCrateFallback => "image_crate_fallback",
        }
    }
}

#[derive(Debug)]
pub struct DecodedImage {
    pub image: RgbaImage,
    pub decoder: DecoderKind,
    pub fallback_used: bool,
}

pub fn decode_for_thumbnail(
    path: &Path,
    extension: &str,
    purpose: DecodePurpose,
) -> Result<DecodedImage, String> {
    let extension = extension.to_ascii_lowercase();

    match extension.as_str() {
        "jpg" | "jpeg" => decode_jpeg_with_fallback(path, purpose),
        _ => decode_with_image_crate(path, DecoderKind::ImageCrateGeneric),
    }
}

fn decode_jpeg_with_fallback(path: &Path, _purpose: DecodePurpose) -> Result<DecodedImage, String> {
    match decode_jpeg_zune(path) {
        Ok(decoded) => Ok(decoded),
        Err(primary_error) => {
            let mut fallback = decode_with_image_crate(path, DecoderKind::ImageCrateFallback)
                .map_err(|fallback_error| {
                    format!(
                        "zune-jpeg failed: {primary_error}; image crate fallback failed: {fallback_error}"
                    )
                })?;
            fallback.fallback_used = true;
            Ok(fallback)
        }
    }
}

fn decode_jpeg_zune(path: &Path) -> Result<DecodedImage, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;

    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::RGBA);
    let cursor = ZCursor::new(bytes.as_slice());
    let mut decoder = JpegDecoder::new_with_options(cursor, options);

    let pixels = decoder.decode().map_err(|error| error.to_string())?;
    let (width, height) = decoder
        .dimensions()
        .ok_or_else(|| "zune-jpeg did not report decoded dimensions".to_string())?;

    let image = RgbaImage::from_raw(width as u32, height as u32, pixels)
        .ok_or_else(|| "zune-jpeg output buffer did not match RGBA dimensions".to_string())?;

    Ok(DecodedImage {
        image,
        decoder: DecoderKind::ZuneJpeg,
        fallback_used: false,
    })
}

fn decode_with_image_crate(path: &Path, decoder: DecoderKind) -> Result<DecodedImage, String> {
    image::open(path)
        .map_err(|error| error.to_string())
        .map(|image| DecodedImage {
            image: image.to_rgba8(),
            decoder,
            fallback_used: matches!(decoder, DecoderKind::ImageCrateFallback),
        })
}
