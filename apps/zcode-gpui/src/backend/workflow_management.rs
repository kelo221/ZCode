use crate::app::store::AppState;
use crate::composer::input::Composer;
use crate::shared::workflow_definition::WorkflowDefinition;
use gpui::{AppContext, Context, Entity};
use serde_json::{Value, json};

#[derive(Clone, PartialEq)]
pub(crate) enum WorkflowMutation {
    Metadata,
    Delete,
    Move,
}
impl WorkflowMutation {
    pub(crate) fn method(&self) -> &'static str {
        match self {
            Self::Metadata => "workflows/updateMeta",
            Self::Delete => "workflows/delete",
            Self::Move => "workflows/move",
        }
    }
}
pub(crate) struct WorkflowManagementDraft {
    pub token: String,
    pub baseline: WorkflowDefinition,
    pub description: Entity<Composer>,
    pub when_to_use: Entity<Composer>,
    pub args: Entity<Composer>,
    pub confirmation: Option<WorkflowMutation>,
    pub error: Option<String>,
    pub needs_reload: bool,
    pub reload_requested: bool,
}
impl WorkflowManagementDraft {
    pub(crate) fn new(baseline: WorkflowDefinition, cx: &mut Context<AppState>) -> Self {
        let description = cx.new(|cx| Composer::new_single_line("Description", cx));
        description.update(cx, |c, _| c.set_text(&baseline.meta.description));
        let when_to_use = cx.new(|cx| Composer::new_single_line("When to use (optional)", cx));
        when_to_use.update(cx, |c, _| {
            c.set_text(baseline.meta.when_to_use.as_deref().unwrap_or(""))
        });
        let args = cx.new(Composer::new);
        args.update(cx, |c, _| {
            c.set_text(&serde_json::to_string(&baseline.meta.args).unwrap_or_default())
        });
        cx.observe(&description, |_, _, cx| cx.notify()).detach();
        cx.observe(&when_to_use, |_, _, cx| cx.notify()).detach();
        cx.observe(&args, |_, _, cx| cx.notify()).detach();
        Self {
            token: uuid::Uuid::now_v7().to_string(),
            baseline,
            description,
            when_to_use,
            args,
            confirmation: None,
            error: None,
            needs_reload: false,
            reload_requested: false,
        }
    }
    pub(crate) fn metadata(&self, cx: &gpui::App) -> Result<Value, String> {
        let description = self.description.read(cx).text().trim();
        if description.is_empty() {
            return Err("Description is required".into());
        }
        let when = self.when_to_use.read(cx).text().trim();
        let text = self.args.read(cx).text().trim();
        let args: Value = if text.is_empty() {
            json!({})
        } else {
            serde_json::from_str(text).map_err(|_| "Argument declarations must be a JSON object")?
        };
        let mut meta = json!({"description":description,"args":args});
        if !when.is_empty() {
            meta["whenToUse"] = json!(when);
        }
        let mut definition =
            serde_json::to_value(&self.baseline).map_err(|_| "Invalid definition")?;
        definition["meta"] = meta.clone();
        WorkflowDefinition::parse(&definition, &self.baseline.scope, &self.baseline.name)?;
        Ok(meta)
    }
    pub(crate) fn dirty(&self, cx: &gpui::App) -> bool {
        self.metadata(cx).is_ok_and(|m| {
            serde_json::from_value::<crate::shared::workflow_definition::WorkflowMetadata>(m)
                .is_ok_and(|m| m != self.baseline.meta)
        })
    }
}
#[derive(Clone)]
pub(crate) struct WorkflowMutationReceipt {
    pub token: String,
    pub baseline: WorkflowDefinition,
    pub action: WorkflowMutation,
    pub meta: Option<Value>,
    pub selection_generation: String,
}
impl AppState {
    pub(crate) fn workflow_management_matches(&self, key: &str, token: &str) -> bool {
        self.active_ws_key().as_deref() == Some(key)
            && !self.is_read_only_view()
            && self.ws(key).is_some_and(|w| {
                w.workflow_management.as_ref().is_some_and(|f| {
                    f.token == token
                        && w.saved_workflow_form.scope == f.baseline.scope
                        && w.saved_workflow_form.selected.as_deref() == Some(&f.baseline.name)
                })
            })
    }
    pub(crate) fn workflow_management_pending(&self, key: &str) -> bool {
        self.ws(key).is_some_and(|w| {
            w.pending.values().any(|p| {
                matches!(
                    p,
                    crate::backend::workspace::Pending::WorkflowPreflight(_)
                        | crate::backend::workspace::Pending::WorkflowMutation(_)
                )
            })
        })
    }
    pub(crate) fn open_workflow_management(
        &mut self,
        key: &str,
        scope: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        if self.active_ws_key().as_deref() != Some(key)
            || self.is_read_only_view()
            || self.workflow_management_pending(key)
            || self.saved_workflow_launch_pending(key)
        {
            return;
        }
        let Some(ws) = self.ws(key) else { return };
        if ws.saved_workflow_form.scope != scope
            || ws.saved_workflow_form.selected.as_deref() != Some(name)
        {
            return;
        }
        if ws
            .workflow_management
            .as_ref()
            .is_some_and(|f| f.baseline.name == name && f.baseline.scope == scope)
        {
            return;
        }
        let Some(definition) = ws
            .workflow_definitions
            .get(&format!("{scope}\0{name}"))
            .filter(|q| !q.loading && q.error.is_none())
            .and_then(|q| q.value.clone())
        else {
            return;
        };
        self.ws_mut(key).unwrap().workflow_management =
            Some(WorkflowManagementDraft::new(definition, cx));
        cx.notify();
    }
    pub(crate) fn request_workflow_confirmation(
        &mut self,
        key: &str,
        action: WorkflowMutation,
        cx: &mut Context<Self>,
    ) {
        if self.active_ws_key().as_deref() != Some(key)
            || self.is_read_only_view()
            || self.workflow_management_pending(key)
            || self.saved_workflow_launch_pending(key)
            || action == WorkflowMutation::Metadata
        {
            return;
        }
        if let Some(ws) = self.ws_mut(key) {
            let project = ws.purpose == crate::backend::workspace::WorkspacePurpose::Project;
            if let Some(form) = &mut ws.workflow_management
                && !form.needs_reload
                && (action != WorkflowMutation::Move || form.baseline.scope == "global" && project)
            {
                form.confirmation = Some(action);
            }
        }
        cx.notify();
    }
    pub(crate) fn cancel_workflow_confirmation(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(f) = self
            .ws_mut(key)
            .and_then(|w| w.workflow_management.as_mut())
        {
            f.confirmation = None;
        }
        cx.notify();
    }
    pub(crate) fn reload_workflow_management(
        &mut self,
        key: &str,
        scope: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        if self.active_ws_key().as_deref() != Some(key)
            || self.is_read_only_view()
            || self.workflow_management_pending(key)
            || self.saved_workflow_launch_pending(key)
        {
            return;
        }
        let Some(ws) = self.ws_mut(key) else { return };
        if ws.saved_workflow_form.scope != scope
            || ws.saved_workflow_form.selected.as_deref() != Some(name)
        {
            return;
        }
        if let Some(form) = ws
            .workflow_management
            .as_mut()
            .filter(|f| f.baseline.scope == scope && f.baseline.name == name)
        {
            form.reload_requested = true;
            form.needs_reload = true;
            form.confirmation = None;
        }
        self.inspect_workflow_definition(key, scope, name, true, cx);
    }
    pub(crate) fn workflow_management_error(
        &mut self,
        key: &str,
        receipt: &WorkflowMutationReceipt,
        error: &str,
    ) {
        if let Some(form) = self
            .ws_mut(key)
            .and_then(|w| w.workflow_management.as_mut())
            .filter(|f| f.token == receipt.token)
        {
            form.error = Some(crate::shared::redact::scrub(error));
        }
    }
}
