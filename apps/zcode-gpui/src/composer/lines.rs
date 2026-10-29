//! Caret movement by visual (wrapped) line: Home/End go to the edges of the
//! line the caret is drawn on, Up/Down keep the caret's x on the line above
//! or below. All of it reads the last painted text layout.

use crate::composer::input::Composer;
use gpui::{Pixels, Point, TextLayout, px};

/// Index a layout hit-test lands on, accepting both exact and clamped hits.
fn hit(layout: &TextLayout, at: Point<Pixels>) -> usize {
    let (Ok(ix) | Err(ix)) = layout.index_for_position(at);
    ix
}

/// A soft-wrap boundary sits after the space the line broke at; the caret
/// there is drawn at the start of the next line, so End stops before it.
pub(crate) fn end_before_wrap(text: &str, ix: usize) -> usize {
    if ix >= text.len() || text[ix..].starts_with('\n') {
        return ix;
    }
    match text[..ix].chars().next_back() {
        Some(c) if c.is_whitespace() && c != '\n' => ix - c.len_utf8(),
        _ => ix,
    }
}

impl Composer {
    fn painted_layout(&self) -> Option<&TextLayout> {
        self.view
            .layout
            .as_ref()
            .filter(|_| self.view.painted.get())
    }

    /// Raw offset of a displayed layout index, out of any chip.
    fn snap_ix(&self, shown: usize) -> usize {
        let mut ix = self.view.display.to_raw(shown).min(self.content.len());
        while !self.content.is_char_boundary(ix) {
            ix -= 1;
        }
        self.chips.snap(ix)
    }

    /// Start (`end == false`) or end of the visual line holding the caret.
    pub(crate) fn visual_line_edge(&self, end: bool) -> Option<usize> {
        let layout = self.painted_layout()?;
        let caret = layout.position_for_index(self.view.display.to_display(self.caret))?;
        let y = caret.y + layout.line_height() / 2.;
        let bounds = layout.bounds();
        let x = if end {
            bounds.right() + px(10_000.)
        } else {
            bounds.left() - px(1.)
        };
        let ix = hit(layout, Point::new(x, y));
        let ix = if end {
            end_before_wrap(&self.view.display.text, ix)
        } else {
            ix
        };
        Some(self.snap_ix(ix))
    }

    /// Index one visual line up or down at the caret's x; `None` on the
    /// first (up) or last (down) line.
    pub(crate) fn visual_line_step(&self, up: bool) -> Option<usize> {
        let layout = self.painted_layout()?;
        let caret = layout.position_for_index(self.view.display.to_display(self.caret))?;
        let line = layout.line_height();
        let y = caret.y + line / 2. + if up { -line } else { line };
        let bounds = layout.bounds();
        if y < bounds.top() || y > bounds.bottom() {
            return None;
        }
        Some(self.snap_ix(hit(layout, Point::new(caret.x, y))))
    }
}

#[cfg(test)]
#[path = "lines_tests.rs"]
mod tests;
