//! fzf-style fuzzy scoring for `@` file mentions (COMPOSER_PARITY.md "@ file
//! index"): the query's characters must appear in order (case-insensitive);
//! among all such alignments the best one is found with a dynamic program, and
//! matches at word starts (after `/`, `_`, `-`, `.`, a space, or a camelCase /
//! digit hump) and runs of consecutive characters score higher, gaps lower.
//! Constants follow fzf's published scoring scheme; the code is our own.

const MATCH: i32 = 16;
const GAP_START: i32 = -3;
const GAP_EXTENSION: i32 = -1;
const BOUNDARY_WHITE: i32 = 10;
const BOUNDARY_DELIMITER: i32 = 9;
const BOUNDARY: i32 = 8;
const NON_WORD: i32 = 8;
const CAMEL_123: i32 = BOUNDARY + GAP_EXTENSION;
const CONSECUTIVE: i32 = -(GAP_START + GAP_EXTENSION);
const FIRST_CHAR_MULTIPLIER: i32 = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    White,
    Delimiter,
    NonWord,
    Lower,
    Upper,
    Letter,
    Number,
}

fn class(c: char) -> Class {
    if c.is_lowercase() {
        Class::Lower
    } else if c.is_uppercase() {
        Class::Upper
    } else if c.is_numeric() {
        Class::Number
    } else if c.is_alphabetic() {
        Class::Letter
    } else if c.is_whitespace() {
        Class::White
    } else if matches!(c, '/' | '\\' | ',' | ':' | ';' | '|') {
        Class::Delimiter
    } else {
        Class::NonWord
    }
}

fn bonus(prev: Class, cur: Class) -> i32 {
    let word = matches!(
        cur,
        Class::Lower | Class::Upper | Class::Letter | Class::Number
    );
    if word {
        match prev {
            Class::White => return BOUNDARY_WHITE,
            Class::Delimiter => return BOUNDARY_DELIMITER,
            Class::NonWord => return BOUNDARY,
            _ => {}
        }
    }
    if (prev == Class::Lower && cur == Class::Upper)
        || (prev != Class::Number && cur == Class::Number)
    {
        return CAMEL_123;
    }
    match cur {
        Class::NonWord | Class::Delimiter => NON_WORD,
        Class::White => BOUNDARY_WHITE,
        _ => 0,
    }
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Cheap pre-check: the (lowercase) query is a subsequence of `lower`.
pub(crate) fn is_subsequence(lower: &str, query: &str) -> bool {
    let mut text = lower.chars();
    query.chars().all(|q| text.any(|t| t == q))
}

/// Reusable buffers, so ranking a large index allocates nothing per entry.
#[derive(Default)]
pub(crate) struct Scratch {
    bonuses: Vec<i32>,
    folded: Vec<char>,
    prev: Vec<i32>,
    prev_run: Vec<i32>,
    row: Vec<i32>,
    row_run: Vec<i32>,
}

const NONE: i32 = i32::MIN / 2;

/// Best alignment score of lowercase `query` in `text` (higher is better);
/// `None` when the characters don't all appear in order. The start of `text`
/// counts as a word boundary.
#[cfg(test)]
pub(crate) fn score(text: &[char], query: &[char]) -> Option<i32> {
    score_with(text, query, &mut Scratch::default())
}

pub(crate) fn score_with(text: &[char], query: &[char], sc: &mut Scratch) -> Option<i32> {
    let (n, m) = (text.len(), query.len());
    if m == 0 {
        return Some(0);
    }
    if m > n {
        return None;
    }
    let bonus_at = |j: usize| {
        let prev = if j == 0 {
            Class::White
        } else {
            class(text[j - 1])
        };
        bonus(prev, class(text[j]))
    };
    if m == 1 {
        return (0..n)
            .filter(|&j| fold(text[j]) == query[0])
            .map(|j| MATCH + bonus_at(j) * FIRST_CHAR_MULTIPLIER)
            .max();
    }
    sc.bonuses.clear();
    sc.bonuses.extend((0..n).map(bonus_at));
    sc.folded.clear();
    sc.folded.extend(text.iter().map(|&c| fold(c)));
    for v in [&mut sc.prev, &mut sc.prev_run, &mut sc.row, &mut sc.row_run] {
        v.clear();
        v.resize(n, NONE);
    }
    let Scratch {
        bonuses,
        folded,
        prev,
        prev_run,
        row,
        row_run,
    } = sc;
    // `prev`: best score with the previous query char matched at j; `prev_run`:
    // the bonus of the consecutive run that path ends in.
    for (i, &q) in query.iter().enumerate() {
        row.fill(NONE);
        // Best `prev[k] + gap(j - k - 1)` over k ≤ j - 2, carried along j.
        let mut gapped = NONE;
        for j in i..n {
            if j >= 2 && prev[j - 2] > NONE {
                gapped = gapped.max(prev[j - 2] + GAP_START);
            }
            let here = (folded[j] == q).then_some(bonuses[j]);
            if let Some(b) = here {
                if i == 0 {
                    row[j] = MATCH + b * FIRST_CHAR_MULTIPLIER;
                    row_run[j] = b;
                } else {
                    let mut best = NONE;
                    let mut run = b;
                    if gapped > NONE {
                        best = gapped + MATCH + b;
                    }
                    if prev[j - 1] > NONE {
                        let run_bonus = if b >= BOUNDARY { b } else { prev_run[j - 1] };
                        let consecutive = prev[j - 1] + MATCH + b.max(run_bonus).max(CONSECUTIVE);
                        if consecutive >= best {
                            best = consecutive;
                            run = run_bonus;
                        }
                    }
                    row[j] = best;
                    row_run[j] = run;
                }
            }
            if gapped > NONE {
                gapped += GAP_EXTENSION;
            }
        }
        std::mem::swap(prev, row);
        std::mem::swap(prev_run, row_run);
    }
    prev.iter().copied().filter(|&s| s > NONE).max()
}

#[cfg(test)]
#[path = "fuzzy_tests.rs"]
mod tests;
