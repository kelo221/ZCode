//! Pure text-editing helpers for the composer: char and word steps and the
//! selection range. Byte offsets always sit on char boundaries.

use std::ops::Range;

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn prev_char(text: &str, pos: usize) -> usize {
    text[..pos].char_indices().next_back().map_or(0, |(i, _)| i)
}

pub fn next_char(text: &str, pos: usize) -> usize {
    text[pos..]
        .chars()
        .next()
        .map_or(pos, |c| pos + c.len_utf8())
}

/// Start of the word before `pos` (Ctrl+Left / Ctrl+Backspace): skip
/// whitespace, then one run of word chars or of punctuation.
pub fn prev_word(text: &str, pos: usize) -> usize {
    let mut chars = text[..pos].char_indices().rev().peekable();
    while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
    let Some(&(_, first)) = chars.peek() else {
        return 0;
    };
    let word = is_word(first);
    let mut start = pos;
    while let Some((i, _)) = chars.next_if(|(_, c)| !c.is_whitespace() && is_word(*c) == word) {
        start = i;
    }
    if start == pos {
        // Only whitespace before `pos`.
        text[..pos]
            .char_indices()
            .rev()
            .find(|(_, c)| !c.is_whitespace())
            .map_or(0, |(i, c)| i + c.len_utf8())
    } else {
        start
    }
}

/// End of the word after `pos` (Ctrl+Right / Ctrl+Delete), mirroring
/// [`prev_word`].
pub fn next_word(text: &str, pos: usize) -> usize {
    let mut chars = text[pos..].char_indices().peekable();
    while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
    let Some(&(_, first)) = chars.peek() else {
        return text.len();
    };
    let word = is_word(first);
    let mut end = pos;
    while let Some((i, c)) = chars.next_if(|(_, c)| !c.is_whitespace() && is_word(*c) == word) {
        end = pos + i + c.len_utf8();
    }
    end
}

/// Non-empty selection between `anchor` and `caret`, in content order.
pub fn selection(anchor: Option<usize>, caret: usize, len: usize) -> Option<Range<usize>> {
    let a = anchor?.min(len);
    let c = caret.min(len);
    (a != c).then(|| a.min(c)..a.max(c))
}

#[cfg(test)]
#[path = "editing_tests.rs"]
mod tests;
