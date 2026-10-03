//! Golden tests for `shared/markdown/code.rs` (kept out-of-line for the 400-line cap).

use super::*;

#[test]
fn highlights_rust_keywords_and_strings() {
    let lines = highlight("fn main() {\n    let s = \"hi\";\n}\n", "rust");
    assert_eq!(lines.len(), 3);
    let flat: String = lines.iter().flatten().map(|s| s.text.as_str()).collect();
    assert!(flat.contains("fn main()"));
    // Chunks split arbitrarily, so assert on variety of colors in the
    // `let s = "hi";` line rather than on one specific span.
    let colors: std::collections::HashSet<u32> = lines[1].iter().map(|s| s.color).collect();
    assert!(
        colors.len() >= 2,
        "expected syntax coloring, got {colors:?}"
    );
}

#[test]
fn unknown_language_falls_back_to_plain() {
    let lines = highlight("just text\n", "not-a-real-lang");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].len(), 1);
    assert_eq!(lines[0][0].color, crate::shared::theme::TEXT);
    assert_eq!(lines[0][0].text, "just text");
}

#[test]
fn oversized_blocks_skip_highlighting() {
    let big = "x".repeat(130_000);
    let lines = highlight(&big, "rust");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0][0].color, crate::shared::theme::TEXT);
}

#[test]
fn empty_code_yields_one_blank_line() {
    let lines = highlight("", "rust");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0][0].text, "");
}
