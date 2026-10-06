use crate::app::store::AppState;
use crate::backend::{
    workflow_settings::{WorkflowSettingValues, configurable},
    workspace::WorkspaceHandle,
};

impl AppState {
    pub(crate) fn reconcile_workflow_settings(&mut self, key: &str, sid: &str, snapshot: bool) {
        self.reconcile_workflow_follow();
        let values: std::collections::HashMap<_, _> = self
            .conversations
            .get(sid)
            .map(|c| {
                c.workflow_runs
                    .runs
                    .iter()
                    .filter(|r| configurable(r))
                    .map(|r| (r.run_id.clone(), WorkflowSettingValues::from_run(r)))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(ws) = self.ws_mut(key) {
            let prefix = format!("{sid}\0");
            ws.workflow_settings.retain(|cache_key, form| {
                let Some(id) = cache_key.strip_prefix(&prefix) else {
                    return true;
                };
                let Some(current) = values.get(id) else {
                    return false;
                };
                if snapshot {
                    form.needs_snapshot = false;
                }
                if form.awaiting.as_ref().is_some_and(|sent| sent == current) {
                    // 新编辑仍归输入 entity 所有；投影确认只更新比较基线，不能替换输入。
                    form.baseline = current.clone();
                    form.awaiting = None;
                    form.error = None;
                }
                true
            });
        }
    }
}
impl WorkspaceHandle {
    pub(crate) fn interrupt_workflow_settings(&mut self) {
        for form in self.workflow_settings.values_mut() {
            form.needs_snapshot = true;
            if form.awaiting.is_some() {
                form.error =
                    Some("Connection interrupted; workflow settings outcome is unknown".into());
            }
        }
    }
}
