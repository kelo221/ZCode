//! Git CLI bridge for the Review pane. Shells out to the local `git`
//! executable (PARITY.md M3 decision: the frontend owns it; the backend
//! plays no part). Parsing is pure and golden-tested; commands run on the
//! background executor with CREATE_NO_WINDOW so no console flashes.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// One working-tree entry from `git status --porcelain=v1`.
#[derive(Clone, Debug, PartialEq)]
pub struct GitFile {
    pub path: String,
    /// X column: staged state (' ' when unstaged-only).
    pub index: char,
    /// Y column: worktree state ('?' for untracked).
    pub worktree: char,
    pub staged_add: u32,
    pub staged_del: u32,
    pub unstaged_add: u32,
    pub unstaged_del: u32,
}

impl GitFile {
    pub fn staged_changed(&self) -> bool {
        self.index != ' '
    }
    pub fn unstaged_changed(&self) -> bool {
        self.worktree != ' '
    }
    pub fn untracked(&self) -> bool {
        self.index == '?' && self.worktree == '?'
    }
    /// Short status badge shown before the path ("M", "A", "??", …). When
    /// the viewed column is empty the other column is shown instead.
    pub fn badge(&self, staged_view: bool) -> String {
        let primary = if staged_view { self.index } else { self.worktree };
        let fallback = if staged_view { self.worktree } else { self.index };
        (if primary == ' ' { fallback } else { primary }).to_string()
    }
    pub fn total_add(&self) -> u32 {
        self.staged_add + self.unstaged_add
    }
    pub fn total_del(&self) -> u32 {
        self.staged_del + self.unstaged_del
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GitStatus {
    pub branch: String,
    pub files: Vec<GitFile>,
}

/// Parse `git status --porcelain=v1 -b` output.
pub fn parse_status(output: &str) -> GitStatus {
    let mut status = GitStatus::default();
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            status.branch = rest
                .split("...")
                .next()
                .unwrap_or(rest)
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();
            continue;
        }
        let bytes = line.as_bytes();
        if bytes.len() < 4 {
            continue;
        }
        let index = bytes[0] as char;
        let worktree = bytes[1] as char;
        let path = line[3..].to_string();
        // Rename entries read "old -> new"; track the new path only.
        let path = match path.split_once(" -> ") {
            Some((_, new)) => new.to_string(),
            None => path,
        };
        if path.is_empty() {
            continue;
        }
        status.files.push(GitFile {
            path,
            index,
            worktree,
            staged_add: 0,
            staged_del: 0,
            unstaged_add: 0,
            unstaged_del: 0,
        });
    }
    status
}

/// Parse `git diff --numstat` output: "add\tdelete\tpath".
pub fn parse_numstat(output: &str) -> HashMap<String, (u32, u32)> {
    let mut map = HashMap::new();
    for line in output.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(a), Some(d), Some(p)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        // Binary files report "-"; treat as 0 changed lines.
        let add = a.parse().unwrap_or(0);
        let del = d.parse().unwrap_or(0);
        map.insert(p.to_string(), (add, del));
    }
    map
}

/// Merge numstat maps into a parsed status (staged from --cached, the rest
/// from the worktree diff).
pub fn apply_numstat(
    status: &mut GitStatus,
    staged: HashMap<String, (u32, u32)>,
    unstaged: HashMap<String, (u32, u32)>,
) {
    for f in &mut status.files {
        if let Some((a, d)) = staged.get(&f.path) {
            f.staged_add = *a;
            f.staged_del = *d;
        }
        if let Some((a, d)) = unstaged.get(&f.path) {
            f.unstaged_add = *a;
            f.unstaged_del = *d;
        }
    }
}

/// Which side of the index the Review pane is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffSide {
    #[default]
    Unstaged,
    Staged,
}

impl DiffSide {
    pub fn label(self) -> &'static str {
        match self {
            DiffSide::Unstaged => "Unstaged",
            DiffSide::Staged => "Staged",
        }
    }
    fn cache_key(self, path: &str) -> String {
        format!("{}:{path}", if self == DiffSide::Staged { "s" } else { "u" })
    }
}

/// UI-side cache for the Review pane (pure data; effects live in
/// review/pane.rs).
#[derive(Default)]
pub struct GitState {
    pub status: GitStatus,
    pub side: DiffSide,
    /// Paths currently showing their inline diff.
    pub expanded: std::collections::HashSet<String>,
    /// Cached diff texts keyed by (side, path).
    pub diffs: HashMap<String, String>,
    /// Diff cache keys with a load in flight (dedupes repeated requests).
    pub pending: std::collections::HashSet<String>,
    pub busy: bool,
    /// Outcome of the last stage/commit/push action: (ok, message).
    pub notice: Option<(bool, String)>,
    /// Workspace the cached status/diffs belong to.
    pub root: Option<std::path::PathBuf>,
    /// Workspace of the most recently requested refresh.
    pub requested: Option<std::path::PathBuf>,
    /// Bumped per refresh; async results from older generations are dropped
    /// so a slow, superseded refresh cannot overwrite newer state.
    pub generation: u64,
}

impl GitState {
    /// Adopt a fresh status for `root`, invalidating every cached diff (the
    /// working tree may have changed since they were loaded).
    pub fn apply_status(&mut self, root: std::path::PathBuf, status: GitStatus) {
        if self.root.as_ref() != Some(&root) {
            self.expanded.clear();
            self.notice = None;
        }
        let paths: std::collections::HashSet<&str> =
            status.files.iter().map(|f| f.path.as_str()).collect();
        self.expanded.retain(|p| paths.contains(p.as_str()));
        self.diffs.clear();
        self.pending.clear();
        self.status = status;
        self.root = Some(root);
    }
    pub fn mark_pending(&mut self, side: DiffSide, path: &str) -> bool {
        self.pending.insert(side.cache_key(path))
    }
    /// Expanded paths whose diff for the current side is neither cached nor
    /// already loading.
    pub fn missing_diffs(&self) -> Vec<String> {
        self.expanded
            .iter()
            .filter(|p| {
                let key = self.side.cache_key(p);
                !self.diffs.contains_key(&key) && !self.pending.contains(&key)
            })
            .cloned()
            .collect()
    }
    pub fn files_for_side(&self) -> Vec<&GitFile> {
        self.status
            .files
            .iter()
            .filter(|f| match self.side {
                DiffSide::Staged => f.staged_changed() || f.untracked(),
                DiffSide::Unstaged => f.unstaged_changed(),
            })
            .collect()
    }
    pub fn totals(&self) -> (usize, u32, u32) {
        let mut add = 0;
        let mut del = 0;
        for f in &self.status.files {
            add += f.total_add();
            del += f.total_del();
        }
        (self.status.files.len(), add, del)
    }
    pub fn diff_for(&self, side: DiffSide, path: &str) -> Option<&String> {
        self.diffs.get(&side.cache_key(path))
    }
    pub fn store_diff(&mut self, side: DiffSide, path: &str, text: String) {
        let key = side.cache_key(path);
        self.pending.remove(&key);
        self.diffs.insert(key, text);
    }
}

/// Run git in `cwd` and return stdout (stderr is folded in on failure).
/// Blocking — call from the background executor only.
pub fn run_git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(cwd);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
