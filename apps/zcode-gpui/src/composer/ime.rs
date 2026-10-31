//! IME (Input Method Editor) and text selection handling for Composer.

use crate::composer::chips::utf16_of;
use crate::composer::input::Composer;
use gpui::{Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};
use std::ops::Range;

impl Composer {
    /// Text in a UTF-16 range clamped to the content, plus the range served.
    // Android 的 IME 桥会用 0..1 探测输入类型、用光标 ±256 拉取镜像文本；越界时返回 None
    // 会让空输入框被当成终端（关闭联想），镜像文本变空导致 Gboard 误判句首而自动大写。
    // 与 TextInput/CodeEditor 一致：越界部分截断，而不是返回 None。
    pub(crate) fn clamped_text_utf16(&self, range_utf16: Range<usize>) -> (String, Range<usize>) {
        let total = utf16_of(&self.content, self.content.len());
        let end = range_utf16.end.min(total);
        let start = range_utf16.start.min(end);
        let start_byte = self.byte_from_utf16(start).unwrap_or(self.content.len());
        let end_byte = self.byte_from_utf16(end).unwrap_or(self.content.len());
        let text = self
            .content
            .get(start_byte..end_byte)
            .unwrap_or_default()
            .to_string();
        (text, start..end)
    }
}

impl EntityInputHandler for Composer {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let caret = self.caret_utf16();
        let anchor = self
            .selection()
            .map(|sel| {
                utf16_of(
                    &self.content,
                    if sel.start == self.caret {
                        sel.end
                    } else {
                        sel.start
                    },
                )
            })
            .unwrap_or(caret);
        Some(UTF16Selection {
            range: anchor.min(caret)..anchor.max(caret),
            reversed: caret < anchor,
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
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let (text, actual) = self.clamped_text_utf16(range_utf16);
        *adjusted_range = Some(actual);
        Some(text)
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
            .or_else(|| self.selection())
            .unwrap_or(self.caret..self.caret);
        let cleaned = self.sanitize(text);
        self.anchor = None;
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
            .or_else(|| self.selection())
            .unwrap_or(self.caret..self.caret);
        let cleaned = self.sanitize(new_text);
        self.anchor = None;
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
        // IME candidate window: at the painted caret, else the whole element.
        Some(self.view.caret_bounds.get().unwrap_or(element_bounds))
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

#[cfg(test)]
#[path = "ime_tests.rs"]
mod tests;
