use crate::files::preview::Preview;
use image::{DynamicImage, Frame, ImageDecoder, ImageFormat, ImageReader, Limits};
use std::{io::Cursor, path::Path, sync::Arc};

pub(crate) const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_PIXELS: u64 = 16_000_000;
const MAX_SIDE: u32 = 8192;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImagePreview {
    pub image: Arc<gpui::RenderImage>,
    pub width: u32,
    pub height: u32,
}

pub(crate) fn format(path: &Path) -> Option<ImageFormat> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some(ImageFormat::Png),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "gif" => Some(ImageFormat::Gif),
        "webp" => Some(ImageFormat::WebP),
        "bmp" => Some(ImageFormat::Bmp),
        "ico" => Some(ImageFormat::Ico),
        _ => None,
    }
}

pub(crate) fn decode(bytes: Vec<u8>, format: ImageFormat) -> Preview {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Preview::TooLarge(bytes.len() as u64);
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let Ok(mut decoder) = reader.into_decoder() else {
        return invalid();
    };
    let (width, height) = decoder.dimensions();
    let pixels = u64::from(width) * u64::from(height);
    if width == 0
        || height == 0
        || width > MAX_SIDE
        || height > MAX_SIDE
        || pixels > MAX_PIXELS
        || decoder.total_bytes() > 64 * 1024 * 1024
    {
        return invalid();
    }
    let Ok(orientation) = decoder.orientation() else {
        return invalid();
    };
    // 不走 GPUI 的全 GIF 帧收集；只解一帧并检查尺寸/输出，避免动画把预览内存放大。
    let Ok(mut decoded) = DynamicImage::from_decoder(decoder) else {
        return invalid();
    };
    decoded.apply_orientation(orientation);
    let (width, height) = (decoded.width(), decoded.height());
    let mut rgba = decoded.into_rgba8();
    if rgba.len() as u64 > MAX_PIXELS * 4 {
        return invalid();
    }
    for pixel in rgba.as_mut().as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Preview::Image(ImagePreview {
        image: Arc::new(gpui::RenderImage::new(vec![Frame::new(rgba)])),
        width,
        height,
    })
}

fn invalid() -> Preview {
    Preview::Error(crate::files::preview::PreviewError::InvalidImage)
}

pub(crate) fn element(preview: &ImagePreview) -> gpui::AnyElement {
    use crate::shared::{
        i18n::label,
        theme::{MUTED, ui_size},
        theme_colors::color,
    };
    use gpui::{IntoElement, ParentElement, Styled, StyledImage, div, img, px};
    div()
        .flex()
        .flex_col()
        .gap_2()
        .p_2()
        .min_w_0()
        .child(
            div()
                .text_size(px(ui_size(12.)))
                .text_color(color(MUTED))
                .child(format!(
                    "{} × {} · {}",
                    preview.width,
                    preview.height,
                    label("Static preview", "静态预览")
                )),
        )
        .child(
            img(gpui::ImageSource::Render(preview.image.clone()))
                .w_full()
                .h(px(320.))
                .object_fit(gpui::ObjectFit::Contain),
        )
        .into_any_element()
}

#[cfg(test)]
#[path = "image_preview_tests.rs"]
mod tests;
