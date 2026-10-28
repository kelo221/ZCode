//! Sidebar project order across runs: the newest task activity of every
//! project whose list has arrived is remembered (shared/project_activity.rs),
//! so projects whose agent hasn't started yet still sort by their last known
//! activity instead of falling to discovery order (PARITY.md "Project order").

use crate::app::store::AppState;
use crate::backend::workspace::{WorkspaceHandle, WorkspacePurpose};
use crate::sessions::items::{filter_session_indices, latest_activity};
use crate::shared::project_activity::ProjectActivity;
use gpui::{AppContext, Context};

impl AppState {
    /// Tests never read the real home file; the phone has no projects of its own.
    pub(crate) fn load_project_activity(remote: bool) -> ProjectActivity {
        if cfg!(test) || remote {
            ProjectActivity::default()
        } else {
            ProjectActivity::load(crate::shared::project_activity::path())
        }
    }

    /// After a sessions-index snapshot or delta: remember the project's newest
    /// listed (non-subagent) activity, the same value the sidebar sorts by.
    pub(crate) fn note_project_activity(&mut self, ws_key: &str) {
        let latest = {
            let Some(ws) = self.ws(ws_key) else { return };
            if ws.purpose != WorkspacePurpose::Project {
                return;
            }
            let child_sids = self.all_child_session_ids();
            let indices = filter_session_indices(&ws.sessions, &child_sids, "");
            latest_activity(&ws.sessions, &indices)
        };
        let key = ws_key.to_string();
        self.project_activity.record(&key, latest);
    }

    /// Sort key for a project row: its live list, or what was last seen of it
    /// when the list hasn't arrived this run.
    pub(crate) fn project_sort_activity(
        &self,
        ws: &WorkspaceHandle,
        indices: &[usize],
    ) -> Option<i64> {
        latest_activity(&ws.sessions, indices).max(self.project_activity.get(&ws.key))
    }

    /// Maintenance sweep: write the remembered activity off the UI thread.
    pub(crate) fn flush_project_activity(&mut self, cx: &mut Context<Self>) {
        if let Some((path, text)) = self.project_activity.take_write() {
            cx.background_spawn(async move {
                let _ = crate::shared::project_activity::write(&path, &text);
            })
            .detach();
        }
    }

    /// App quit: the background executor may not run any more.
    pub(crate) fn flush_project_activity_now(&mut self) {
        if let Some((path, text)) = self.project_activity.take_write() {
            let _ = crate::shared::project_activity::write(&path, &text);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{store::AppState, test_support::RUNTIME_TEST_LOCK};
    use crate::backend::workspace::WorkspaceHandle;
    use crate::conversation::model::SessionEntry;
    use gpui::TestApp;

    fn at(sid: &str, ts: i64) -> SessionEntry {
        SessionEntry {
            session_id: sid.into(),
            title: sid.into(),
            phase: "completedSuccess".into(),
            last_activity_at: Some(ts),
            sort_activity_at: Some(ts),
            preview: String::new(),
            has_background_work: false,
        }
    }

    #[test]
    fn unloaded_project_sorts_by_remembered_activity() {
        let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut app = TestApp::new();
        let state = app.new_entity(AppState::for_test);
        app.update_entity(&state, |s, _| {
            let loaded_key = s.workspaces[0].key.clone();
            s.workspaces[0].sessions = vec![at("a", 100), at("b", 300)];
            s.child_owner
                .insert("b".into(), (loaded_key.clone(), "a".into()));
            s.note_project_activity(&loaded_key);
            // Subagent sessions don't count, matching the sidebar.
            assert_eq!(s.project_activity.get(&loaded_key), Some(100));

            // A project whose agent never started this run, seen before at 500.
            let unloaded = WorkspaceHandle::new(std::env::temp_dir().join("never-opened"), vec![]);
            s.project_activity.record(&unloaded.key, Some(500));
            assert_eq!(s.project_sort_activity(&unloaded, &[]), Some(500));

            // Once its list arrives, newer live activity wins.
            let mut opened = unloaded;
            opened.sessions = vec![at("c", 900)];
            assert_eq!(s.project_sort_activity(&opened, &[0]), Some(900));
        });
    }
}
