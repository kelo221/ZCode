//! Workspace file index behind `@` file mentions (COMPOSER_PARITY.md "@ file
//! index"). One index per workspace root, built on a background thread with
//! the `ignore` walker (`.gitignore`, `.ignore`, `.zcodeignore`, no depth or
//! count limit), kept until a watched change marks it dirty, then rebuilt the
//! next time `@` searches it. Ranking is fzf-style (`composer/fuzzy.rs`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Directories never worth mentioning, even without an ignore file (Unity's
/// `Library`/`Temp`/`Logs`/`UserSettings` hold tens of thousands of entries).
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "Library",
    "Temp",
    "Logs",
    "UserSettings",
    "obj",
];
/// Like Desktop's `fileService` cache: at most this many workspaces stay indexed.
const MAX_INDEXES: usize = 4;
/// Quiet time after a file change before a search rebuilds.
const REBUILD_DEBOUNCE: Duration = Duration::from_millis(500);
/// Larger indexes rank off the UI thread (~20 ms per 100k entries in release).
const SYNC_RANK_MAX: usize = 20_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileHit {
    /// `/`-separated path relative to the root; folders end with `/`.
    pub(crate) rel: String,
    pub(crate) name: String,
    pub(crate) is_dir: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Search {
    pub(crate) hits: Vec<FileHit>,
    /// True while the first build runs (nothing to search yet).
    pub(crate) indexing: bool,
    /// A large index is ranking this query in the background; `hits` are the
    /// previous query's until it finishes.
    pub(crate) searching: bool,
}

struct Entry {
    hit: FileHit,
    lower_rel: String,
}

#[derive(Default)]
struct Shared {
    entries: Option<Arc<Vec<Entry>>>,
    built_at: Option<Instant>,
    changed_at: Option<Instant>,
    /// Query that already triggered a rebuild-on-miss (once per query).
    missed: Option<String>,
    memo: Option<(String, usize, Arc<Vec<FileHit>>)>,
    /// `(query, limit)` being ranked on a background thread.
    ranking: Option<(String, usize)>,
}

struct Index {
    root: PathBuf,
    shared: Arc<Mutex<Shared>>,
    building: Arc<AtomicBool>,
    used_at: Instant,
    _watcher: Option<notify::RecommendedWatcher>,
}

fn registry() -> &'static Mutex<HashMap<PathBuf, Index>> {
    static REG: OnceLock<Mutex<HashMap<PathBuf, Index>>> = OnceLock::new();
    REG.get_or_init(Default::default)
}

/// Best `limit` files and folders under `root` for `query` (lowercase or not).
pub(crate) fn search(root: &Path, query: &str, limit: usize) -> Search {
    let mut reg = registry().lock().unwrap_or_else(|e| e.into_inner());
    if !reg.contains_key(root) {
        if reg.len() >= MAX_INDEXES
            && let Some(oldest) = reg
                .iter()
                .min_by_key(|(_, i)| i.used_at)
                .map(|(k, _)| k.clone())
        {
            reg.remove(&oldest);
        }
        reg.insert(root.to_path_buf(), Index::new(root));
    }
    let index = reg.get_mut(root).expect("inserted above");
    index.used_at = Instant::now();
    index.search(query, limit)
}

/// An index build or a background ranking is still running for `root`, so
/// the `@` popup should look again shortly. Never creates an index.
pub(crate) fn pending(root: &Path) -> bool {
    let reg = registry().lock().unwrap_or_else(|e| e.into_inner());
    reg.get(root).is_some_and(Index::pending)
}

#[path = "file_index_named.rs"]
mod named;
pub(crate) use named::{count_named_if_indexed, files_named};

/// Build synchronously (tests).
#[cfg(test)]
pub(crate) fn build_now(root: &Path) -> Vec<FileHit> {
    walk(root).into_iter().map(|e| e.hit).collect()
}

