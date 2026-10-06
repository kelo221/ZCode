use crate::app::store::AppState;
use crate::backend::workflow_management::{WorkflowMutation, WorkflowMutationReceipt};
use crate::backend::workspace::{Pending, WorkspacePurpose};
use crate::shared::workflow_definition::WorkflowDefinition;
use gpui::Context;
use serde_json::{Value, json};

impl AppState {
    pub(crate) fn submit_workflow_management(
        &mut self,
        key: &str,
        action: WorkflowMutation,
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
        let Some(form) = &ws.workflow_management else {
            return;
        };
        if !ws.started
            || ws.inbound.is_none()
            || form.needs_reload
            || ws.saved_workflow_form.scope != form.baseline.scope
            || ws.saved_workflow_form.selected.as_deref() != Some(&form.baseline.name)
            || !ws
                .saved_workflows
                .get(&form.baseline.scope)
                .and_then(|q| q.value.as_ref())
                .is_some_and(|l| l.workflows.iter().any(|e| e.name == form.baseline.name))
            || action != WorkflowMutation::Metadata && form.confirmation.as_ref() != Some(&action)
            || action == WorkflowMutation::Move
                && (form.baseline.scope != "global" || ws.purpose != WorkspacePurpose::Project)
        {
            return;
        }
        let meta = if action == WorkflowMutation::Metadata {
            match form.metadata(cx) {
                Ok(meta) if form.dirty(cx) => Some(meta),
                Ok(_) => return,
                Err(error) => {
                    self.ws_mut(key)
                        .unwrap()
                        .workflow_management
                        .as_mut()
                        .unwrap()
                        .error = Some(error);
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        let receipt = WorkflowMutationReceipt {
            token: form.token.clone(),
            baseline: form.baseline.clone(),
            action,
            meta,
            selection_generation: ws.saved_workflow_form.generation.clone(),
        };
        let ws = self.ws_mut(key).unwrap();
        ws.workflow_management.as_mut().unwrap().error = None;
        ws.workflow_management.as_mut().unwrap().confirmation = None;
        let id = ws.next_id();
        ws.pending
            .insert(id, Pending::WorkflowPreflight(receipt.clone()));
        if !ws.send_pending_line(id, json!({"id":id,"method":"workflows/get","params":{"workspace":{"workspacePath":ws.path,"workspaceKey":ws.key},"scope":receipt.baseline.scope,"name":receipt.baseline.name}}).to_string()) {
            self.workflow_management_error(key, &receipt, "Workflow preflight could not be sent");
        }
        cx.notify();
    }

    pub(crate) fn settle_workflow_preflight(
        &mut self,
        key: &str,
        receipt: WorkflowMutationReceipt,
        result: Result<Value, String>,
        cx: &mut Context<Self>,
    ) {
        let current = match result.and_then(|v| {
            WorkflowDefinition::parse(&v, &receipt.baseline.scope, &receipt.baseline.name)
        }) {
            Ok(current) => current,
            Err(error) => {
                self.workflow_management_error(key, &receipt, &error);
                return;
            }
        };
        if !self
            .ws(key)
            .and_then(|w| w.workflow_management.as_ref())
            .is_some_and(|f| f.token == receipt.token && !f.needs_reload)
        {
            return;
        }
        if current != receipt.baseline {
            // 文件由 CLI 所有；前置读发现外部修改时不能继续覆盖，也不能隐式重试。
            self.ws_mut(key)
                .unwrap()
                .workflow_management
                .as_mut()
                .unwrap()
                .needs_reload = true;
            self.workflow_management_error(
                key,
                &receipt,
                "Workflow definition changed; Reload before another write",
            );
            return;
        }
        let Some(ws) = self
            .ws_mut(key)
            .filter(|w| w.started && w.inbound.is_some())
        else {
            return;
        };
        let mut params = json!({"workspace":{"workspacePath":ws.path,"workspaceKey":ws.key},"name":receipt.baseline.name});
        if receipt.action != WorkflowMutation::Move {
            params["scope"] = json!(receipt.baseline.scope);
        }
        if let Some(meta) = &receipt.meta {
            params["meta"] = meta.clone();
        }
        let id = ws.next_id();
        ws.pending
            .insert(id, Pending::WorkflowMutation(receipt.clone()));
        if !ws.send_pending_line(
            id,
            json!({"id":id,"method":receipt.action.method(),"params":params}).to_string(),
        ) {
            self.workflow_management_error(key, &receipt, "Workflow mutation could not be sent");
        }
        cx.notify();
    }

    pub(crate) fn settle_workflow_mutation(
        &mut self,
        key: &str,
        receipt: WorkflowMutationReceipt,
        result: Result<Value, String>,
        cx: &mut Context<Self>,
    ) {
        let outcome = result.and_then(|v| {
            crate::shared::workflow_mutation::validate(
                &v,
                receipt.action == WorkflowMutation::Move,
                &receipt.baseline.path,
            )
        });
        if let Err(error) = outcome {
            // 写入后的 RPC 错误不能证明文件未改变；先重新读取，禁止自动重复写入。
            if let Some(form) = self
                .ws_mut(key)
                .and_then(|w| w.workflow_management.as_mut())
                .filter(|f| f.token == receipt.token)
            {
                form.needs_reload = true;
            }
            self.workflow_management_error(
                key,
                &receipt,
                &format!("{error}; Reload before another write"),
            );
            return;
        }
        let Some(ws) = self.ws_mut(key) else { return };
        if receipt.action == WorkflowMutation::Metadata {
            if let Some(form) = ws
                .workflow_management
                .as_mut()
                .filter(|f| f.token == receipt.token)
            {
                form.baseline.meta = serde_json::from_value(receipt.meta.clone().unwrap())
                    .expect("validated metadata");
                form.error = None;
            }
        } else {
            if ws
                .workflow_management
                .as_ref()
                .is_some_and(|f| f.token == receipt.token)
            {
                ws.workflow_management = None;
            }
            if ws.saved_workflow_form.generation == receipt.selection_generation
                && ws.saved_workflow_form.scope == receipt.baseline.scope
                && ws.saved_workflow_form.selected.as_deref() == Some(&receipt.baseline.name)
            {
                ws.saved_workflow_form = crate::app::saved_workflow_form::SavedWorkflowForm {
                    scope: receipt.baseline.scope.clone(),
                    ..Default::default()
                };
            }
        }
        let cache_key = format!("{}\0{}", receipt.baseline.scope, receipt.baseline.name);
        ws.workflow_definitions.remove(&cache_key);
        ws.workflow_histories.remove(&cache_key);
        if receipt.action == WorkflowMutation::Move {
            let destination_key = format!("project\0{}", receipt.baseline.name);
            ws.workflow_definitions.remove(&destination_key);
            ws.workflow_histories.remove(&destination_key);
        }
        let scopes: &[&str] = if receipt.action == WorkflowMutation::Move {
            &["global", "project"]
        } else {
            &[&receipt.baseline.scope]
        };
        ws.pending.retain(|_, p| match p {
            Pending::SavedWorkflowList(scope) => !scopes.contains(&scope.as_str()),
            Pending::WorkflowDefinition { scope, name }
            | Pending::WorkflowHistory { scope, name } => {
                !scopes.contains(&scope.as_str()) || name != &receipt.baseline.name
            }
            _ => true,
        });
        for scope in scopes {
            ws.saved_workflows.remove(*scope);
        }
        for scope in scopes {
            self.fetch_saved_workflows_for(key, scope, true, cx);
        }
        cx.notify();
    }
}
