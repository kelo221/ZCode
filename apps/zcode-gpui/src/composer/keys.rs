//! Composer keyboard editing: caret and word movement, Shift-extended
//! selection, select all, word delete, and clipboard copy/cut/paste.
//!
//! Word steps use Ctrl on Windows/Linux and Alt on macOS; select all and the
//! clipboard use the primary modifier (Ctrl / Cmd).

use crate::composer::delivery::{SubmitTrigger, primary_modifier};
use crate::composer::editing::{next_char, next_word, prev_char, prev_word, selection};
use crate::composer::input::{Composer, ComposerEvent};
use gpui::{ClipboardItem, Context, KeyDownEvent, Keystroke, Modifiers, Window};
use std::ops::Range;

fn word_modifier(m: &Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        m.alt
    } else {
        m.control
    }
}

impl Composer {
    pub(crate) fn selection(&self) -> Option<Range<usize>> {
        selection(self.anchor, self.caret, self.content.len())
    }

    pub(crate) fn select_all(&mut self, cx: &mut Context<Self>) {
        self.marked_utf16 = None;
        self.anchor = Some(0);
        self.caret = self.content.len();
        cx.notify();
    }

    /// Move the caret to `pos`; with `extend` the selection anchor stays put.
    pub(crate) fn move_to(&mut self, pos: usize, extend: bool, cx: &mut Context<Self>) {
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = self.chips.snap(pos.min(self.content.len()));
        self.marked_utf16 = None;
        cx.notify();
    }

    /// Remove the selected text. `false` when nothing was selected.
    pub(crate) fn delete_selection(&mut self) -> bool {
        let Some(sel) = self.selection() else {
            self.anchor = None;
            return false;
        };
        let removed = self.edit_range(sel, "");
        self.caret = removed.start;
        self.anchor = None;
        true
    }

    fn step_target(&self, forward: bool, word: bool) -> usize {
        let (text, at) = (self.content.as_str(), self.caret);
        match (forward, word) {
            (false, true) => prev_word(text, at),
            (true, true) => next_word(text, at),
            (false, false) => self
                .chips
                .chip_ending_at(at)
                .unwrap_or_else(|| prev_char(text, at)),
            (true, false) => self
                .chips
                .chip_starting_at(at)
                .unwrap_or_else(|| next_char(text, at)),
        }
    }

    fn arrow(&mut self, forward: bool, m: &Modifiers, cx: &mut Context<Self>) {
        let word = word_modifier(m);
        // A plain arrow collapses an existing selection to its edge.
        if !m.shift
            && !word
            && let Some(sel) = self.selection()
        {
            self.move_to(if forward { sel.end } else { sel.start }, false, cx);
            return;
        }
        let line_jump = cfg!(target_os = "macos") && m.platform;
        let target = match (line_jump, forward) {
            (true, false) => 0,
            (true, true) => self.content.len(),
            _ => self.step_target(forward, word),
        };
        self.move_to(target, m.shift, cx);
    }

    fn delete(&mut self, forward: bool, m: &Modifiers, cx: &mut Context<Self>) {
        if self.marked_utf16.take().is_some() {
            cx.notify();
            return;
        }
        if !self.delete_selection() {
            let target = self.step_target(forward, word_modifier(m));
            let range = if forward {
                self.caret..target
            } else {
                target..self.caret
            };
            if !range.is_empty() {
                let removed = self.edit_range(range, "");
                self.caret = removed.start;
            }
        }
        cx.notify();
    }

    fn copy_selection(&mut self, cut: bool, cx: &mut Context<Self>) -> bool {
        let Some(sel) = self.selection() else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(self.content[sel].to_string()));
        if cut {
            self.delete_selection();
            cx.notify();
        }
        true
    }

    pub(crate) fn on_key_down(
        &mut self,
        ev: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Keystroke { key, modifiers, .. } = &ev.keystroke;
        let primary = primary_modifier(modifiers);
        let mut handled = true;
        match key.as_str() {
            "enter" => {
                if self.marked_utf16.is_none() && !modifiers.shift {
                    let trigger = if !self.single_line && primary {
                        SubmitTrigger::ModifiedEnter
                    } else {
                        SubmitTrigger::Ordinary
                    };
                    cx.emit(ComposerEvent::Submitted(trigger));
                } else if modifiers.shift {
                    self.insert_at_caret("\n", cx);
                } else {
                    handled = false;
                }
            }
            "backspace" => self.delete(false, modifiers, cx),
            "delete" => self.delete(true, modifiers, cx),
            "left" => self.arrow(false, modifiers, cx),
            "right" => self.arrow(true, modifiers, cx),
            "home" => self.move_to(0, modifiers.shift, cx),
            "end" => self.move_to(self.content.len(), modifiers.shift, cx),
            "a" if primary => self.select_all(cx),
            "c" if primary => handled = self.copy_selection(false, cx),
            "x" if primary => handled = self.copy_selection(true, cx),
            "v" if primary => self.paste_clipboard(cx),
            "escape" if self.selection().is_some() => {
                self.anchor = None;
                cx.notify();
            }
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
            let text = item.text().unwrap_or_default();
            if !self.single_line && crate::composer::pasted_text::is_long_paste(&text) {
                cx.emit(ComposerEvent::TextPasted {
                    text,
                    replacement: self.replacement_generation,
                });
            } else {
                self.insert_at_caret(&text, cx);
            }
        }
    }
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
