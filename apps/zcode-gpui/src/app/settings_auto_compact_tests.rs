//! Tests for `app/settings_auto_compact.rs`.

use super::*;

const ON_50: AutoCompactEntry = AutoCompactEntry {
    enabled: true,
    minutes: 50,
};

#[test]
fn a_new_value_commits_and_keeps_the_switch() {
    assert_eq!(
        minutes_update(ON_50, " 30 "),
        Some(AutoCompactEntry {
            enabled: true,
            minutes: 30
        })
    );
}

#[test]
fn empty_unchanged_or_garbage_drafts_commit_nothing() {
    assert_eq!(minutes_update(ON_50, ""), None);
    assert_eq!(minutes_update(ON_50, "50"), None);
    assert_eq!(minutes_update(ON_50, "abc"), None);
}

#[test]
fn out_of_range_drafts_are_clamped() {
    assert_eq!(minutes_update(ON_50, "1").map(|e| e.minutes), Some(5));
    assert_eq!(minutes_update(ON_50, "9999").map(|e| e.minutes), Some(1440));
    let at_min = AutoCompactEntry {
        minutes: 5,
        ..ON_50
    };
    assert_eq!(
        minutes_update(at_min, "0"),
        None,
        "clamps back to the same value"
    );
}
