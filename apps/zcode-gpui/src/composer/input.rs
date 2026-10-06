//! Single-line composer with IME support (composition via marked text),
//! caret navigation, clipboard paste, and Enter-to-submit.

use crate::composer::attachment::AttachmentRef;
use crate::composer::chips::{self, ChipTable};
use crate::shared::theme::ui_size;
use crate::shared::theme_colors::color as rgb;
use gpui::{
    ClickEvent, Context, CursorStyle, EventEmitter, FocusHandle, KeyDownEvent, Keystroke, Render,
    Window, canvas, div, prelude::*, px,
};
use std::ops::Range;
use std::path::PathBuf;

pub enum ComposerEvent {
    Submitted(crate::composer::delivery::SubmitTrigger),
    ImagePasted {
        bytes: Vec<u8>,
        format: gpui::ImageFormat,
        replacement: u64,
    },
}

pub struct Composer {
    pub(crate) content: String,
    pub(crate) replacement_generation: u64,
    pub(crate) caret: usize,
    pub(crate) marked_utf16: Option<Range<usize>>,
    pub(crate) focus: FocusHandle,
    pub(crate) single_line: bool,
    pub(crate) placeholder: &'static str,
    pub(crate) attachments: Vec<AttachmentRef>,
    pub(crate) temp_owned: Vec<PathBuf>,
    pub(crate) chips: ChipTable,
}

