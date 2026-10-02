//! Golden tests for `terminal/grid.rs` (kept out-of-line for the 400-line cap).

use super::*;

#[test]
fn palette_matches_xterm_layout() {
    assert_eq!(indexed(0), 0x2b303b);
    assert_eq!(indexed(15), 0xeff1f5);
    // 6×6×6 cube corners.
    assert_eq!(indexed(16), 0x000000);
    assert_eq!(indexed(231), 0xffffff);
    // Grayscale ramp start/end.
    assert_eq!(indexed(232), 0x080808);
    assert_eq!(indexed(255), 0xeeeeee);
}

#[test]
fn named_colors_resolve_to_palette_or_default() {
    assert_eq!(resolve_color(Color::Named(NamedColor::Red), 0), 0xbf616a);
    assert_eq!(resolve_color(Color::Named(NamedColor::Foreground), TERM_FG), TERM_FG);
    assert_eq!(resolve_color(Color::Spec(Rgb { r: 0x12, g: 0x34, b: 0x56 }), 0), 0x123456);
}

#[test]
fn key_mapping_covers_shell_needs() {
    assert_eq!(key_to_bytes("enter", false, false, false), b"\r".to_vec());
    assert_eq!(key_to_bytes("backspace", false, false, false), b"\x7f".to_vec());
    assert_eq!(key_to_bytes("up", false, false, false), b"\x1b[A".to_vec());
    assert_eq!(key_to_bytes("left", true, false, false), b"\x1b[1;5D".to_vec());
    assert_eq!(key_to_bytes("c", true, false, false), vec![3]); // Ctrl+C = ETX
    assert_eq!(key_to_bytes("d", true, false, false), vec![4]); // Ctrl+D = EOT
    assert_eq!(key_to_bytes("z", true, false, false), vec![26]); // Ctrl+Z = SUB
    assert_eq!(key_to_bytes("l", true, false, false), vec![12]); // Ctrl+L clears
    assert_eq!(key_to_bytes("a", false, true, false), b"\x1ba".to_vec()); // Alt+a
    assert_eq!(key_to_bytes("x", false, false, false), b"x".to_vec());
    // Unhandled keys pass through empty for app-level shortcuts.
    assert!(key_to_bytes("f5", false, false, false).is_empty());
}

struct Dims(usize, usize);
impl alacritty_terminal::grid::Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.1
    }
    fn screen_lines(&self) -> usize {
        self.1
    }
    fn columns(&self) -> usize {
        self.0
    }
}

fn row_text(row: &[GridRun]) -> String {
    row.iter().map(|r| r.text.as_str()).collect()
}

#[test]
fn scrollback_maps_to_viewport_rows() {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::grid::Scroll;
    use alacritty_terminal::term::{Config, Term};
    use alacritty_terminal::vte::ansi::Processor;

    let mut term = Term::new(Config::default(), &Dims(20, 3), VoidListener);
    let mut parser: Processor = Processor::default();
    parser.advance(&mut term, b"l0\r\nl1\r\nl2\r\nl3\r\nl4");
    term.scroll_display(Scroll::Delta(2));
    let rows = grid_rows(term.renderable_content(), TERM_FG, 0);
    // Scrolled up by two: the viewport shows l0..l2 on separate rows.
    assert_eq!(rows.len(), 3);
    assert_eq!(row_text(&rows[0]), "l0");
    assert_eq!(row_text(&rows[1]), "l1");
    assert_eq!(row_text(&rows[2]), "l2");
}

#[test]
fn wide_char_spacers_are_skipped() {
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::{Config, Term};
    use alacritty_terminal::vte::ansi::Processor;

    let mut term = Term::new(Config::default(), &Dims(20, 2), VoidListener);
    let mut parser: Processor = Processor::default();
    parser.advance(&mut term, "中x\r\n".as_bytes());
    let rows = grid_rows(term.renderable_content(), TERM_FG, 0);
    assert_eq!(row_text(&rows[0]), "中x");
}
