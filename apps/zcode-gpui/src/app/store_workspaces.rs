//! `AppState` helpers for the built-in conversation ("Tasks") workspace.

use crate::app::store::AppState;
use crate::backend::workspace::{WorkspaceHandle, WorkspacePurpose};

impl AppState {
    pub(crate) fn register_conversation_workspace(
        &mut self,
        candidates: Vec<crate::backend::launcher::BackendLaunch>,
    ) {
        use crate::backend::workspace::{
            canonical_workspace_string, ensure_conversation_workspace_dir,
        };
        let p = ensure_conversation_workspace_dir();
        let key = canonical_workspace_string(&p);
        let mut h = WorkspaceHandle::new(p, candidates);
        h.purpose = WorkspacePurpose::Conversation;
        h.display = "Tasks".into();
        match self.workspaces.iter_mut().find(|w| w.key == key) {
            Some(ws) => *ws = h,
            None => self.workspaces.insert(0, h),
        }
    }

    pub fn conversation_workspace_key(&self) -> Option<String> {
        self.workspaces
            .iter()
            .find(|w| w.purpose == WorkspacePurpose::Conversation)
            .map(|w| w.key.clone())
    }

    pub fn is_conversation_workspace(&self, ws_key: &str) -> bool {
        self.ws(ws_key)
            .is_some_and(|w| w.purpose == WorkspacePurpose::Conversation)
    }

    /// Remove a project from the sidebar (project context menu): stop its
    /// agent, drop every local mirror of its sessions and forget it in
    /// `setting.json`. Nothing on disk is deleted; the folder can be opened
    /// again. Refused while it has live work, since stopping the agent would
    /// end it. Returns whether the project was removed.
    pub(crate) fn remove_workspace(&mut self, key: &str, cx: &mut gpui::Context<Self>) -> bool {
        if self.phone_client.enabled {
            return false;
        }
        let Some(ws) = self.ws(key) else {
            return false;
        };
        if ws.purpose != WorkspacePurpose::Project {
            return false;
        }
        let display = ws.display.clone();
        if self.workspace_busy(key) {
            self.push_error(format!(
                "{display} has running tasks; stop them before removing the project"
            ));
            cx.notify();
            return false;
        }
        if self.active_ws_key().as_deref() == Some(key) {
            let next = self
                .workspaces
                .iter()
                .find(|w| w.key != key && w.purpose == WorkspacePurpose::Project)
                .or_else(|| {
                    self.workspaces
                        .iter()
                        .find(|w| w.purpose == WorkspacePurpose::Conversation)
                })
                .map(|w| w.key.clone());
            match next {
                Some(next) => self.set_active_workspace(&next, cx),
                None => {
                    self.active = None;
                    self.active_workspace = None;
                }
            }
        }
        self.unload_workspace(key);
        let session_ids: Vec<String> = self
            .ws(key)
            .map(|w| w.sessions.iter().map(|s| s.session_id.clone()).collect())
            .unwrap_or_default();
        for sid in &session_ids {
            self.conversations.remove(sid);
            self.session_drafts.remove(sid);
            self.draft_submission_overrides.remove(sid);
            self.recovered_attachments.remove(sid);
        }
        self.session_drafts.remove(&format!("draft:{key}"));
        self.draft_models.configs.remove(&format!("draft:{key}"));
        self.child_owner.retain(|_, (ws_key, _)| ws_key != key);
        self.workspace_configs.remove(key);
        self.workspaces.retain(|w| w.key != key);
        crate::shared::preferences::Preferences::enqueue(
            crate::shared::preferences::PreferenceChange::RemoveProject(key.to_string()),
            cx,
        );
        self.push_log(format!("removed project {key}"));
        cx.notify();
        true
    }
}

#[cfg(test)]
#[path = "store_workspaces_tests.rs"]
mod tests;
