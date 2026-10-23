//! Git CLI bridge for the Review pane. Shells out to the local `git`
//! executable (PARITY.md M3 decision: the frontend owns it; the backend
//! plays no part). Parsing is pure and golden-tested; commands run on the
//! background executor with CREATE_NO_WINDOW so no console flashes.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

pub use crate::review::git_state::GitState;

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
        let primary = if staged_view {
            self.index
        } else {
            self.worktree
        };
        let fallback = if staged_view {
            self.worktree
        } else {
            self.index
        };
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
    pub branches: Vec<String>,
    pub files: Vec<GitFile>,
}

/// Parse `git status --porcelain=v1 -z -b` (NUL records; newline still ok).
pub fn parse_status(output: &str) -> GitStatus {
    let mut status = GitStatus::default();
    let records: Vec<&str> = if output.contains('\0') {
        output.split('\0').filter(|s| !s.is_empty()).collect()
    } else {
        output.lines().collect()
    };
    let mut i = 0;
    while i < records.len() {
        let rec = records[i];
        i += 1;
        if let Some(rest) = rec.strip_prefix("## ") {
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
        let bytes = rec.as_bytes();
        if bytes.len() < 4 {
            continue;
        }
        let index = bytes[0] as char;
        let worktree = bytes[1] as char;
        let mut path = rec[3..].to_string();
        if matches!(index, 'R' | 'C') {
            if let Some((_, new)) = path.split_once(" -> ") {
                path = new.to_string();
            } else if i < records.len() {
                path = records[i].to_string();
                i += 1;
            }
        }
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

/// Parse `git diff --numstat -z`: "add\\tdelete\\tpath\\0" (renames:
/// "add\\tdelete\\0old\\0new\\0"). Newline form still accepted.
pub fn parse_numstat(output: &str) -> HashMap<String, (u32, u32)> {
    let mut map = HashMap::new();
    if output.contains('\0') {
        let recs: Vec<&str> = output.split('\0').filter(|s| !s.is_empty()).collect();
        let mut i = 0;
        while i < recs.len() {
            let rec = recs[i];
            i += 1;
            let mut parts = rec.splitn(3, '\t');
            let (Some(a), Some(d), path_or_empty) = (parts.next(), parts.next(), parts.next())
            else {
                continue;
            };
            let path = if let Some(p) = path_or_empty.filter(|p| !p.is_empty()) {
                p.to_string()
            } else if i + 1 < recs.len() {
                i += 1;
                let new = recs[i].to_string();
                i += 1;
                new
            } else {
                continue;
            };
            map.insert(path, (a.parse().unwrap_or(0), d.parse().unwrap_or(0)));
        }
        return map;
    }
    for line in output.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(a), Some(d), Some(p)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        map.insert(
            p.to_string(),
            (a.parse().unwrap_or(0), d.parse().unwrap_or(0)),
        );
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
    pub(crate) fn cache_key(self, path: &str) -> String {
        format!(
            "{}:{path}",
            if self == DiffSide::Staged { "s" } else { "u" }
        )
    }
}

/// Wait budget for one git invocation. Pushes of sizeable packs can be slow,
/// but an infinite hang (credential prompt, wedged helper) must never freeze
/// the Review pane's background executor slot.
const GIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
/// Read cap per stream: display-only output never needs more than this.
const MAX_GIT_OUTPUT_BYTES: u64 = 4 * 1024 * 1024;

/// Run git in `cwd` and return stdout (stderr is folded in on failure).
/// Blocking — call from the background executor only.
///
/// Hardened against the failure modes the 2026-10-05 audit listed: credential
/// prompts fail fast (GIT_TERMINAL_PROMPT=0 / GCM_INTERACTIVE=Never), every
/// invocation has a deadline, and both pipes are drained on threads so a
/// large diff cannot deadlock a full pipe buffer.
pub fn run_git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    run_vcs("git", cwd, args)
}

/// Blocking: `jj` with the same deadline and pipe handling as [`run_git`].
/// Callers pass `--ignore-working-copy` so the Review pane never snapshots
/// (and so never rewrites) the user's working-copy commit.
pub fn run_jj(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let mut full = vec!["--no-pager", "--color=never"];
    full.extend_from_slice(args);
    run_vcs("jj", cwd, &full)
}

fn run_vcs(program: &str, cwd: &Path, args: &[&str]) -> Result<String, String> {
    use std::process::Stdio;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GCM_INTERACTIVE", "Never")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    fn drain<R: std::io::Read + Send + 'static>(
        pipe: Option<R>,
    ) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut kept = Vec::new();
            if let Some(mut pipe) = pipe {
                // Drain to EOF so the child never blocks on a full pipe or
                // dies with EPIPE after the cap, while retaining only a
                // capped prefix for display (review finding 11.1).
                let mut chunk = [0u8; 8192];
                loop {
                    match pipe.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let room = (MAX_GIT_OUTPUT_BYTES as usize).saturating_sub(kept.len());
                            kept.extend_from_slice(&chunk[..n.min(room)]);
                        }
                    }
                }
            }
            kept
        })
    }
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let deadline = std::time::Instant::now() + GIT_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Ok(st),
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(format!(
                    "{program} timed out after {}s",
                    GIT_TIMEOUT.as_secs()
                ));
            }
            Err(e) => break Err(e.to_string()),
        }
    };
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();
    match status {
        Ok(st) if st.success() => Ok(String::from_utf8_lossy(&out).into_owned()),
        Ok(st) => Err(format_git_failure(
            st.code(),
            &String::from_utf8_lossy(&out),
            &String::from_utf8_lossy(&err),
        )),
        Err(e) => Err(e),
    }
}

/// Failure text: stderr, or stdout when stderr is empty (hooks often print
/// there), plus the exit code, capped to a few lines.
pub fn format_git_failure(code: Option<i32>, stdout: &str, stderr: &str) -> String {
    let body = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    let body: Vec<&str> = body.lines().take(12).collect();
    let code = code.map_or_else(|| "a signal".to_string(), |c| c.to_string());
    if body.is_empty() {
        format!("git exited with {code}")
    } else {
        format!(
            "{} (exit {code})",
            body.join(
                "
"
            )
        )
    }
}

/// Read cap for untracked previews: only `MAX_DIFF_LINES` are shown, so a
/// multi-GB log must not be loaded whole.
const MAX_UNTRACKED_BYTES: u64 = 256 * 1024;
const MAX_DIFF_LINES: usize = 400;

pub fn untracked_as_diff(root: &Path, rel: &str) -> String {
    use std::io::Read;
    let mut bytes = Vec::new();
    let read = std::fs::File::open(root.join(rel))
        .and_then(|f| f.take(MAX_UNTRACKED_BYTES).read_to_end(&mut bytes));
    match read {
        Ok(_) if bytes.contains(&0) => "(binary file)".to_string(),
        Ok(_) => String::from_utf8_lossy(&bytes)
            .lines()
            .take(MAX_DIFF_LINES)
            .map(|l| format!("+{l}"))
            .collect::<Vec<_>>()
            .join("\n"),
        Err(e) => format!("(cannot read untracked file: {e})"),
    }
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
