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
}
