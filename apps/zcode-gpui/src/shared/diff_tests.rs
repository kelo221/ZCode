//! Golden tests for `shared/diff_view.rs` (kept out-of-line for the 400-line cap).

use super::*;

#[test]
fn detects_hunk_headers() {
    assert!(looks_like_diff("@@ -1,3 +1,4 @@\n-old\n+new\n ctx"));
}

#[test]
fn detects_git_headers() {
    assert!(looks_like_diff("diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@"));
}

#[test]
fn plain_prose_is_not_a_diff() {
    assert!(!looks_like_diff("+1 looks good to me\nthanks for the +1 review"));
    assert!(!looks_like_diff("regular chat text\nnothing special"));
}

#[test]
fn yaml_front_matter_and_rules_are_not_diffs() {
    assert!(!looks_like_diff("---\ntitle: Notes\n---\nbody text"));
    assert!(!looks_like_diff("Intro\n\n---\n\nNext section"));
    // A bare ---/+++ header pair is still recognised.
    assert!(looks_like_diff("--- a/x.rs\n+++ b/x.rs\n-old\n+new"));
}

#[test]
fn balanced_add_del_blocks_count_as_diff() {
    let text = "old line one\nold line two\n+new one\n+new two\n-old one\n-old two\nkeep";
    assert!(looks_like_diff(text));
}

#[test]
fn classify_prefixes() {
    assert_eq!(classify("+added"), LineKind::Add);
    assert_eq!(classify("-removed"), LineKind::Del);
    assert_eq!(classify("@@ -1 +1 @@"), LineKind::Hunk);
    assert_eq!(classify("+++ b/file.rs"), LineKind::Meta);
    assert_eq!(classify(" context"), LineKind::Context);
    assert_eq!(classify(""), LineKind::Context);
}
