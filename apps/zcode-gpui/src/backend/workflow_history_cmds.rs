use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use crate::shared::workflow_history::WorkflowHistory;
use gpui::Context;
use serde_json::{Value, json};

impl AppState {
    pub(crate) fn fetch_workflow_history(
        &mut self,
        key: &str,
        scope: &str,
        name: &str,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        if self.active_ws_key().as_deref() != Some(key) {
            return;
        }
        let Some(ws) = self.ws_mut(key) else { return };
        if !ws.started
            || ws.inbound.is_none()
            || ws.saved_workflow_form.scope != scope
            || ws.saved_workflow_form.selected.as_deref() != Some(name)
            || !ws
                .saved_workflows
                .get(scope)
                .and_then(|q| q.value.as_ref())
                .is_some_and(|list| list.workflows.iter().any(|w| w.name == name))
        {
            return;
        }
        let cache_key = format!("{scope}\0{name}");
        if !ws.workflow_histories.contains_key(&cache_key) && ws.workflow_histories.len() >= 32 {
            let removable = ws
                .workflow_histories
                .iter()
                .find(|(_, q)| !q.loading)
                .map(|(k, _)| k.clone());
            let Some(removable) = removable else { return };
            ws.workflow_histories.remove(&removable);
        }
        let query = ws.workflow_histories.entry(cache_key.clone()).or_default();
        if query.loading || query.attempted && !refresh {
            return;
        }
        query.start();
        let id = ws.next_id();
        ws.pending.insert(
            id,
            Pending::WorkflowHistory {
                scope: scope.into(),
                name: name.into(),
            },
        );
        if !ws.send_pending_line(id, json!({"id":id,"method":"workflows/runs","params":{"workspace":{"workspacePath":ws.path,"workspaceKey":ws.key},"scope":scope,"name":name,"limit":50}}).to_string()) {
            ws.workflow_histories.get_mut(&cache_key).unwrap().fail("Workflow history could not be requested");
        }
        cx.notify();
    }
    pub(crate) fn settle_workflow_history(
        &mut self,
        key: &str,
        scope: &str,
        name: &str,
        result: Result<Value, String>,
    ) {
        let Some(query) = self
            .ws_mut(key)
            .and_then(|ws| ws.workflow_histories.get_mut(&format!("{scope}\0{name}")))
        else {
            return;
        };
        match result.and_then(|v| WorkflowHistory::parse(&v, name)) {
            Ok(history) => query.finish(history),
            Err(error) => query.fail(&error),
        }
    }
    pub(crate) fn workflow_history_parent(
        &self,
        key: &str,
        scope: &str,
        name: &str,
        id: &str,
    ) -> Option<String> {
        let ws = self.ws(key)?;
        let run = ws
            .workflow_histories
            .get(&format!("{scope}\0{name}"))?
            .value
            .as_ref()?
            .runs
            .iter()
            .find(|r| r.run_id == id)?;
        if run.tool_call_id.as_deref()?.trim().is_empty() {
            return None;
        }
        let sid = run.parent_session_id.as_deref()?.trim();
        if sid.is_empty()
            || run.cwd.as_ref().map_or(scope != "project", |cwd| {
                std::path::Path::new(cwd) != ws.path
            })
        {
            return None;
        }
        Some(sid.into())
    }
    pub(crate) fn open_workflow_history_chat(
        &mut self,
        key: &str,
        scope: &str,
        name: &str,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.active_ws_key().as_deref() != Some(key)
            || self.is_read_only_view()
            || !self.ws(key).is_some_and(|w| {
                w.saved_workflow_form.scope == scope
                    && w.saved_workflow_form.selected.as_deref() == Some(name)
            })
        {
            return;
        }
        if let Some(sid) = self.workflow_history_parent(key, scope, name, id) {
            self.select_session(key, &sid, cx);
        }
    }
}
