//! IME (Input Method Editor) and text selection handling for Composer.

use crate::composer::chips::utf16_of;
use crate::composer::input::Composer;
use gpui::{Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};
use std::ops::Range;

impl EntityInputHandler for Composer {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        // Caret-only selection (empty range) in UTF-16 units.
        let pos = self.caret_utf16();
        Some(UTF16Selection {
            range: pos..pos,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_utf16.clone()
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let start = self.byte_from_utf16(range_utf16.start)?;
        let end = self.byte_from_utf16(range_utf16.end)?;
        self.content.get(start..end).map(str::to_string)
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .or_else(|| self.marked_utf16.clone())
            .map(|r| {
                let start = self.byte_from_utf16(r.start).unwrap_or(0);
                let end = self.byte_from_utf16(r.end).unwrap_or(self.content.len());
                start..end
            })
            .unwrap_or(self.caret..self.caret);
        let cleaned = self.sanitize(text);
        // Chip-atomic: a range touching a chip is widened to the whole chip.
        let range = self.edit_range(range, &cleaned);
        self.caret = range.start + cleaned.len();
        self.marked_utf16 = None;
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let byte_range = range_utf16
            .or_else(|| self.marked_utf16.clone())
            .map(|r| {
                let start = self.byte_from_utf16(r.start).unwrap_or(0);
                let end = self.byte_from_utf16(r.end).unwrap_or(self.content.len());
                start..end
            })
            .unwrap_or(self.caret..self.caret);
        let cleaned = self.sanitize(new_text);
        let marked_len_utf16: usize = cleaned.chars().map(char::len_utf16).sum();
        // The marked range is derived from the range actually replaced, so a
        // composition never starts inside (or spans part of) a chip.
        let applied = self.edit_range(byte_range, &cleaned);
        let start_utf16 = utf16_of(&self.content, applied.start);
        self.caret = applied.start + cleaned.len();
        self.marked_utf16 = Some(start_utf16..start_utf16 + marked_len_utf16);
        cx.notify();
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_utf16 = None;
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        element_bounds: gpui::Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<gpui::Bounds<Pixels>> {
        // IME candidate window positioning: anchor to the composer element.
        Some(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
