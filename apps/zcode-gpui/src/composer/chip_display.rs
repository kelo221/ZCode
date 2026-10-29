//! What the composer paints: its raw markdown with each chip replaced by the
//! chip's pill text (`⚡ Code Review` for `[$code-review](…)`), plus the map
//! between raw byte offsets (caret, selection, edits) and displayed byte
//! offsets (the text layout). The content itself is never rewritten.

use crate::composer::chips::ChipTable;
use std::ops::Range;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct ChipDisplay {
    pub(crate) text: String,
    /// `(raw chip range, displayed pill range)`, in order.
    spans: Vec<(Range<usize>, Range<usize>)>,
}

impl ChipDisplay {
    pub(crate) fn new(content: &str, chips: &ChipTable) -> Self {
        let mut text = String::with_capacity(content.len());
        let mut spans = Vec::new();
        let mut at = 0;
        for chip in chips.iter() {
            if chip.start < at || chip.end > content.len() {
                continue;
            }
            let raw = &content[chip.clone()];
            let Some(mention) = crate::shared::mention_parse::whole_mention(raw) else {
                continue;
            };
            text.push_str(&content[at..chip.start]);
            let start = text.len();
            text.push_str(&mention.display());
            spans.push((chip.clone(), start..text.len()));
            at = chip.end;
        }
        text.push_str(&content[at..]);
        Self { text, spans }
    }

    /// Displayed pill ranges.
    pub(crate) fn pills(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.spans.iter().map(|(_, d)| d.clone())
    }

    /// Displayed offset of raw offset `raw` (inside a chip → the pill's end).
    pub(crate) fn to_display(&self, raw: usize) -> usize {
        let mut delta: isize = 0;
        for (r, d) in &self.spans {
            if raw <= r.start {
                break;
            }
            if raw < r.end {
                return d.end;
            }
            delta = d.end as isize - r.end as isize;
        }
        (raw as isize + delta).max(0) as usize
    }

    /// Raw offset of displayed offset `shown`; inside a pill snaps to the nearer edge.
    pub(crate) fn to_raw(&self, shown: usize) -> usize {
        let mut delta: isize = 0;
        for (r, d) in &self.spans {
            if shown <= d.start {
                break;
            }
            if shown < d.end {
                return if shown - d.start < d.end - shown {
                    r.start
                } else {
                    r.end
                };
            }
            delta = r.end as isize - d.end as isize;
        }
        (shown as isize + delta).max(0) as usize
    }

    pub(crate) fn range_to_display(&self, r: Range<usize>) -> Range<usize> {
        self.to_display(r.start)..self.to_display(r.end)
    }
}

/// Sorted, disjoint highlights from possibly overlapping ones (pills under a
/// selection), merging the styles of overlapping pieces in order.
pub(crate) fn merge_highlights(
    items: Vec<(Range<usize>, gpui::HighlightStyle)>,
) -> Vec<(Range<usize>, gpui::HighlightStyle)> {
    let mut cuts: Vec<usize> = items.iter().flat_map(|(r, _)| [r.start, r.end]).collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut out: Vec<(Range<usize>, gpui::HighlightStyle)> = Vec::new();
    for w in cuts.windows(2) {
        let piece = w[0]..w[1];
        let mut style: Option<gpui::HighlightStyle> = None;
        for (r, s) in &items {
            if r.start <= piece.start && piece.end <= r.end {
                match &mut style {
                    Some(acc) => *acc = acc.highlight(*s),
                    None => style = Some(*s),
                }
            }
        }
        if let Some(style) = style {
            match out.last_mut() {
                Some((last, s)) if last.end == piece.start && *s == style => last.end = piece.end,
                _ => out.push((piece, style)),
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "chip_display_tests.rs"]
mod tests;
