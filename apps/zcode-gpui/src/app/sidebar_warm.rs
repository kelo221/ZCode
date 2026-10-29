//! Sidebar ready at startup (PARITY.md "Cached rows and warm agents"): cached
//! rows seed every workspace whose agent hasn't sent its list, then the Tasks
//! agent and the agents of folders open last run start in the background, so
//! their lists and 3 newest conversations (backend/preload.rs) load before
//! they are clicked.

use crate::app::store::AppState;
use crate::backend::workspace::WorkspacePurpose;
use crate::shared::sidebar_cache::SidebarCache;
use gpui::Context;
use std::time::Duration;

/// Gap between background agent starts, so cold starts don't pile up.
const WARM_STAGGER: Duration = Duration::from_millis(400);

impl AppState {
    /// Tests never read the real home file; the phone has no projects of its own.
    pub(crate) fn load_sidebar_cache(remote: bool) -> SidebarCache {
        if cfg!(test) || remote {
            SidebarCache::default()
        } else {
            SidebarCache::load(crate::shared::sidebar_cache::path())
        }
    }

    /// Workspaces to start in the background: Tasks, then open folders.
    pub(crate) fn warm_targets(&self) -> Vec<String> {
        let open = self.sidebar_cache.open_folders();
        let tasks = self
            .workspaces
            .iter()
            .filter(|w| w.purpose == WorkspacePurpose::Conversation);
        let folders = self
            .workspaces
            .iter()
            .filter(|w| w.purpose == WorkspacePurpose::Project && open.contains(&w.key));
        tasks
            .chain(folders)
            .filter(|w| Some(&w.key) != self.active_workspace.as_ref())
            .map(|w| w.key.clone())
            .collect()
    }

    /// Seed cached rows, then start the warm targets one by one.
    pub(crate) fn warm_sidebar(&mut self, cx: &mut Context<Self>) {
        self.seed_cached_sessions();
        let targets = self.warm_targets();
        cx.spawn(async move |this, cx| {
            for key in targets {
                cx.background_executor().timer(WARM_STAGGER).await;
                if this.update(cx, |s, cx| s.ensure_spawned(&key, cx)).is_err() {
                    return;
                }
            }
        })
        .detach();
    }

    pub(crate) fn seed_cached_sessions(&mut self) {
        let Self {
            workspaces,
            sidebar_cache,
            ..
        } = self;
        for ws in workspaces.iter_mut().filter(|w| w.sessions.is_empty()) {
            ws.sessions = sidebar_cache.rows(&ws.key);
            ws.sessions_cached = !ws.sessions.is_empty();
        }
    }

    /// After a live sessions-index change: remember the newest rows.
    pub(crate) fn note_sidebar_rows(&mut self, ws_key: &str) {
        let Self {
            workspaces,
            sidebar_cache,
            ..
        } = self;
        if let Some(ws) = workspaces
            .iter()
            .find(|w| w.key == ws_key && !w.sessions_cached)
        {
            sidebar_cache.record(ws_key, &ws.sessions);
        }
    }

    pub(crate) fn take_sidebar_cache_write(&mut self) -> Option<(std::path::PathBuf, String)> {
        self.sidebar_cache.take_write()
    }
}

#[cfg(test)]
#[path = "sidebar_warm_tests.rs"]
mod tests;
