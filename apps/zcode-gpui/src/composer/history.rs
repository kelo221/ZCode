//! Composer undo / redo. Every edit goes through `Composer::edit_range` or
//! `set_text`; each records the state before it, except that a run of typed
//! characters (or of single-character deletes) at the caret merges into one
//! step, like a browser textarea. IME composition records once, when it starts.

use crate::composer::chips::ChipTable;
use crate::composer::input::Composer;
use gpui::Context;
use std::ops::Range;

const MAX_STEPS: usize = 100;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Snapshot {
    content: String,
    caret: usize,
    anchor: Option<usize>,
    chips: ChipTable,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Run {
    /// Typing that ended at this byte.
    Typing(usize),
    /// Single-character deletes whose last removal started at this byte.
    Deleting(usize),
}

#[derive(Default)]
pub(crate) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    run: Option<Run>,
}

/// What an edit of `range` with `text` continues, if anything.
fn classify(range: &Range<usize>, text: &str) -> Option<Run> {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if range.is_empty() && !c.is_whitespace() => {
            Some(Run::Typing(range.start + c.len_utf8()))
        }
        (None, _) if range.len() <= 4 && !range.is_empty() => Some(Run::Deleting(range.start)),
        _ => None,
    }
}

/// Whether this edit merges into the previous step instead of starting one.
fn continues(prev: Option<Run>, range: &Range<usize>, next: Option<Run>) -> bool {
    match (prev, next) {
        (Some(Run::Typing(end)), Some(Run::Typing(_))) => range.start == end,
        // Backspace removes the byte before the last removal; Delete removes at it.
        (Some(Run::Deleting(at)), Some(Run::Deleting(_))) => range.end == at || range.start == at,
        _ => false,
    }
}

impl History {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    fn push(stack: &mut Vec<Snapshot>, snap: Snapshot) {
        if stack.last() == Some(&snap) {
            return;
        }
        stack.push(snap);
        if stack.len() > MAX_STEPS {
            stack.remove(0);
        }
    }

    /// Called before an edit of `range` with `text`.
    pub(crate) fn before_edit(&mut self, before: Snapshot, range: &Range<usize>, text: &str) {
        let next = classify(range, text);
        if !continues(self.run, range, next) {
            Self::push(&mut self.undo, before);
        }
        self.run = next;
        self.redo.clear();
    }

    /// Called before a whole-text replacement.
    pub(crate) fn before_replace(&mut self, before: Snapshot) {
        Self::push(&mut self.undo, before);
        self.run = None;
        self.redo.clear();
    }

    /// A caret move ends the current typing run.
    pub(crate) fn break_run(&mut self) {
        self.run = None;
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    fn step(&mut self, current: Snapshot, back: bool) -> Option<Snapshot> {
        let (from, to) = if back {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let snap = from.pop()?;
        Self::push(to, current);
        self.run = None;
        Some(snap)
    }
}

impl Composer {
    pub(crate) fn snapshot(&self) -> Snapshot {
        Snapshot {
            content: self.content.clone(),
            caret: self.caret,
            anchor: self.anchor,
            chips: self.chips.clone(),
        }
    }

    fn restore(&mut self, snap: Snapshot) {
        self.content = snap.content;
        self.caret = snap.caret.min(self.content.len());
        self.anchor = snap.anchor.filter(|&a| a <= self.content.len());
        self.chips = snap.chips;
        self.marked_utf16 = None;
    }

    /// Undo (`back`) or redo one step; `false` when there is none.
    pub(crate) fn undo_redo(&mut self, back: bool, cx: &mut Context<Self>) -> bool {
        if self.marked_utf16.is_some() {
            return false;
        }
        let current = self.snapshot();
        let Some(snap) = self.history.step(current, back) else {
            return false;
        };
        self.restore(snap);
        cx.notify();
        true
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
