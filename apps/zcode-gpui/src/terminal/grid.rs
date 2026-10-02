//! Pure terminal-grid helpers: xterm-256 palette resolution, grid → styled
//! text runs, and keystroke → PTY byte mapping (PARITY.md M3
//! "portable-pty + alacritty_terminal rendered in gpui").

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::RenderableContent;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};

/// base16-ocean.dark ANSI palette (matches the syntect code theme).
const PALETTE: [u32; 16] = [
    0x2b303b, 0xbf616a, 0xa3be8c, 0xebcb8b, 0x8fa1b3, 0xb48ead, 0x96b5b4, 0xc0c5ce, 0x65737e,
    0xbf616a, 0xa3be8c, 0xebcb8b, 0x8fa1b3, 0xb48ead, 0x96b5b4, 0xeff1f5,
];
pub const TERM_FG: u32 = 0xc0c5ce;

/// Resolve a terminal cell color to an RGB token using the base palette
/// (per-session VT color overrides are folded in by the caller when set).
pub fn resolve_color(color: Color, default: u32) -> u32 {
    match color {
        Color::Named(n) => named_color(n, default),
        Color::Spec(Rgb { r, g, b }) => ((r as u32) << 16) | ((g as u32) << 8) | b as u32,
        Color::Indexed(i) => indexed(i),
    }
}

fn named_color(n: NamedColor, default: u32) -> u32 {
    use NamedColor::*;
    match n {
        Foreground | Background | BrightForeground | DimForeground => default,
        Cursor => 0xffffff,
        DimBlack => 0x4f5b66,
        DimWhite => 0x65737e,
        DimRed => 0xa3575e,
        DimGreen => 0x8ba47a,
        DimYellow => 0xc7b183,
        DimBlue => 0x7788a0,
        DimMagenta => 0x9a7d94,
        DimCyan => 0x82a2a0,
        BrightBlack => PALETTE[8],
        BrightRed => PALETTE[9],
        BrightGreen => PALETTE[10],
        BrightYellow => PALETTE[11],
        BrightBlue => PALETTE[12],
        BrightMagenta => PALETTE[13],
        BrightCyan => PALETTE[14],
        BrightWhite => PALETTE[15],
        Black => PALETTE[0],
        Red => PALETTE[1],
        Green => PALETTE[2],
        Yellow => PALETTE[3],
        Blue => PALETTE[4],
        Magenta => PALETTE[5],
        Cyan => PALETTE[6],
        White => PALETTE[7],
    }
}

/// xterm 256-color: 16 base + 6×6×6 cube + 24 grayscale.
pub fn indexed(i: u8) -> u32 {
    if i < 16 {
        return PALETTE[i as usize];
    }
    if i < 232 {
        let i = (i - 16) as u32;
        let steps = [0u32, 95, 135, 175, 215, 255];
        let (r, rest) = (i / 36, i % 36);
        let (g, b) = (rest / 6, rest % 6);
        (steps[r as usize] << 16) | (steps[g as usize] << 8) | steps[b as usize]
    } else {
        let v = 8 + (i as u32 - 232) * 10;
        (v << 16) | (v << 8) | v
    }
}

/// A run of consecutive cells sharing the same styling.
#[derive(Clone, Debug, PartialEq)]
pub struct GridRun {
    pub text: String,
    pub fg: u32,
    pub bg: Option<u32>,
    pub bold: bool,
    pub italic: bool,
}

struct RunStyle {
    fg: u32,
    bg: Option<u32>,
    bold: bool,
    italic: bool,
}