impl Composer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            content: String::new(),
            replacement_generation: 0,
            caret: 0,
            marked_utf16: None,
            focus: cx.focus_handle(),
            single_line: false,
            placeholder: "Ask for follow-up changes",
            attachments: Vec::new(),
            temp_owned: Vec::new(),
            chips: ChipTable::default(),
        }
    }

    pub fn new_single_line(placeholder: &'static str, cx: &mut Context<Self>) -> Self {
        Self {
            single_line: true,
            placeholder,
            ..Self::new(cx)
        }
    }

    pub fn take_text(&mut self) -> String {
        self.replacement_generation = self
            .replacement_generation
            .checked_add(1)
            .expect("composer generation exhausted");
        self.caret = 0;
        self.marked_utf16 = None;
        self.chips.clear();
        std::mem::take(&mut self.content)
    }

    pub fn take_attachments(&mut self) -> Vec<AttachmentRef> {
        std::mem::take(&mut self.attachments)
    }

    pub fn drain_temp_ownership(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.temp_owned)
    }

    pub(crate) fn take_submission(&mut self) -> (String, Vec<AttachmentRef>, Vec<PathBuf>) {
        (
            self.take_text(),
            self.take_attachments(),
            self.drain_temp_ownership(),
        )
    }

    pub(crate) fn restore_submission(
        &mut self,
        text: String,
        attachments: Vec<AttachmentRef>,
        temps: Vec<PathBuf>,
    ) {
        self.set_text(&text);
        self.attachments = attachments;
        self.temp_owned = temps;
    }

    pub fn attachments(&self) -> &[AttachmentRef] {
        &self.attachments
    }

    pub fn set_text(&mut self, text: &str) {
        self.replacement_generation = self
            .replacement_generation
            .checked_add(1)
            .expect("composer generation exhausted");
        self.content = text.to_string();
        self.caret = self.content.len();
        self.marked_utf16 = None;
        self.chips.clear();
    }

    pub fn insert_chip(&mut self, range: Range<usize>, chip: &str) {
        self.marked_utf16 = None;
        self.caret = chips::insert_chip(&mut self.content, &mut self.chips, range, chip);
    }

    pub(crate) fn edit_range(&mut self, range: Range<usize>, text: &str) -> Range<usize> {
        chips::edit(&mut self.content, &mut self.chips, range, text)
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub(crate) fn sanitize(&self, text: &str) -> String {
        text.chars()
            .filter(|c| *c != '\r' && (!self.single_line || *c != '\n'))
            .collect()
    }

    fn insert_at_caret(&mut self, text: &str, cx: &mut Context<Self>) {
        let cleaned = self.sanitize(text);
        if cleaned.is_empty() {
            return;
        }
        let r = self.edit_range(self.caret..self.caret, &cleaned);
        self.caret = r.start + cleaned.len();
        cx.notify();
    }

    pub(crate) fn byte_from_utf16(&self, pos: usize) -> Option<usize> {
        let mut units = 0usize;
        for (offset, ch) in self.content.char_indices() {
            if units >= pos {
                return Some(offset);
            }
            units += ch.len_utf16();
        }
        (units >= pos).then_some(self.content.len())
    }

    pub(crate) fn caret_utf16(&self) -> usize {
        self.content[..self.caret]
            .chars()
            .map(char::len_utf16)
            .sum()
    }

    fn move_caret(&mut self, delta: isize, cx: &mut Context<Self>) {
        let bytes = &self.content;
        let new = if delta < 0 {
            self.chips.chip_ending_at(self.caret).unwrap_or_else(|| {
                bytes[..self.caret]
                    .char_indices()
                    .next_back()
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            })
        } else {
            self.chips.chip_starting_at(self.caret).unwrap_or_else(|| {
                bytes[self.caret..]
                    .chars()
                    .next()
                    .map(|c| self.caret + c.len_utf8())
                    .unwrap_or(self.caret)
            })
        };
        self.caret = self.chips.snap(new);
        self.marked_utf16 = None;
        cx.notify();
    }

    fn delete_at_caret(&mut self, backward: bool, cx: &mut Context<Self>) {
        if self.marked_utf16.take().is_some() {
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
            let removed = self.edit_range(r, "");
            self.caret = removed.start;
        }
        cx.notify();
    }

    fn on_key_down(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let Keystroke { key, modifiers, .. } = &ev.keystroke;
        let mut handled = true;
        match key.as_str() {
            "enter" => {
                if self.marked_utf16.is_none() && !modifiers.shift {
                    let trigger = if !self.single_line
                        && crate::composer::delivery::primary_modifier(modifiers)
                    {
                        crate::composer::delivery::SubmitTrigger::ModifiedEnter
                    } else {
                        crate::composer::delivery::SubmitTrigger::Ordinary
                    };
                    cx.emit(ComposerEvent::Submitted(trigger));
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
            "v" if modifiers.control || modifiers.platform => self.paste_clipboard(cx),
            _ => handled = false,
        }
        if handled {
            cx.stop_propagation();
        }
    }

    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let mut has_image = false;
        for entry in item.entries() {
            if let gpui::ClipboardEntry::Image(img) = entry
                && !img.bytes.is_empty()
            {
                has_image = true;
                cx.emit(ComposerEvent::ImagePasted {
                    bytes: img.bytes.clone(),
                    format: img.format,
                    replacement: self.replacement_generation,
                });
            }
        }
        if !has_image {
            self.marked_utf16 = None;
            self.insert_at_caret(&item.text().unwrap_or_default(), cx);
        }
    }
}

impl EventEmitter<ComposerEvent> for Composer {}

impl crate::app::root::RootView {
    pub(crate) fn wire_submit_events(
        composer: &gpui::Entity<Composer>,
        commit_input: &gpui::Entity<Composer>,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe(
            composer,
            |this, _composer, ev: &ComposerEvent, cx| match ev {
                ComposerEvent::Submitted(trigger)
                    if *trigger == crate::composer::delivery::SubmitTrigger::Ordinary =>
                {
                    this.submit(cx)
                }
                ComposerEvent::Submitted(trigger) => this
                    .state
                    .update(cx, |s, cx| s.submit_composer_with_trigger(*trigger, cx)),
                ComposerEvent::ImagePasted {
                    bytes,
                    format,
                    replacement,
                } => this.state.update(cx, |s, cx| {
                    s.start_image_upload(bytes.clone(), *format, *replacement, cx)
                }),
            },
        )
        .detach();
        cx.subscribe(commit_input, |this, _composer, ev: &ComposerEvent, cx| {
            if matches!(ev, ComposerEvent::Submitted(_)) {
                this.do_commit(cx);
            }
        })
        .detach();
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
            .w_full()
            .min_w_0()
            .min_h(px(44.))
            .px_1()
            .text_size(px(ui_size(14.)))
            .text_color(rgb(crate::shared::theme::TEXT))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                window.focus(&this.focus, cx);
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
                        div().text_color(rgb(crate::shared::theme::MUTED)).child(
                            if self.single_line {
                                self.placeholder
                            } else {
                                crate::shared::i18n::label(
                                    "Ask for follow-up changes",
                                    "请求后续修改",
                                )
                            },
                        )
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
