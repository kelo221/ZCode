use super::*;

pub(crate) fn bmp_bytes(alpha: u8) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, alpha]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Bmp)
        .unwrap();
    out
}

#[test]
fn model_formats_pass_through_untouched() {
    let (bytes, format) = to_model_image(vec![1, 2, 3], ImageFormat::Jpeg).unwrap();
    assert_eq!((bytes, format), (vec![1, 2, 3], ImageFormat::Jpeg));
}

#[test]
fn bmp_is_transcoded_to_png() {
    let (bytes, format) = to_model_image(bmp_bytes(255), ImageFormat::Bmp).unwrap();
    assert_eq!(format, ImageFormat::Png);
    let decoded = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .unwrap()
        .to_rgba8();
    assert_eq!(decoded.dimensions(), (3, 2));
    assert_eq!(decoded.get_pixel(0, 0).0, [10, 20, 30, 255]);
}

#[test]
fn all_zero_alpha_dib_becomes_opaque() {
    let (bytes, _) = to_model_image(bmp_bytes(0), ImageFormat::Bmp).unwrap();
    let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert!(decoded.pixels().all(|p| p[3] == 255));
}

#[test]
fn undecodable_and_svg_inputs_are_rejected() {
    assert!(to_model_image(vec![1, 2, 3], ImageFormat::Bmp).is_err());
    assert!(to_model_image(b"<svg/>".to_vec(), ImageFormat::Svg).is_err());
}

#[test]
fn pick_prefers_model_format_and_skips_empty_entries() {
    let bmp = Image::from_bytes(ImageFormat::Bmp, vec![1]);
    let empty_png = Image::from_bytes(ImageFormat::Png, vec![]);
    let png = Image::from_bytes(ImageFormat::Png, vec![2]);
    assert_eq!(
        pick([&bmp, &empty_png, &png]).unwrap().format,
        ImageFormat::Png
    );
    assert_eq!(pick([&empty_png, &bmp]).unwrap().format, ImageFormat::Bmp);
    assert!(pick([&empty_png]).is_none());
}
