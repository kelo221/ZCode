use super::*;

#[test]
fn prev_word_skips_whitespace_then_one_run() {
    let t = "let foo = bar_baz  ";
    assert_eq!(prev_word(t, t.len()), 10);
    assert_eq!(prev_word(t, 10), 8, "punctuation is its own run");
    assert_eq!(prev_word(t, 8), 4);
    assert_eq!(prev_word(t, 3), 0);
    assert_eq!(prev_word(t, 0), 0);
}

#[test]
fn next_word_mirrors_prev_word() {
    let t = "let foo = bar_baz";
    assert_eq!(next_word(t, 0), 3);
    assert_eq!(next_word(t, 3), 7);
    assert_eq!(next_word(t, 7), 9);
    assert_eq!(next_word(t, 9), t.len());
    assert_eq!(next_word(t, t.len()), t.len());
}

#[test]
fn word_steps_respect_multibyte_chars() {
    let t = "héllo wörld";
    let end = t.len();
    let start = prev_word(t, end);
    assert_eq!(&t[start..], "wörld");
    assert_eq!(&t[..next_word(t, 0)], "héllo");
    assert_eq!(prev_char(t, 3), 1, "é is two bytes");
    assert_eq!(next_char(t, 1), 3);
}

#[test]
fn whitespace_only_prefix_goes_to_start() {
    assert_eq!(prev_word("   ", 3), 0);
    assert_eq!(next_word("   ", 0), 3);
}

#[test]
fn selection_is_ordered_and_empty_is_none() {
    assert_eq!(selection(Some(5), 2, 10), Some(2..5));
    assert_eq!(selection(Some(2), 5, 10), Some(2..5));
    assert_eq!(selection(Some(3), 3, 10), None);
    assert_eq!(selection(None, 3, 10), None);
    assert_eq!(
        selection(Some(50), 3, 10),
        Some(3..10),
        "stale anchor clamps"
    );
}
