use crate::app::store::AppState;
use crate::composer::input::Composer;
use gpui::{AppContext, Context, Entity};
use std::collections::BTreeMap;

pub(crate) struct SavedWorkflowForm {
    pub scope: String,
    pub selected: Option<String>,
    pub inputs: BTreeMap<String, Entity<Composer>>,
    pub error: Option<String>,
    pub generation: String,
    pub baseline: Option<crate::shared::saved_workflows::SavedWorkflowEntry>,
}
impl Default for SavedWorkflowForm {
    fn default() -> Self {
        Self {
            scope: "project".into(),
            selected: None,
            inputs: BTreeMap::new(),
            error: None,
            generation: uuid::Uuid::now_v7().to_string(),
            baseline: None,
        }
    }
}
impl AppState {
    pub(crate) fn select_saved_workflow(
        &mut self,
        key: &str,
        scope: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        if self.active_ws_key().as_deref() != Some(key)
            || self.saved_workflow_launch_pending(key)
            || self.workflow_management_pending(key)
        {
            return;
        }
        let entry = self
            .ws(key)
            .and_then(|ws| ws.saved_workflows.get(scope))
            .and_then(|q| q.value.as_ref())
            .and_then(|list| list.workflows.iter().find(|w| w.name == name))
            .cloned();
        let Some(entry) = entry else { return };
        if self
            .ws(key)
            .is_some_and(|ws| ws.saved_workflow_form.baseline.as_ref() == Some(&entry))
        {
            return;
        }
        let inputs = entry
            .args
            .keys()
            .map(|key| {
                (
                    key.clone(),
                    cx.new(|cx| {
                        Composer::new_single_line("Argument value (blank uses default)", cx)
                    }),
                )
            })
            .collect();
        self.ws_mut(key).unwrap().saved_workflow_form = SavedWorkflowForm {
            scope: scope.into(),
            selected: Some(name.into()),
            inputs,
            error: None,
            generation: uuid::Uuid::now_v7().to_string(),
            baseline: Some(entry),
        };
        cx.notify();
    }
}
