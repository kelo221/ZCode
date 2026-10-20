//! UI-side cache for the Review pane (pure data; effects live in
//! review/pane.rs and review/agent_branches_view.rs). Split from
//! review/git.rs for the 400-line cap.

use crate::review::agent_branches::{AgentBranch, ReviewTarget};
use crate::review::git::{DiffSide, GitFile, GitStatus};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct GitState {
    pub status: GitStatus,
    pub side: DiffSide,
    /// Paths currently showing their inline diff.
    pub expanded: HashSet<String>,
    /// Cached diff texts keyed by (target, side, path).
    pub diffs: HashMap<String, String>,
    /// Diff cache keys with a load in flight (dedupes repeated requests).
    pub pending: HashSet<String>,
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
    /// Destructive ops (discard) arm first: the path awaiting its second
    /// click, if any.
    pub confirm_discard: Option<String>,
    /// Working tree, or a pinned agent-branch comparison (read-only).
    pub target: ReviewTarget,
    /// Agent branches of the active repository.
    pub agent_branches: Vec<AgentBranch>,
    /// Generation of the agent-branch listing (independent of `generation`).
    pub branches_generation: u64,
}

impl GitState {
    /// Adopt a fresh status for `root`, invalidating every cached diff (the
    /// working tree may have changed since they were loaded).
    pub fn apply_status(&mut self, root: std::path::PathBuf, status: GitStatus) {
        if self.root.as_ref() != Some(&root) {
            self.expanded.clear();
            self.notice = None;
            self.confirm_discard = None;
        }
        let paths: HashSet<&str> = status.files.iter().map(|f| f.path.as_str()).collect();
        self.expanded.retain(|p| paths.contains(p.as_str()));
        self.diffs.clear();
        self.pending.clear();
        self.status = status;
        self.root = Some(root);
    }
    /// Switch between the working tree and an agent-branch comparison. Every
    /// cached row belongs to the old target, and the generation bump drops
    /// any status or diff still loading for it.
    pub fn set_target(&mut self, target: ReviewTarget) -> bool {
        if self.target == target {
            return false;
        }
        self.target = target;
        self.status = GitStatus::default();
        self.expanded.clear();
        self.diffs.clear();
        self.pending.clear();
        self.notice = None;
        self.confirm_discard = None;
        self.root = None;
        self.generation += 1;
        true
    }
    pub fn diff_key(&self, side: DiffSide, path: &str) -> String {
        format!("{}|{}", self.target.cache_prefix(), side.cache_key(path))
    }
    pub fn mark_pending(&mut self, side: DiffSide, path: &str) -> bool {
        let key = self.diff_key(side, path);
        self.pending.insert(key)
    }
    /// Expanded paths whose diff for the current side is neither cached nor
    /// already loading.
    pub fn missing_diffs(&self) -> Vec<String> {
        self.expanded
            .iter()
            .filter(|p| {
                let key = self.diff_key(self.side, p);
                !self.diffs.contains_key(&key) && !self.pending.contains(&key)
            })
            .cloned()
            .collect()
    }
    pub fn files_for_side(&self) -> Vec<&GitFile> {
        self.status
            .files
            .iter()
            .filter(|f| {
                self.target.is_compare()
                    || match self.side {
                        DiffSide::Staged => f.staged_changed() || f.untracked(),
                        DiffSide::Unstaged => f.unstaged_changed(),
                    }
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
        self.diffs.get(&self.diff_key(side, path))
    }
    /// Store a diff under the key it was requested with.
    pub fn store_diff_key(&mut self, key: String, text: String) {
        self.pending.remove(&key);
        self.diffs.insert(key, text);
    }
}
