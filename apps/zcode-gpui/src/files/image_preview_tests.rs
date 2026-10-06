use super::*;
use std::io::Cursor;

fn encoded(format: ImageFormat) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(2, 1, image::Rgba([255, 0, 0, 255]));
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut output, format)
        .unwrap();
    output.into_inner()
}

#[test]
fn static_image_formats_decode_one_bgra_frame_with_visible_dimensions() {
    for format in [
        ImageFormat::Png,
        ImageFormat::Gif,
        ImageFormat::WebP,
        ImageFormat::Bmp,
        ImageFormat::Ico,
    ] {
        let Preview::Image(preview) = decode(encoded(format), format) else {
            panic!("format {format:?}")
        };
        assert_eq!((preview.width, preview.height), (2, 1));
        assert_eq!(preview.image.frame_count(), 1);
        assert_eq!(&preview.image.as_bytes(0).unwrap()[..4], &[0, 0, 255, 255]);
    }
    let rgb = image::RgbImage::from_pixel(2, 1, image::Rgb([255, 0, 0]));
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(rgb)
        .write_to(&mut output, ImageFormat::Jpeg)
        .unwrap();
    assert!(matches!(
        decode(output.into_inner(), ImageFormat::Jpeg),
        Preview::Image(_)
    ));
}

#[test]
fn image_preview_rejects_malformed_payload_and_encoded_dimension_output_bounds() {
    assert!(matches!(
        decode(vec![1, 2, 3], ImageFormat::Png),
        Preview::Error(_)
    ));
    assert!(matches!(
        decode(vec![0; MAX_IMAGE_BYTES as usize + 1], ImageFormat::Png),
        Preview::TooLarge(_)
    ));
    let mut bytes = encoded(ImageFormat::Png);
    bytes[16..20].copy_from_slice(&8193_u32.to_be_bytes());
    let crc = crc32fast::hash(&bytes[12..29]);
    bytes[29..33].copy_from_slice(&crc.to_be_bytes());
    assert!(matches!(decode(bytes, ImageFormat::Png), Preview::Error(_)));
    let mut bytes = encoded(ImageFormat::Png);
    bytes[16..20].copy_from_slice(&4001_u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&4000_u32.to_be_bytes());
    let crc = crc32fast::hash(&bytes[12..29]);
    bytes[29..33].copy_from_slice(&crc.to_be_bytes());
    assert!(matches!(decode(bytes, ImageFormat::Png), Preview::Error(_)));
    assert_eq!(format(Path::new("IMAGE.JPEG")), Some(ImageFormat::Jpeg));
    assert!(format(Path::new("image.svg")).is_none());
}

#[test]
fn animated_gif_preview_decodes_only_the_first_frame() {
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        for pixel in [[255, 0, 0, 255], [0, 255, 0, 255]] {
            encoder
                .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                    2,
                    1,
                    image::Rgba(pixel),
                )))
                .unwrap();
        }
    }
    let Preview::Image(preview) = decode(bytes, ImageFormat::Gif) else {
        panic!("gif")
    };
    assert_eq!(preview.image.frame_count(), 1);
    assert_eq!(&preview.image.as_bytes(0).unwrap()[..4], &[0, 0, 255, 255]);
}

#[test]
fn preview_errors_follow_live_locale_without_storing_untrusted_messages() {
    let _guard = crate::app::test_support::RUNTIME_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    use crate::{
        files::preview::PreviewError,
        shared::i18n::{Locale, current_locale, set_current_locale},
    };
    let previous = current_locale();
    set_current_locale(Locale::EnUs);
    assert_eq!(
        PreviewError::InvalidImage.label(),
        "Image is invalid or exceeds preview limits"
    );
    set_current_locale(Locale::ZhCn);
    assert_eq!(PreviewError::InvalidImage.label(), "图片无效或超出预览限制");
    set_current_locale(previous);
}

#[test]
fn real_local_image_reader_keeps_text_bound_and_image_bytes_separate() {
    let root = std::env::temp_dir().join(uuid::Uuid::now_v7().to_string());
    std::fs::create_dir(&root).unwrap();
    let path = root.join("image.png");
    std::fs::write(&path, encoded(ImageFormat::Png)).unwrap();
    assert!(matches!(
        crate::files::preview::read_preview(&path),
        Preview::Image(_)
    ));
    let text = root.join("large.txt");
    std::fs::write(&text, vec![b'a'; 256 * 1024 + 1]).unwrap();
    assert!(matches!(
        crate::files::preview::read_preview(&text),
        Preview::TooLarge(_)
    ));
    std::fs::remove_dir_all(root).unwrap();
}
