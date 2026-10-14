//! Composer text input with IME support (composition via marked text) and
//! Enter-to-submit. Keyboard editing lives in `keys.rs`, wrapped rendering
//! and mouse placement in `text_view.rs`.

use crate::composer::attachment::AttachmentRef;
use crate::composer::chips::{self, ChipTable};
use gpui::{Context, EventEmitter, FocusHandle};
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
    /// Selection anchor; the selection spans anchor..caret when they differ.
    pub(crate) anchor: Option<usize>,
    pub(crate) marked_utf16: Option<Range<usize>>,
    pub(crate) focus: FocusHandle,
    pub(crate) single_line: bool,
    pub(crate) placeholder: &'static str,
    /// Empty conversation: Desktop's new-task placeholder instead of follow-up.
    pub(crate) new_task: bool,
    pub(crate) attachments: Vec<AttachmentRef>,
    pub(crate) temp_owned: Vec<PathBuf>,
    pub(crate) chips: ChipTable,
    pub(crate) view: crate::composer::text_view::TextViewState,
}

impl Composer {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            content: String::new(),
            replacement_generation: 0,
            caret: 0,
            anchor: None,
            marked_utf16: None,
            focus: cx.focus_handle(),
            single_line: false,
            placeholder: "Ask for follow-up changes",
            new_task: false,
            attachments: Vec::new(),
            temp_owned: Vec::new(),
            chips: ChipTable::default(),
            view: Default::default(),
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
        self.anchor = None;
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
        self.anchor = None;
        self.marked_utf16 = None;
        self.chips.clear();
    }

    pub fn insert_chip(&mut self, range: Range<usize>, chip: &str) {
        self.marked_utf16 = None;
        self.anchor = None;
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

    /// Insert at the caret, replacing the selection if there is one.
    pub(crate) fn insert_at_caret(&mut self, text: &str, cx: &mut Context<Self>) {
        let cleaned = self.sanitize(text);
        if cleaned.is_empty() {
            return;
        }
        let range = self.selection().unwrap_or(self.caret..self.caret);
        let r = self.edit_range(range, &cleaned);
        self.caret = r.start + cleaned.len();
        self.anchor = None;
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
