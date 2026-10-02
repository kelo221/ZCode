//! Tests for `terminal/io.rs` pure helpers (kept out-of-line for the 400-line cap).

use super::*;

#[test]
fn startup_dsr_is_answered_once_and_stripped() {
    let mut answered = false;
    let mut first = b"hello\x1b[6nworld".to_vec();
    assert!(take_startup_dsr(&mut first, &mut answered));
    assert_eq!(first, b"helloworld".to_vec());
    // Later queries are left in the stream for alacritty to answer with the
    // real cursor position.
    let mut second = b"\x1b[6n".to_vec();
    assert!(!take_startup_dsr(&mut second, &mut answered));
    assert_eq!(second, b"\x1b[6n".to_vec());
}

#[test]
fn startup_dsr_absent_leaves_data_untouched() {
    let mut answered = false;
    let mut data = b"plain output".to_vec();
    assert!(!take_startup_dsr(&mut data, &mut answered));
    assert!(!answered);
    assert_eq!(data, b"plain output".to_vec());
}

#[test]
fn pixel_dims_are_clamped_for_pty() {
    // Tiny / degenerate areas fall back to the minimum grid.
    assert_eq!(dims_for_pixels(0., 0.), TermDims { cols: 20, rows: 8 });
    assert_eq!(dims_for_pixels(f32::NAN, f32::NAN), TermDims { cols: 20, rows: 8 });
    // Huge areas must not overflow PtySize's u16 fields.
    let huge = dims_for_pixels(1.0e9, 1.0e9);
    assert!(huge.cols <= u16::MAX as usize && huge.rows <= u16::MAX as usize);
}