/// Convert the terminal's renderable viewport into styled rows (one Vec per
/// screen line, viewport-relative). Cursor cell renders inverted; trailing
/// unstyled spaces are trimmed.
pub fn grid_rows(content: RenderableContent, default_fg: u32, default_bg: u32) -> Vec<Vec<GridRun>> {
    let cursor = &content.cursor.point;
    let cursor_visible = content.cursor.shape != CursorShape::Hidden;
    // Grid lines are negative while scrolled into history; shift them back
    // to viewport rows (otherwise every history line collapses onto row 0).
    let offset = content.display_offset as i32;
    let mut rows: Vec<Vec<GridRun>> = Vec::new();
    for indexed in content.display_iter {
        let Ok(row) = usize::try_from(indexed.point.line.0 + offset) else {
            continue;
        };
        while rows.len() <= row {
            rows.push(Vec::new());
        }
        let cell = indexed.cell;
        // The trailing half of a double-width glyph carries a placeholder
        // space; rendering it would push the rest of the row right.
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let is_cursor = cursor_visible
            && indexed.point.line.0 == cursor.line.0
            && indexed.point.column.0 == cursor.column.0;
        let fg = resolve_color(cell.fg, default_fg);
        let bg = (cell.bg != Color::Named(NamedColor::Background))
            .then(|| resolve_color(cell.bg, default_bg));
        let (fg, bg) = if is_cursor {
            let bg = bg.unwrap_or(default_bg);
            (bg, Some(fg))
        } else {
            (fg, bg)
        };
        let style = RunStyle {
            fg,
            bg,
            bold: cell.flags.contains(Flags::BOLD),
            italic: cell.flags.contains(Flags::ITALIC),
        };
        let row_runs = &mut rows[row];
        if let Some(last) = row_runs.last_mut()
            && last.fg == style.fg
            && last.bg == style.bg
            && last.bold == style.bold
            && last.italic == style.italic
        {
            last.text.push(cell.c);
            continue;
        }
        row_runs.push(GridRun {
            text: cell.c.to_string(),
            fg: style.fg,
            bg: style.bg,
            bold: style.bold,
            italic: style.italic,
        });
    }
    for row in &mut rows {
        if let Some(last) = row.last_mut()
            && last.bg.is_none()
        {
            let trimmed = last.text.trim_end().to_string();
            if trimmed.is_empty() {
                row.pop();
            } else {
                last.text = trimmed;
            }
        }
    }
    rows
}

/// Map a gpui keystroke to PTY bytes. Returns an empty vec for unhandled
/// keys (letting chat shortcuts pass through).
pub fn key_to_bytes(key: &str, ctrl: bool, alt: bool, shift: bool) -> Vec<u8> {
    let seq = |s: &str| {
        let mut v = b"\x1b".to_vec();
        v.extend_from_slice(s.as_bytes());
        v
    };
    match key {
        "enter" => return b"\r".to_vec(),
        "backspace" => return b"\x7f".to_vec(),
        "tab" => return if shift { seq("[Z") } else { b"\t".to_vec() },
        "escape" => return b"\x1b".to_vec(),
        "up" => return seq(if ctrl { "[1;5A" } else { "[A" }),
        "down" => return seq(if ctrl { "[1;5B" } else { "[B" }),
        "right" => return seq(if ctrl { "[1;5C" } else { "[C" }),
        "left" => return seq(if ctrl { "[1;5D" } else { "[D" }),
        "home" => return seq("[H"),
        "end" => return seq("[F"),
        "pageup" => return seq("[5~"),
        "pagedown" => return seq("[6~"),
        "delete" => return seq("[3~"),
        _ => {}
    }
    let chars: Vec<char> = key.chars().collect();
    if ctrl && chars.len() == 1 {
        let c = chars[0].to_ascii_lowercase();
        if c.is_ascii_lowercase() {
            return vec![(c as u8) - b'a' + 1];
        }
        if c == ' ' {
            return vec![0];
        }
        return Vec::new();
    }
    if chars.len() != 1 {
        return Vec::new();
    }
    let mut out = Vec::new();
    if alt {
        out.push(0x1b);
    }
    let mut buf = [0u8; 4];
    out.extend_from_slice(chars[0].encode_utf8(&mut buf).as_bytes());
    out
}

#[cfg(test)]
#[path = "grid_tests.rs"]
mod tests;
