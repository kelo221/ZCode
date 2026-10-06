use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use crate::shared::workflow_definition::WorkflowDefinition;
use gpui::Context;
use serde_json::{Value, json};

impl AppState {
    pub(crate) fn inspect_workflow_definition(
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
        if !ws.workflow_definitions.contains_key(&cache_key) && ws.workflow_definitions.len() >= 32
        {
            let removable = ws
                .workflow_definitions
                .iter()
                .find(|(_, q)| !q.loading)
                .map(|(k, _)| k.clone());
            let Some(removable) = removable else { return };
            ws.workflow_definitions.remove(&removable);
        }
        let query = ws
            .workflow_definitions
            .entry(cache_key.clone())
            .or_default();
        if query.loading || query.attempted && !refresh {
            return;
        }
        query.start();
        let id = ws.next_id();
        ws.pending.insert(
            id,
            Pending::WorkflowDefinition {
                scope: scope.into(),
                name: name.into(),
            },
        );
        if !ws.send_pending_line(id, json!({"id":id,"method":"workflows/get","params":{"workspace":{"workspacePath":ws.path,"workspaceKey":ws.key},"scope":scope,"name":name}}).to_string()) {
            ws.workflow_definitions.get_mut(&cache_key).unwrap().fail("Workflow definition could not be requested");
        }
        cx.notify();
    }
    pub(crate) fn settle_workflow_definition(
        &mut self,
        key: &str,
        scope: &str,
        name: &str,
        result: Result<Value, String>,
    ) {
        let Some(query) = self
            .ws_mut(key)
            .and_then(|ws| ws.workflow_definitions.get_mut(&format!("{scope}\0{name}")))
        else {
            return;
        };
        match result.and_then(|v| WorkflowDefinition::parse(&v, scope, name)) {
            Ok(definition) => {
                query.finish(definition.clone());
                if let Some(form) = self
                    .ws_mut(key)
                    .and_then(|w| w.workflow_management.as_mut())
                    && form.reload_requested
                    && form.baseline.scope == scope
                    && form.baseline.name == name
                {
                    // Reload 明确采用新基线，但输入仍属于用户，不能抹掉刷新期间的新编辑。
                    form.baseline = definition;
                    form.token = uuid::Uuid::now_v7().to_string();
                    form.reload_requested = false;
                    form.needs_reload = false;
                    form.error = None;
                }
            }
            Err(error) => query.fail(&error),
        }
    }
}
