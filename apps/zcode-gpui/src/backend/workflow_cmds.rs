//! V4 Workflow commands: startSavedWorkflow, resumeWorkflowRun, amendWorkflowRunSettings.
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/command.ts
//! and packages/shared/src/zcode-protocol-v4/workflow-run-settings-command.ts.

use crate::app::store::AppState;
use crate::backend::workspace::{CommandCtx, Pending};
use gpui::Context;
use serde_json::{Value, json};

#[allow(dead_code)]
pub fn start_saved_workflow_payload(name: &str, scope: Option<&str>, args: Option<Value>) -> Value {
    let mut payload = json!({ "name": name });
    if let Some(scope) = scope {
        payload["scope"] = json!(scope);
    }
    if let Some(args) = args {
        payload["args"] = args;
    }
    payload
}

pub fn resume_workflow_run_payload(work_id: &str, name: Option<&str>) -> Value {
    let mut payload = json!({ "workId": work_id });
    if let Some(name) = name {
        payload["name"] = json!(name);
    }
    payload
}

#[allow(dead_code)]
pub fn amend_workflow_run_settings_payload(
    work_id: &str,
    subagent_model: Option<Option<String>>,
    max_concurrency: Option<Option<u64>>,
) -> Value {
    let mut payload = json!({ "workId": work_id });
    if let Some(model) = subagent_model {
        payload["subagentModel"] = match model {
            Some(m) => json!(m),
            None => Value::Null,
        };
    }
    if let Some(conc) = max_concurrency {
        payload["maxConcurrency"] = match conc {
            Some(c) => json!(c),
            None => Value::Null,
        };
    }
    payload
}

impl AppState {
    #[allow(dead_code)]
    pub fn start_saved_workflow(
        &mut self,
        name: &str,
        scope: Option<&str>,
        args: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        let Some(ws_key) = self.active_ws_key() else {
            return;
        };
        let sid = self.active.clone();
        let payload = start_saved_workflow_payload(name, scope, args);
        let ctx = CommandCtx::new(
            sid.as_deref().unwrap_or(""),
            "startSavedWorkflow",
            payload.clone(),
        );
        self.send_command(
            &ws_key,
            sid,
            "startSavedWorkflow",
            payload,
            None,
            Pending::Command(ctx),
        );
        self.push_log(format!("starting saved workflow {name}…"));
        cx.notify();
    }

    pub fn resume_workflow_run(
        &mut self,
        run_id: &str,
        name: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return;
        };
        let payload = resume_workflow_run_payload(run_id, name);
        let ctx = CommandCtx::new(&sid, "resumeWorkflowRun", payload.clone());
        self.send_command(
            &ws_key,
            Some(sid),
            "resumeWorkflowRun",
            payload,
            None,
            Pending::Command(ctx),
        );
        self.push_log(format!("resuming workflow run {run_id}…"));
        cx.notify();
    }

    #[allow(dead_code)]
    pub fn amend_workflow_run_settings(
        &mut self,
        run_id: &str,
        subagent_model: Option<Option<String>>,
        max_concurrency: Option<Option<u64>>,
        cx: &mut Context<Self>,
    ) {
        let (Some(sid), Some(ws_key)) = (self.active.clone(), self.active_ws_key()) else {
            return;
        };
        let payload = amend_workflow_run_settings_payload(run_id, subagent_model, max_concurrency);
        let ctx = CommandCtx::new(&sid, "amendWorkflowRunSettings", payload.clone());
        self.send_command(
            &ws_key,
            Some(sid),
            "amendWorkflowRunSettings",
            payload,
            None,
            Pending::Command(ctx),
        );
        self.push_log(format!("amending workflow run {run_id} settings…"));
        cx.notify();
    }
}

#[cfg(test)]
#[path = "workflow_cmds_tests.rs"]
mod tests;
