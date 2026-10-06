//! V4 Workflow commands: startSavedWorkflow, resumeWorkflowRun, amendWorkflowRunSettings.
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/command.ts
//! and packages/shared/src/zcode-protocol-v4/workflow-run-settings-command.ts.

use crate::app::store::AppState;
use crate::backend::workspace::{CommandCtx, Pending};
use gpui::Context;
use serde_json::{Value, json};

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
    pub(crate) fn workflow_resume_pending(&self, workspace: &str, sid: &str, run_id: &str) -> bool {
        self.ws(workspace).is_some_and(|ws| ws.pending.values().any(|p| matches!(p, Pending::Command(ctx) if ctx.sid == sid && ctx.ctype == "resumeWorkflowRun" && ctx.payload["workId"].as_str() == Some(run_id))))
    }

    pub(crate) fn resume_workflow_run_for(
        &mut self,
        workspace: &str,
        sid: &str,
        run_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.active_ws_key().as_deref() != Some(workspace)
            || self.active.as_deref() != Some(sid)
            || self.is_read_only_view()
            || self.workflow_resume_pending(workspace, sid, run_id)
        {
            return;
        }
        // stopped 不是恢复许可；显式 resumable=false 或被替代的 run 必须保持关闭。
        let allowed = self.conversations.get(sid).is_some_and(|c| {
            c.workflow_runs.runs.iter().any(|r| {
                r.run_id == run_id
                    && r.status == "stopped"
                    && r.resumable == Some(true)
                    && r.superseded_by.is_none()
            })
        });
        if !allowed {
            return;
        }
        self.send_session_command(
            workspace,
            CommandCtx::new(
                sid,
                "resumeWorkflowRun",
                resume_workflow_run_payload(run_id, None),
            ),
        );
        cx.notify();
    }
}

#[cfg(test)]
#[path = "workflow_cmds_tests.rs"]
mod tests;
