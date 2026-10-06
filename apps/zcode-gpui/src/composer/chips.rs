//! Atomic mention/skill chips over the composer's plain-string content.
//!
//! The composer stays a `String`; chips are a side-table of byte ranges
//! (sorted, disjoint). Every edit is routed through [`edit`], which widens
//! any range that touches a chip to cover the whole chip, so neither a
//! keystroke nor an IME composition can leave half a mention behind.

use std::ops::Range;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChipTable(Vec<Range<usize>>);

impl ChipTable {
    pub(crate) fn has_chips(&self) -> bool {
        !self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    #[cfg(test)]
    pub fn ranges(&self) -> &[Range<usize>] {
        &self.0
    }

    /// Register a chip (caller guarantees it does not overlap another).
    pub fn insert(&mut self, chip: Range<usize>) {
        let at = self.0.partition_point(|c| c.start < chip.start);
        self.0.insert(at, chip);
    }

    /// Widen `r` so it never covers part of a chip. An empty range (an
    /// insertion point) strictly inside a chip moves to the chip's end.
    pub fn expand(&self, r: Range<usize>) -> Range<usize> {
        if r.is_empty() {
            let p = self
                .0
                .iter()
                .find(|c| c.start < r.start && r.start < c.end)
                .map_or(r.start, |c| c.end);
            return p..p;
        }
        let mut out = r;
        for c in &self.0 {
            if c.start < out.end && out.start < c.end {
                out.start = out.start.min(c.start);
                out.end = out.end.max(c.end);
            }
        }
        out
    }

    /// Account for `r` (already expanded) being replaced by `new_len` bytes:
    /// chips inside `r` are gone, chips at or after `r.end` shift.
    fn apply_edit(&mut self, r: &Range<usize>, new_len: usize) {
        self.0.retain(|c| !(c.start < r.end && r.start < c.end));
        for c in &mut self.0 {
            if c.start >= r.end {
                c.start = c.start - r.end + r.start + new_len;
                c.end = c.end - r.end + r.start + new_len;
            }
        }
    }

    /// Never leave the caret inside a chip.
    pub fn snap(&self, pos: usize) -> usize {
        self.expand(pos..pos).start
    }

    /// Caret-left target when a chip ends exactly at `pos`.
    pub fn chip_ending_at(&self, pos: usize) -> Option<usize> {
        self.0.iter().find(|c| c.end == pos).map(|c| c.start)
    }

    /// Caret-right target when a chip starts exactly at `pos`.
    pub fn chip_starting_at(&self, pos: usize) -> Option<usize> {
        self.0.iter().find(|c| c.start == pos).map(|c| c.end)
    }
}

/// Replace `range` of `content` with `text`, atomically with respect to
/// chips. Returns the range that was actually replaced (after widening).
pub fn edit(
    content: &mut String,
    chips: &mut ChipTable,
    range: Range<usize>,
    text: &str,
) -> Range<usize> {
    let end = range.end.min(content.len());
    let r = chips.expand(range.start.min(end)..end);
    content.replace_range(r.clone(), text);
    chips.apply_edit(&r, text.len());
    r
}

/// Replace `range` with an atomic chip followed by a plain space. Returns
/// the caret position after the space.
pub fn insert_chip(
    content: &mut String,
    chips: &mut ChipTable,
    range: Range<usize>,
    chip: &str,
) -> usize {
    let r = edit(content, chips, range, "");
    let at = r.start;
    edit(content, chips, at..at, &format!("{chip} "));
    chips.insert(at..at + chip.len());
    at + chip.len() + 1
}

/// UTF-16 offset of byte offset `byte` in `content`.
pub fn utf16_of(content: &str, byte: usize) -> usize {
    content[..byte.min(content.len())]
        .chars()
        .map(char::len_utf16)
        .sum()
}

#[cfg(test)]
#[path = "chips_tests.rs"]
mod tests;
