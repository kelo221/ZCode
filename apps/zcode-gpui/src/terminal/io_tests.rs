//! Tests for `terminal/io.rs` pure helpers (kept out-of-line for the 400-line cap).

use super::*;

#[test]
fn startup_dsr_is_answered_once_and_stripped() {
    let mut dsr = StartupDsr::default();
    let (out, query) = dsr.filter(b"hello[6nworld");
    assert!(query);
    assert_eq!(out, b"helloworld".to_vec());
    // Later queries are left in the stream for alacritty to answer with the
    // real cursor position.
    let (out, query) = dsr.filter(b"[6n");
    assert!(!query);
    assert_eq!(out, b"[6n".to_vec());
}

#[test]
fn startup_dsr_split_across_reads_is_reassembled_and_stripped() {
    for split in 1..4 {
        let mut dsr = StartupDsr::default();
        let full = b"ab[6ncd";
        let (first, q1) = dsr.filter(&full[..2 + split]);
        let (second, q2) = dsr.filter(&full[2 + split..]);
        assert!(!q1 && q2, "split at {split}");
        let mut joined = first;
        joined.extend(second);
        assert_eq!(
            joined,
            b"abcd".to_vec(),
            "no raw escape leaks at split {split}"
        );
    }
}

#[test]
fn queries_after_startup_window_reach_alacritty() {
    let mut dsr = StartupDsr::default();
    let (out, query) = dsr.filter(&vec![b'x'; STARTUP_WINDOW]);
    assert!(!query);
    assert_eq!(out.len(), STARTUP_WINDOW);
    // A program's own query later must not get the fake 1;1 answer.
    let (out, query) = dsr.filter(b"[6n");
    assert!(!query);
    assert_eq!(out, b"[6n".to_vec());
}

#[test]
fn plain_output_and_lone_escape_pass_through() {
    let mut dsr = StartupDsr::default();
    assert_eq!(
        dsr.filter(b"plain output"),
        (b"plain output".to_vec(), false)
    );
    // A trailing ESC is held back, then released with the next read.
    let (out, _) = dsr.filter(b"x");
    assert_eq!(out, b"x".to_vec());
    let (out, _) = dsr.filter(b"[0m");
    assert_eq!(out, b"[0m".to_vec());
}

#[test]
fn pixel_dims_are_clamped_for_pty() {
    // Tiny / degenerate areas fall back to the minimum grid.
    assert_eq!(dims_for_pixels(0., 0.), TermDims { cols: 20, rows: 8 });
    assert_eq!(
        dims_for_pixels(f32::NAN, f32::NAN),
        TermDims { cols: 20, rows: 8 }
    );
    // Huge areas must not overflow PtySize's u16 fields.
    let huge = dims_for_pixels(1.0e9, 1.0e9);
    assert!(huge.cols <= u16::MAX as usize && huge.rows <= u16::MAX as usize);
}