impl Index {
    fn new(root: &Path) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let watcher = watch(root, shared.clone());
        let index = Self {
            root: root.to_path_buf(),
            shared,
            building: Arc::new(AtomicBool::new(false)),
            used_at: Instant::now(),
            _watcher: watcher,
        };
        index.rebuild();
        index
    }

    fn rebuild(&self) {
        if self.building.swap(true, Ordering::AcqRel) {
            return;
        }
        let (root, shared, building) = (
            self.root.clone(),
            self.shared.clone(),
            self.building.clone(),
        );
        std::thread::Builder::new()
            .name("zcode-file-index".into())
            .spawn(move || {
                let started = Instant::now();
                let entries = walk(&root);
                let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                s.entries = Some(Arc::new(entries));
                s.built_at = Some(started);
                s.memo = None;
                building.store(false, Ordering::Release);
            })
            .map_err(|_| self.building.store(false, Ordering::Release))
            .ok();
    }

    fn search(&self, query: &str, limit: usize) -> Search {
        let query = query.trim().to_lowercase();
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entries) = s.entries.clone() else {
            return Search {
                hits: vec![],
                indexing: true,
                searching: false,
            };
        };
        let stale = matches!((s.changed_at, s.built_at), (Some(c), Some(b)) if c > b)
            && s.changed_at
                .is_some_and(|c| c.elapsed() >= REBUILD_DEBOUNCE);
        if stale {
            drop(s);
            self.rebuild();
            s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        }
        let memo = s
            .memo
            .as_ref()
            .filter(|(q, l, _)| *q == query && *l == limit)
            .map(|(_, _, hits)| hits.clone());
        let hits = match memo {
            Some(hits) => hits,
            None if entries.len() <= SYNC_RANK_MAX => {
                let hits = Arc::new(rank(&entries, &query, limit));
                s.memo = Some((query.clone(), limit, hits.clone()));
                hits
            }
            None => {
                self.rank_in_background(&mut s, entries, &query, limit);
                return Search {
                    hits: s
                        .memo
                        .as_ref()
                        .map_or_else(Vec::new, |(_, _, h)| h.to_vec()),
                    indexing: false,
                    searching: true,
                };
            }
        };
        // A file created where the watcher could not see it: one rebuild per missed query.
        if hits.is_empty() && !query.is_empty() && s.missed.as_deref() != Some(&query) {
            s.missed = Some(query);
            drop(s);
            self.rebuild();
        }
        Search {
            hits: hits.to_vec(),
            indexing: false,
            searching: false,
        }
    }

    fn rank_in_background(
        &self,
        s: &mut Shared,
        entries: Arc<Vec<Entry>>,
        query: &str,
        limit: usize,
    ) {
        let job = (query.to_string(), limit);
        if s.ranking.as_ref() == Some(&job) {
            return;
        }
        s.ranking = Some(job.clone());
        let shared = self.shared.clone();
        std::thread::Builder::new()
            .name("zcode-file-rank".into())
            .spawn(move || {
                let hits = Arc::new(rank(&entries, &job.0, job.1));
                let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                if s.ranking.as_ref() != Some(&job) {
                    return;
                }
                s.ranking = None;
                // A rebuild finished meanwhile: these hits are from the old entries.
                if s.entries.as_ref().is_some_and(|e| Arc::ptr_eq(e, &entries)) {
                    s.memo = Some((job.0, job.1, hits));
                }
            })
            .ok();
    }

    fn pending(&self) -> bool {
        self.building.load(Ordering::Acquire)
            || self
                .shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .ranking
                .is_some()
    }
}

fn watch(root: &Path, shared: Arc<Mutex<Shared>>) -> Option<notify::RecommendedWatcher> {
    use notify::Watcher;
    let base = root.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        if event.kind.is_access() || event.paths.iter().all(|p| in_skipped_dir(&base, p)) {
            return;
        }
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        s.changed_at = Some(Instant::now());
        s.missed = None;
    })
    .ok()?;
    watcher.watch(root, notify::RecursiveMode::Recursive).ok()?;
    Some(watcher)
}

/// Whether `path` lies under a [`SKIP_DIRS`] folder inside `root`.
fn in_skipped_dir(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .any(|c| SKIP_DIRS.iter().any(|s| c.as_os_str() == *s))
}

