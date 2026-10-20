//! Agent branches (read-only): worktrees the CLI created for `isolation:
//! "worktree"` subagents, and the pinned `base..head` comparison the Review
//! pane shows for one of them (PARITY.md "Agent branches (read-only)").
//! Parsing is pure; the blocking loaders run on the background executor.

use crate::review::git::{GitFile, GitStatus, parse_numstat, run_git};
use std::path::{Path, PathBuf};

const AGENT_BRANCH_PREFIX: &str = "refs/heads/zcode/agent_";

/// One `git worktree list --porcelain` entry that belongs to an agent.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentWorktree {
    pub path: PathBuf,
    /// Short branch name, e.g. `zcode/agent_1`.
    pub branch: String,
    pub head: String,
}

/// A reviewable agent branch: the comparison is pinned to `base..result`.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentBranch {
    pub repo_top: PathBuf,
    pub worktree: PathBuf,
    pub branch: String,
    pub base: String,
    pub result: String,
    pub files: u32,
    pub add: u32,
    pub del: u32,
}

impl AgentBranch {
    pub fn label(&self) -> String {
        format!(
            "{} · {}..{}",
            self.branch,
            short_sha(&self.base),
            short_sha(&self.result)
        )
    }
}

/// What the Review pane is showing.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ReviewTarget {
    #[default]
    WorkingTree,
    AgentBranch(AgentBranch),
}

impl ReviewTarget {
    pub fn is_compare(&self) -> bool {
        matches!(self, ReviewTarget::AgentBranch(_))
    }
    /// Prefix for diff-cache keys, so diffs never cross targets.
    pub fn cache_prefix(&self) -> String {
        match self {
            ReviewTarget::WorkingTree => "wt".to_string(),
            ReviewTarget::AgentBranch(b) => format!("{}..{}", b.base, b.result),
        }
    }
}

pub fn short_sha(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

/// Agent worktrees from `git worktree list --porcelain`: under
/// `<repo_top>/.zcode/worktrees/` and on a `zcode/agent_*` branch.
pub fn parse_worktree_list(output: &str, repo_top: &Path) -> Vec<AgentWorktree> {
    let root = format!(
        "{}/.zcode/worktrees/",
        path_key(&repo_top.to_string_lossy())
    );
    let mut found = Vec::new();
    for block in output.replace("\r\n", "\n").split("\n\n") {
        let (mut path, mut head, mut branch) = (None, None, None);
        for line in block.lines() {
            if let Some(v) = line.strip_prefix("worktree ") {
                path = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("HEAD ") {
                head = Some(v.to_string());
            } else if let Some(v) = line.strip_prefix("branch ") {
                branch = Some(v.to_string());
            }
        }
        let (Some(path), Some(head), Some(branch)) = (path, head, branch) else {
            continue;
        };
        if !branch.starts_with(AGENT_BRANCH_PREFIX) || !path_key(&path).starts_with(&root) {
            continue;
        }
        found.push(AgentWorktree {
            path: PathBuf::from(path),
            branch: branch.trim_start_matches("refs/heads/").to_string(),
            head,
        });
    }
    found
}

/// Comparable form of a path: forward slashes, no trailing slash, and
/// case-folded on Windows (git prints `C:/x`, Rust may hold `c:\x`).
fn path_key(path: &str) -> String {
    let key = path.replace('\\', "/").trim_end_matches('/').to_string();
    if cfg!(windows) {
        key.to_lowercase()
    } else {
        key
    }
}

/// `git diff --shortstat` → (files, insertions, deletions).
pub fn parse_shortstat(output: &str) -> (u32, u32, u32) {
    let mut stat = (0, 0, 0);
    for part in output.trim().split(',') {
        let mut words = part.split_whitespace();
        let (Some(n), Some(kind)) = (words.next(), words.next()) else {
            continue;
        };
        let n = n.parse().unwrap_or(0);
        if kind.starts_with("file") {
            stat.0 = n;
        } else if kind.starts_with("insertion") {
            stat.1 = n;
        } else if kind.starts_with("deletion") {
            stat.2 = n;
        }
    }
    stat
}

/// `git diff --name-status -z`: "M\0path\0", renames "R100\0old\0new\0".
pub fn parse_name_status(output: &str) -> Vec<(char, String)> {
    let recs: Vec<&str> = output.split('\0').filter(|s| !s.is_empty()).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < recs.len() {
        let letter = recs[i].chars().next().unwrap_or('M');
        i += 1;
        if matches!(letter, 'R' | 'C') {
            i += 1;
        }
        if let Some(path) = recs.get(i) {
            out.push((letter, (*path).to_string()));
        }
        i += 1;
    }
    out
}

/// Files of a pinned comparison, shown with the worktree column (the pane
/// renders them read-only).
pub fn compare_status(branch: &AgentBranch, name_status: &str, numstat: &str) -> GitStatus {
    let stats = parse_numstat(numstat);
    let files = parse_name_status(name_status)
        .into_iter()
        .map(|(letter, path)| {
            let (add, del) = stats.get(&path).copied().unwrap_or((0, 0));
            GitFile {
                path,
                index: ' ',
                worktree: letter,
                staged_add: 0,
                staged_del: 0,
                unstaged_add: add,
                unstaged_del: del,
            }
        })
        .collect();
    GitStatus {
        branch: branch.label(),
        branches: Vec::new(),
        files,
    }
}

pub fn compare_file_diff_args(branch: &AgentBranch, path: &str) -> Vec<String> {
    vec![
        "diff".into(),
        branch.base.clone(),
        branch.result.clone(),
        "--".into(),
        path.to_string(),
    ]
}

/// Blocking: list the agent branches of the repository containing `cwd`.
pub fn load_agent_branches(cwd: &Path) -> Result<Vec<AgentBranch>, String> {
    let top = PathBuf::from(run_git(cwd, &["rev-parse", "--show-toplevel"])?.trim());
    let list = run_git(&top, &["worktree", "list", "--porcelain"])?;
    let mut branches = Vec::new();
    for wt in parse_worktree_list(&list, &top) {
        let recorded = run_git(
            &top,
            &[
                "config",
                "--get",
                &format!("branch.{}.zcodeBase", wt.branch),
            ],
        )
        .ok();
        let base = recorded
            .or_else(|| run_git(&top, &["merge-base", "HEAD", &wt.branch]).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let Some(base) = base else {
            continue;
        };
        let (files, add, del) = run_git(&top, &["diff", "--shortstat", &base, &wt.head])
            .map(|s| parse_shortstat(&s))
            .unwrap_or_default();
        branches.push(AgentBranch {
            repo_top: top.clone(),
            worktree: wt.path,
            branch: wt.branch,
            base,
            result: wt.head,
            files,
            add,
            del,
        });
    }
    Ok(branches)
}

/// Blocking: the file list of one pinned comparison.
pub fn load_compare(branch: &AgentBranch) -> Result<GitStatus, String> {
    let range = [branch.base.as_str(), branch.result.as_str()];
    let names = run_git(
        &branch.repo_top,
        &["diff", "--name-status", "-z", range[0], range[1]],
    )?;
    let numstat = run_git(
        &branch.repo_top,
        &["diff", "--numstat", "-z", range[0], range[1]],
    )?;
    Ok(compare_status(branch, &names, &numstat))
}

#[cfg(test)]
#[path = "agent_branches_tests.rs"]
mod tests;
