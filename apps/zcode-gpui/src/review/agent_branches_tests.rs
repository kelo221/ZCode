//! Tests for `review/agent_branches.rs` and the target-aware diff cache.

use super::*;
use crate::review::git::DiffSide;
use crate::review::git_state::GitState;
use std::process::Command;

fn branch(base: &str, result: &str) -> AgentBranch {
    AgentBranch {
        repo_top: PathBuf::from("/repo"),
        worktree: PathBuf::from("/repo/.zcode/worktrees/agent_1"),
        branch: "zcode/agent_1".into(),
        base: base.into(),
        result: result.into(),
        files: 1,
        add: 2,
        del: 0,
    }
}

#[test]
fn lists_only_agent_worktrees_inside_the_repository() {
    let out = concat!(
        "worktree C:/repo\nHEAD 1111111\nbranch refs/heads/main\n\n",
        "worktree C:/repo/.zcode/worktrees/agent_1\nHEAD 2222222\nbranch refs/heads/zcode/agent_1\n\n",
        "worktree C:/elsewhere/agent_2\nHEAD 3333333\nbranch refs/heads/zcode/agent_2\n\n",
        "worktree C:/repo/.zcode/worktrees/feature\nHEAD 4444444\nbranch refs/heads/feature\n\n",
        "worktree C:/repo/.zcode/worktrees/agent_3\nHEAD 5555555\ndetached\n",
    );
    let found = parse_worktree_list(out, Path::new("C:/repo"));
    assert_eq!(
        found,
        vec![AgentWorktree {
            path: PathBuf::from("C:/repo/.zcode/worktrees/agent_1"),
            branch: "zcode/agent_1".into(),
            head: "2222222".into(),
        }]
    );
}

#[cfg(windows)]
#[test]
fn matches_windows_paths_regardless_of_separator_and_case() {
    let out = "worktree C:/Repo/.zcode/worktrees/agent_1\r\nHEAD 22\r\nbranch refs/heads/zcode/agent_1\r\n";
    assert_eq!(parse_worktree_list(out, Path::new(r"c:\repo\")).len(), 1);
}

#[test]
fn parses_shortstat_variants() {
    assert_eq!(
        parse_shortstat(" 3 files changed, 10 insertions(+), 2 deletions(-)\n"),
        (3, 10, 2)
    );
    assert_eq!(parse_shortstat(" 1 file changed, 1 deletion(-)"), (1, 0, 1));
    assert_eq!(parse_shortstat(""), (0, 0, 0));
}

#[test]
fn builds_a_read_only_compare_status_with_renames() {
    let b = branch("aaaaaaaaaa", "bbbbbbbbbb");
    let names = "M\0src/a.rs\0R100\0old.rs\0new.rs\0A\0added.rs\0";
    let numstat = "3\t1\tsrc/a.rs\0".to_string() + "0\t0\0old.rs\0new.rs\0" + "5\t0\tadded.rs\0";
    let st = compare_status(&b, names, &numstat);
    let rows: Vec<(String, char, u32, u32)> = st
        .files
        .iter()
        .map(|f| (f.path.clone(), f.worktree, f.unstaged_add, f.unstaged_del))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("src/a.rs".into(), 'M', 3, 1),
            ("new.rs".into(), 'R', 0, 0),
            ("added.rs".into(), 'A', 5, 0),
        ]
    );
    assert_eq!(st.branch, "zcode/agent_1 · aaaaaaa..bbbbbbb");
    assert_eq!(
        compare_file_diff_args(&b, "src/a.rs"),
        vec!["diff", "aaaaaaaaaa", "bbbbbbbbbb", "--", "src/a.rs"]
    );
}

#[test]
fn compare_mode_shows_every_file_and_keys_diffs_by_target() {
    let mut g = GitState::default();
    let wt_key = g.diff_key(DiffSide::Unstaged, "a.rs");
    g.set_target(ReviewTarget::AgentBranch(branch("base1", "head1")));
    let compare_key = g.diff_key(DiffSide::Unstaged, "a.rs");
    assert_ne!(wt_key, compare_key);
    g.status = compare_status(&branch("base1", "head1"), "M\0a.rs\0", "1\t0\ta.rs\0");
    g.side = DiffSide::Staged;
    assert_eq!(g.files_for_side().len(), 1);
}

#[test]
fn switching_target_drops_a_diff_still_loading_for_the_old_one() {
    let mut g = GitState::default();
    g.set_target(ReviewTarget::AgentBranch(branch("base1", "head1")));
    g.expanded.insert("a.rs".into());
    assert!(g.mark_pending(DiffSide::Unstaged, "a.rs"));
    let stale_key = g.diff_key(DiffSide::Unstaged, "a.rs");
    let stale_generation = g.generation;

    assert!(g.set_target(ReviewTarget::AgentBranch(branch("base2", "head2"))));
    assert_ne!(
        g.generation, stale_generation,
        "the late response is dropped"
    );
    assert!(g.pending.is_empty() && g.expanded.is_empty() && g.status.files.is_empty());
    // Even if a late response were stored, it would not answer the new target.
    g.store_diff_key(stale_key, "old diff".into());
    assert!(g.diff_for(DiffSide::Unstaged, "a.rs").is_none());
    assert!(!g.set_target(ReviewTarget::AgentBranch(branch("base2", "head2"))));
}

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn reads_a_finalized_agent_branch_from_a_real_repository() {
    let repo = std::env::temp_dir().join(format!("zc-agent-branches-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    for (k, v) in [
        ("user.name", "Test"),
        ("user.email", "t@example.com"),
        ("commit.gpgsign", "false"),
        ("core.autocrlf", "false"),
    ] {
        git(&repo, &["config", k, v]);
    }
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "a.txt"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    let base = run_git(&repo, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_string();
    let wt = repo.join(".zcode").join("worktrees").join("agent_x");
    let wt_str = wt.to_string_lossy().to_string();
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "zcode/agent_x",
            &wt_str,
            &base,
        ],
    );
    git(&repo, &["config", "branch.zcode/agent_x.zcodeBase", &base]);
    std::fs::write(wt.join("a.txt"), "one\ntwo\n").unwrap();
    std::fs::write(wt.join("b.txt"), "new\n").unwrap();
    git(&wt, &["add", "-A"]);
    git(&wt, &["commit", "-q", "-m", "zcode agent agent_x: work"]);
    // The main checkout moves on; the comparison stays on the recorded base.
    std::fs::write(repo.join("c.txt"), "main\n").unwrap();
    git(&repo, &["add", "c.txt"]);
    git(&repo, &["commit", "-q", "-m", "main moves"]);

    let branches = load_agent_branches(&repo).unwrap();
    assert_eq!(branches.len(), 1);
    let b = &branches[0];
    assert_eq!(
        (b.branch.as_str(), b.base.as_str()),
        ("zcode/agent_x", base.as_str())
    );
    assert_eq!((b.files, b.add, b.del), (2, 2, 0));

    let status = load_compare(b).unwrap();
    let paths: Vec<_> = status
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.worktree))
        .collect();
    assert_eq!(paths, vec![("a.txt", 'M'), ("b.txt", 'A')]);
    let args = compare_file_diff_args(b, "a.txt");
    let diff = run_git(
        &b.repo_top,
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(diff.contains("+two"), "{diff}");

    git(&repo, &["worktree", "remove", "--force", &wt_str]);
    let _ = std::fs::remove_dir_all(&repo);
}
