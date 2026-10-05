//! Single-line composer with IME support (composition via marked text),
//! caret navigation, clipboard paste, and Enter-to-submit.

use crate::composer::attachment::AttachmentRef;
use crate::composer::chips::{self, ChipTable};
use gpui::{
    ClickEvent, Context, CursorStyle, EventEmitter, FocusHandle, KeyDownEvent, Keystroke, Render,
    Window, canvas, div, prelude::*, px, rgb,
};
use std::ops::Range;
use std::path::PathBuf;

pub enum ComposerEvent {
    Submitted,
}

pub struct Composer {
    pub(crate) content: String,
    /// Byte offset into `content`, always on a char boundary.
    pub(crate) caret: usize,
    /// Active IME composition range in UTF-16 code units.
    pub(crate) marked_utf16: Option<Range<usize>>,
    pub(crate) focus: FocusHandle,
    /// Single-line mode (commit box): Enter submits, newlines are stripped.
    pub(crate) single_line: bool,
    pub(crate) placeholder: &'static str,
    pub(crate) attachments: Vec<AttachmentRef>,
    /// Temp files this composer created for pasted images (owned: deleted on
    /// remove; retired to the app on submit; 2026-10-05 audit P1.7).
    pub(crate) temp_owned: Vec<PathBuf>,
    /// Atomic mention/skill chips (byte ranges into `content`).
    pub(crate) chips: ChipTable,
}

impl Composer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            content: String::new(),
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

    /// One-line variant reused for the commit-message input.
    pub fn new_single_line(placeholder: &'static str, cx: &mut Context<Self>) -> Self {
        Self {
            single_line: true,
            placeholder,
            ..Self::new(cx)
        }
    }

    pub fn take_text(&mut self) -> String {
        self.caret = 0;
        self.marked_utf16 = None;
        self.chips.clear();
        std::mem::take(&mut self.content)
    }

    pub fn take_attachments(&mut self) -> Vec<AttachmentRef> {
        std::mem::take(&mut self.attachments)
    }

    /// Hand over ownership of the temp files behind pasted images (call
    /// alongside `take_attachments` on submit): the caller retires them so
    /// they stay readable for the backend until app quit.
    pub fn drain_temp_ownership(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.temp_owned)
    }

    pub fn attachments(&self) -> &[AttachmentRef] {
        &self.attachments
    }

    /// True when the attachment's reference is a temp file this composer
    /// created (content-hash name under the owned prefix).
    fn is_owned_temp(att: &AttachmentRef) -> bool {
        crate::shared::temp_attachments::is_owned_path(&att.reference)
    }

    pub fn add_attachment(&mut self, att: AttachmentRef, cx: &mut Context<Self>) {
        let owned = Self::is_owned_temp(&att);
        if !self
            .attachments
            .iter()
            .any(|a| a.reference == att.reference)
        {
            if owned {
                self.temp_owned.push(PathBuf::from(&att.reference));
            }
            self.attachments.push(att);
            cx.notify();
        }
    }

    pub fn remove_attachment(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.attachments.len() {
            let removed = self.attachments.remove(index);
            // A pasted-image temp file we created dies with the attachment.
            if Self::is_owned_temp(&removed) {
                self.temp_owned
                    .retain(|p| p.as_os_str() != removed.reference.as_str());
                crate::shared::temp_attachments::delete_owned(std::path::Path::new(
                    &removed.reference,
                ));
            }
            cx.notify();
        }
    }

    pub fn set_text(&mut self, text: &str) {
        self.content = text.to_string();
        self.caret = self.content.len();
        self.marked_utf16 = None;
        self.chips.clear();
    }

    /// Replace `range` (bytes) with an atomic chip plus a trailing space.
    pub fn insert_chip(&mut self, range: Range<usize>, chip: &str) {
        self.marked_utf16 = None;
        self.caret = chips::insert_chip(&mut self.content, &mut self.chips, range, chip);
    }

    /// Replace `range` (bytes) with `text`, chip-atomically. Returns the
    /// byte range actually replaced (widened to whole chips).
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
        // A chip is one caret stop: jump over it whole.
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
            // Touching a chip deletes the whole chip.
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
                    let mut has_image = false;
                    for entry in item.entries() {
                        if let gpui::ClipboardEntry::Image(img) = entry
                            && !img.bytes.is_empty()
                        {
                            has_image = true;
                            // Content-derived temp name, owned by this
                            // composer (deleted on remove / retired on
                            // submit / swept at startup — see
                            // shared::temp_attachments; the old timestamp
                            // naming leaked files on every paste).
                            match crate::shared::temp_attachments::write_temp_image(
                                &img.bytes, "png",
                            ) {
                                Ok(tmp_path) => {
                                    let file_name = tmp_path
                                        .file_name()
                                        .map(|n| n.to_string_lossy().into_owned())
                                        .unwrap_or_default();
                                    let att = AttachmentRef {
                                        reference: tmp_path.to_string_lossy().into_owned(),
                                        file_name,
                                        mime: "image/png".to_string(),
                                        bytes: img.bytes.len() as u64,
                                        preview_ref: None,
                                    };
                                    self.add_attachment(att, cx);
                                }
                                Err(e) => {
                                    eprintln!("[zcode-gpui] paste temp file failed: {e}");
                                }
                            }
                        }
                    }
                    if !has_image {
                        let text = item.text().unwrap_or_default();
                        self.marked_utf16 = None;
                        self.insert_at_caret(&text, cx);
                    }
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

impl crate::app::root::RootView {
    /// Submit plumbing (split from app/root.rs for the 400-line cap): both
    /// composer entities bubble `Submitted` into the root's submit/commit
    /// actions.
    pub(crate) fn wire_submit_events(
        composer: &gpui::Entity<Composer>,
        commit_input: &gpui::Entity<Composer>,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe(composer, |this, _composer, ev: &ComposerEvent, cx| {
            if matches!(ev, ComposerEvent::Submitted) {
                this.submit(cx);
            }
        })
        .detach();
        cx.subscribe(commit_input, |this, _composer, ev: &ComposerEvent, cx| {
            if matches!(ev, ComposerEvent::Submitted) {
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
            .text_size(px(14.))
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
                        div()
                            .text_color(rgb(crate::shared::theme::MUTED))
                            .child(self.placeholder)
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
