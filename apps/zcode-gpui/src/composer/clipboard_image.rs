//! Clipboard images → a format every model provider accepts.
//!
//! Windows clipboards usually carry the same picture several times (PNG plus a
//! device-independent bitmap that GPUI surfaces as BMP). Providers only accept
//! PNG, JPEG, WebP and GIF image inputs; an `image/bmp` block in the request is
//! rejected with a 400, and because it lives in the persisted user message the
//! session can never send again.

use gpui::{Image, ImageFormat};

/// Image formats the model APIs accept as-is.
pub(crate) fn is_model_format(format: ImageFormat) -> bool {
    matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::Webp | ImageFormat::Gif
    )
}

/// One paste yields one attachment: prefer an entry the model accepts as-is,
/// otherwise the first non-empty one (transcoded later by [`to_model_image`]).
pub(crate) fn pick<'a>(images: impl IntoIterator<Item = &'a Image>) -> Option<&'a Image> {
    let mut fallback = None;
    for image in images.into_iter().filter(|i| !i.bytes.is_empty()) {
        if is_model_format(image.format) {
            return Some(image);
        }
        fallback.get_or_insert(image);
    }
    fallback
}

/// Returns the bytes unchanged for model formats; decodes BMP/ICO/TIFF/PNM and
/// re-encodes them as PNG. SVG has no rasterizer here and is rejected.
pub(crate) fn to_model_image(
    bytes: Vec<u8>,
    format: ImageFormat,
) -> Result<(Vec<u8>, ImageFormat), String> {
    if is_model_format(format) {
        return Ok((bytes, format));
    }
    let source = match format {
        ImageFormat::Bmp => image::ImageFormat::Bmp,
        ImageFormat::Ico => image::ImageFormat::Ico,
        ImageFormat::Tiff => image::ImageFormat::Tiff,
        ImageFormat::Pnm => image::ImageFormat::Pnm,
        _ => return Err(unsupported(format)),
    };
    let decoded =
        image::load_from_memory_with_format(&bytes, source).map_err(|_| unsupported(format))?;
    let mut rgba = decoded.to_rgba8();
    // 32 位 DIB 常把 alpha 通道整体写 0（GDI 不使用 alpha），原样转 PNG 会得到全透明图片。
    if rgba.pixels().all(|p| p[3] == 0) {
        rgba.pixels_mut().for_each(|p| p[3] = u8::MAX);
    }
    let mut png = Vec::new();
    rgba.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|_| unsupported(format))?;
    Ok((png, ImageFormat::Png))
}

fn unsupported(format: ImageFormat) -> String {
    format!(
        "Clipboard image ({}) could not be converted to PNG",
        format.mime_type()
    )
}

#[cfg(test)]
#[path = "clipboard_image_tests.rs"]
mod tests;