fn walk(root: &Path) -> Vec<Entry> {
    let found = Mutex::new(Vec::new());
    ignore::WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .add_custom_ignore_filename(".zcodeignore")
        .filter_entry(|e| {
            let dir = e.file_type().is_some_and(|t| t.is_dir());
            let name = e.file_name();
            !(dir && SKIP_DIRS.iter().any(|s| name == *s))
                && !(!dir && Path::new(name).extension().is_some_and(|x| x == "meta"))
        })
        .build_parallel()
        .run(|| {
            let found = &found;
            Box::new(move |res| {
                if let Ok(e) = res
                    && e.depth() > 0
                    && let Ok(rel) = e.path().strip_prefix(root)
                {
                    let is_dir = e.file_type().is_some_and(|t| t.is_dir());
                    let mut rel = rel.to_string_lossy().replace('\\', "/");
                    if is_dir {
                        rel.push('/');
                    }
                    let name = e.file_name().to_string_lossy().into_owned();
                    found.lock().unwrap_or_else(|e| e.into_inner()).push(Entry {
                        lower_rel: rel.trim().to_lowercase(),
                        hit: FileHit { rel, name, is_dir },
                    });
                }
                ignore::WalkState::Continue
            })
        });
    let mut entries = found.into_inner().unwrap_or_else(|e| e.into_inner());
    // Shallow paths first, then alphabetical: a stable default order for an empty query.
    entries.sort_by(|a, b| {
        let depth = |e: &Entry| e.hit.rel.trim_end_matches('/').matches('/').count();
        depth(a)
            .cmp(&depth(b))
            .then_with(|| a.hit.rel.cmp(&b.hit.rel))
    });
    entries
}

/// Extra score for matching inside the file name rather than only across the
/// path, so `player` prefers `Player.cs` over `player/README.md`.
const NAME_BONUS: i32 = 8;

/// fzf-style score of `query` against one entry: the better of its file name
/// (plus [`NAME_BONUS`]) and its whole relative path.
fn entry_score(
    e: &Entry,
    query: &[char],
    buf: &mut Vec<char>,
    sc: &mut crate::composer::fuzzy::Scratch,
) -> Option<i32> {
    use crate::composer::fuzzy::score_with;
    buf.clear();
    buf.extend(e.hit.rel.trim_end_matches('/').chars());
    let name_start = buf.iter().rposition(|&c| c == '/').map_or(0, |i| i + 1);
    let name = score_with(&buf[name_start..], query, sc).map(|s| s + NAME_BONUS);
    let rel = score_with(buf, query, sc);
    name.max(rel)
}

fn rank(entries: &[Entry], query: &str, limit: usize) -> Vec<FileHit> {
    if query.is_empty() {
        // Desktop default: files before folders, otherwise index order.
        let files = entries.iter().filter(|e| !e.hit.is_dir);
        let dirs = entries.iter().filter(|e| e.hit.is_dir);
        return files
            .chain(dirs)
            .take(limit)
            .map(|e| e.hit.clone())
            .collect();
    }
    let q: Vec<char> = query.chars().collect();
    let mut buf = Vec::new();
    let mut sc = crate::composer::fuzzy::Scratch::default();
    // Best first: higher score, then the shorter path, then index order.
    let mut scored: Vec<(std::cmp::Reverse<i32>, usize, usize)> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| crate::composer::fuzzy::is_subsequence(&e.lower_rel, query))
        .filter_map(|(i, e)| {
            let score = entry_score(e, &q, &mut buf, &mut sc)?;
            Some((std::cmp::Reverse(score), e.hit.rel.len(), i))
        })
        .collect();
    let take = limit.min(scored.len());
    if take == 0 {
        return vec![];
    }
    scored.select_nth_unstable(take - 1);
    scored.truncate(take);
    scored.sort_unstable();
    scored
        .into_iter()
        .map(|(_, _, i)| entries[i].hit.clone())
        .collect()
}

#[cfg(test)]
#[path = "file_index_tests.rs"]
mod tests;
