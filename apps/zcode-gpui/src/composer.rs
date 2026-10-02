//! Single-line composer with IME support (composition via marked text),
//! caret navigation, clipboard paste, and Enter-to-submit.

use gpui::{
    ClickEvent, Context, CursorStyle, EntityInputHandler, EventEmitter, FocusHandle, KeyDownEvent,
    Keystroke, Pixels, Point, Render, UTF16Selection, Window, canvas, div, prelude::*, px, rgb,
};
use std::ops::Range;

pub enum ComposerEvent {
    Submitted,
}

pub struct Composer {
    content: String,
    /// Byte offset into `content`, always on a char boundary.
    caret: usize,
    /// Active IME composition range in UTF-16 code units.
    marked_utf16: Option<Range<usize>>,
    focus: FocusHandle,
}

impl Composer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            content: String::new(),
            caret: 0,
            marked_utf16: None,
            focus: cx.focus_handle(),
        }
    }

    pub fn take_text(&mut self) -> String {
        self.caret = 0;
        self.marked_utf16 = None;
        std::mem::take(&mut self.content)
    }

    pub fn set_text(&mut self, text: &str) {
        self.content = text.to_string();
        self.caret = self.content.len();
        self.marked_utf16 = None;
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    fn sanitize(text: &str) -> String {
        text.chars().filter(|c| *c != '\r').collect()
    }

    fn insert_at_caret(&mut self, text: &str, cx: &mut Context<Self>) {
        let cleaned = Self::sanitize(text);
        if cleaned.is_empty() {
            return;
        }
        self.content.insert_str(self.caret, &cleaned);
        self.caret += cleaned.len();
        cx.notify();
    }

    fn byte_from_utf16(&self, pos: usize) -> Option<usize> {
        let mut units = 0usize;
        for (offset, ch) in self.content.char_indices() {
            if units >= pos {
                return Some(offset);
            }
            units += ch.len_utf16();
        }
        (units >= pos).then_some(self.content.len())
    }

    fn caret_utf16(&self) -> usize {
        self.content[..self.caret]
            .chars()
            .map(char::len_utf16)
            .sum()
    }

    fn move_caret(&mut self, delta: isize, cx: &mut Context<Self>) {
        let bytes = &self.content;
        let new = if delta < 0 {
            bytes[..self.caret]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0)
        } else {
            bytes[self.caret..]
                .chars()
                .next()
                .map(|c| self.caret + c.len_utf8())
                .unwrap_or(self.caret)
        };
        self.caret = new;
        self.marked_utf16 = None;
        cx.notify();
    }

    fn delete_at_caret(&mut self, backward: bool, cx: &mut Context<Self>) {
        if self.marked_utf16.take().is_some() {
            // Composing: let the IME finish; just drop the mark.
            cx.notify();
            return;
        }
        let range = if backward {
            self.content[..self.caret]
                .char_indices()
                .next_back()
                .map(|(i, c)| i..i + c.len_utf8())
        } else {
            self.content[self.caret..]
                .chars()
                .next()
                .map(|c| self.caret..self.caret + c.len_utf8())
        };
        if let Some(r) = range {
            self.content.replace_range(r, "");
            if !backward {
                // caret stays; deleting forward removes the next char
            } else {
                self.caret = self.caret.min(self.content.len());
            }
        }
        cx.notify();
    }

    fn on_key_down(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Keystroke { key, modifiers, .. } = &ev.keystroke;
        let mut handled = true;
        match key.as_str() {
            "enter" => {
                if self.marked_utf16.is_none() && !modifiers.shift {
                    cx.emit(ComposerEvent::Submitted);
                } else if modifiers.shift {
                    self.insert_at_caret("\n", cx);
                } else {
                    handled = false;
                }
            }
            "backspace" => self.delete_at_caret(true, cx),
            "delete" => self.delete_at_caret(false, cx),
            "left" => self.move_caret(-1, cx),
            "right" => self.move_caret(1, cx),
            "home" => {
                self.caret = 0;
                self.marked_utf16 = None;
                cx.notify();
            }
            "end" => {
                self.caret = self.content.len();
                self.marked_utf16 = None;
                cx.notify();
            }
            "v" if modifiers.control || modifiers.platform => {
                if let Some(item) = cx.read_from_clipboard() {
                    let text = item.text().unwrap_or_default();
                    self.marked_utf16 = None;
                    self.insert_at_caret(&text, cx);
                }
            }
            _ => handled = false,
        }
        if handled {
            cx.stop_propagation();
        }
    }
}

impl EventEmitter<ComposerEvent> for Composer {}

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
        let cleaned = Self::sanitize(text);
        self.content.replace_range(range.clone(), &cleaned);
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
        let (byte_range, start_utf16) =
            match range_utf16.or_else(|| self.marked_utf16.clone()).map(|r| {
                let start = self.byte_from_utf16(r.start).unwrap_or(0);
                let end = self.byte_from_utf16(r.end).unwrap_or(self.content.len());
                (start..end, r.start)
            }) {
                Some(v) => v,
                None => ((self.caret..self.caret), self.caret_utf16()),
            };
        let cleaned = Self::sanitize(new_text);
        let marked_len_utf16: usize = cleaned.chars().map(char::len_utf16).sum();
        self.content.replace_range(byte_range, &cleaned);
        self.caret = (self.byte_from_utf16(start_utf16).unwrap_or(0)) + cleaned.len();
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

impl Render for Composer {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let empty = self.content.is_empty();
        let before = self.content[..self.caret].to_string();
        let after = self.content[self.caret..].to_string();
        let entity = cx.entity();
        let focus = self.focus.clone();

        div()
            .id("composer-input")
            .key_context("Composer")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .flex_1()
            .min_w_0()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(rgb(0x232323))
            .border_1()
            .border_color(rgb(0x333333))
            .text_size(px(14.))
            .text_color(rgb(0xececec))
            .on_click(cx.listener(|this, _: &ClickEvent, window, _cx| {
                window.focus(&this.focus);
            }))
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                div()
                    .id("composer-canvas-host")
                    .relative()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .w_full()
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, cx| {
                                window.handle_input(
                                    &focus,
                                    gpui::ElementInputHandler::new(bounds, entity.clone()),
                                    cx,
                                );
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                    .children(empty.then(|| {
                        div()
                            .text_color(rgb(0x777777))
                            .child("Type a message, Enter to send…")
                    })),
            )
            .children((!empty).then(|| {
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .min_w_0()
                    .child(before)
                    .child(
                        div()
                            .w(px(1.5))
                            .h(px(18.))
                            .bg(rgb(0x2dd4bf))
                            .mr(px(0.5))
                            .ml(px(0.5)),
                    )
                    .child(after)
            }))
    }
}
