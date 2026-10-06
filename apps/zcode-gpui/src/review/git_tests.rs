//! Golden tests for `review/git.rs` parsing (kept out-of-line for the 400-line cap).

use super::*;

#[test]
fn parses_porcelain_status_and_branch() {
    // concat! avoids Rust's `\<newline>` continuation, which would strip the
    // leading spaces that porcelain status uses for the index column.
    let out = concat!(
        "## main...origin/main [ahead 1]\n",
        "M  src/staged.rs\n",
        " M src/unstaged.rs\n",
        "MM both.rs\n",
        "?? new.txt\n",
        "R  old.rs -> renamed.rs\n",
    );
    let st = parse_status(out);
    assert_eq!(st.branch, "main");
    assert_eq!(st.files.len(), 5);
    let staged = &st.files[0];
    assert_eq!(staged.path, "src/staged.rs");
    assert!(staged.staged_changed());
    assert!(!staged.unstaged_changed());
    assert!(st.files[1].unstaged_changed());
    let both = &st.files[2];
    assert!(both.staged_changed() && both.unstaged_changed());
    assert!(st.files[3].untracked());
    assert_eq!(st.files[4].path, "renamed.rs");
}

#[test]
fn detached_head_has_no_dots() {
    let st = parse_status("## HEAD (no branch)\n");
    assert_eq!(st.branch, "HEAD");
}

#[test]
fn parses_nul_delimited_status_and_rename() {
    // `git status -z`: rename is "R  old\0new\0", not "old -> new".
    let out = "## main\0R  old.rs\0renamed.rs\0?? new file.txt\0";
    let st = parse_status(out);
    assert_eq!(st.branch, "main");
    assert_eq!(st.files.len(), 2);
    assert_eq!(st.files[0].path, "renamed.rs");
    assert_eq!(st.files[1].path, "new file.txt");
}

#[test]
fn parses_nul_delimited_numstat_rename() {
    // `git diff --numstat -z` rename: "add\tdel\0old\0new\0".
    let map = parse_numstat(concat!("3\t1\0old.rs\0new.rs\0", "12\t0\tok.rs\0"));
    assert_eq!(map["new.rs"], (3, 1));
    assert_eq!(map["ok.rs"], (12, 0));
}

#[test]
fn discard_confirm_clears_on_repository_switch() {
    let mut g = GitState {
        confirm_discard: Some("a.rs".into()),
        ..GitState::default()
    };
    g.apply_status(std::path::PathBuf::from("/repo-a"), GitStatus::default());
    g.confirm_discard = Some("a.rs".into());
    g.apply_status(std::path::PathBuf::from("/repo-b"), GitStatus::default());
    assert!(
        g.confirm_discard.is_none(),
        "armed discard must not follow a repo switch"
    );
}

#[test]
fn parses_numstat_and_binary() {
    let map = parse_numstat("12\t3\ta.rs\n-\t-\timg.png\n");
    assert_eq!(map["a.rs"], (12, 3));
    assert_eq!(map["img.png"], (0, 0));
}

#[test]
fn applies_numstat_to_status() {
    let mut st = parse_status("MM both.rs\n M only-wt.rs\n");
    let staged = [("both.rs".to_string(), (5u32, 2u32))].into();
    let unstaged = [
        ("both.rs".to_string(), (1u32, 1u32)),
        ("only-wt.rs".to_string(), (9u32, 0u32)),
    ]
    .into();
    apply_numstat(&mut st, staged, unstaged);
    assert_eq!(st.files[0].total_add(), 6);
    assert_eq!(st.files[0].total_del(), 3);
    assert_eq!(st.files[1].unstaged_add, 9);
    assert_eq!(st.files[1].staged_add, 0);
}

#[test]
fn badge_prefers_view_column() {
    let f = GitFile {
        path: "x".into(),
        index: 'A',
        worktree: 'M',
        staged_add: 0,
        staged_del: 0,
        unstaged_add: 0,
        unstaged_del: 0,
    };
    assert_eq!(f.badge(true), "A");
    assert_eq!(f.badge(false), "M");
    let unstaged_only = GitFile {
        index: ' ',
        worktree: 'M',
        ..f.clone()
    };
    // Staged view of an unstaged-only file falls back to the worktree letter.
    assert_eq!(unstaged_only.badge(true), "M");
}
