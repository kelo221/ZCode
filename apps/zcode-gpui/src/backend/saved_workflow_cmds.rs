use crate::app::store::AppState;
use crate::backend::{workflow_cmds::start_saved_workflow_payload, workspace::Pending};
use crate::shared::saved_workflows::{SavedWorkflowList, parse_args};
use gpui::Context;
use serde_json::{Value, json};

#[derive(Clone)]
pub(crate) struct SavedWorkflowLaunch {
    pub generation: u64,
    pub name: String,
    pub scope: String,
    pub args: Value,
    pub form_generation: String,
}

impl AppState {
    pub(crate) fn fetch_saved_workflows(
        &mut self,
        scope: &str,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(key) = self.active_ws_key() {
            self.fetch_saved_workflows_for(&key, scope, refresh, cx);
        }
    }

    pub(crate) fn fetch_saved_workflows_for(
        &mut self,
        key: &str,
        scope: &str,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        if !matches!(scope, "project" | "global") {
            return;
        }
        let Some(ws) = self.ws_mut(key) else { return };
        let query = ws.saved_workflows.entry(scope.into()).or_default();
        if !ws.started || ws.inbound.is_none() || query.loading || query.attempted && !refresh {
            return;
        }
        query.start();
        let params =
            json!({"workspace":{"workspacePath":ws.path,"workspaceKey":ws.key},"scope":scope});
        let id = ws.next_id();
        ws.pending
            .insert(id, Pending::SavedWorkflowList(scope.into()));
        if !ws.send_pending_line(
            id,
            json!({"id":id,"method":"workflows/list","params":params}).to_string(),
        ) {
            ws.saved_workflows
                .get_mut(scope)
                .unwrap()
                .fail("Saved workflow query could not be sent");
        }
        cx.notify();
    }

    pub(crate) fn settle_saved_workflow_list(
        &mut self,
        key: &str,
        scope: &str,
        result: Result<Value, String>,
    ) {
        let Some(ws) = self.ws_mut(key) else { return };
        let query = ws.saved_workflows.entry(scope.into()).or_default();
        match result.and_then(|v| SavedWorkflowList::parse(&v, scope)) {
            Ok(list) => query.finish(list),
            Err(error) => query.fail(&error),
        }
    }

    pub(crate) fn saved_workflow_launch_pending(&self, key: &str) -> bool {
        self.ws(key).is_some_and(|ws| {
            ws.pending.values().any(|p| {
                matches!(
                    p,
                    Pending::SavedWorkflowCreate(_) | Pending::SavedWorkflowStart { .. }
                )
            })
        })
    }

    pub(crate) fn launch_saved_workflow(
        &mut self,
        key: &str,
        scope: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        if self.active_ws_key().as_deref() != Some(key)
            || self.is_read_only_view()
            || self.saved_workflow_launch_pending(key)
            || self.workflow_management_pending(key)
        {
            return;
        }
        let Some(ws) = self.ws(key) else { return };
        if !ws.started
            || ws.inbound.is_none()
            || ws.saved_workflow_form.scope != scope
            || ws.saved_workflow_form.selected.as_deref() != Some(name)
        {
            return;
        }
        let Some(entry) = ws
            .saved_workflows
            .get(scope)
            .and_then(|q| q.value.as_ref())
            .and_then(|list| list.workflows.iter().find(|w| w.name == name))
        else {
            return;
        };
        if ws.saved_workflow_form.baseline.as_ref() != Some(entry) {
            self.saved_workflow_error(
                key,
                "Workflow definition changed; re-select before launching",
            );
            cx.notify();
            return;
        }
        let inputs = ws
            .saved_workflow_form
            .inputs
            .iter()
            .map(|(key, input)| (key.clone(), input.read(cx).text().to_owned()))
            .collect();
        let args = match parse_args(entry, &inputs) {
            Ok(args) => args,
            Err(error) => {
                self.saved_workflow_error(key, &error);
                cx.notify();
                return;
            }
        };
        let launch = SavedWorkflowLaunch {
            generation: self.navigation_generation,
            name: name.into(),
            scope: scope.into(),
            args,
            form_generation: ws.saved_workflow_form.generation.clone(),
        };
        self.ws_mut(key).unwrap().saved_workflow_form.error = None;
        if !self.send_command(
            key,
            None,
            "createSession",
            json!({"workspaceId":key}),
            None,
            Pending::SavedWorkflowCreate(launch),
        ) {
            self.saved_workflow_error(key, "Workflow target session could not be created");
        }
        cx.notify();
    }

    pub(crate) fn settle_saved_workflow_create(
        &mut self,
        key: &str,
        launch: SavedWorkflowLaunch,
        ack: Option<&Value>,
        cx: &mut Context<Self>,
    ) {
        let sid = ack
            .filter(|a| {
                a["status"] == "accepted"
                    && a.pointer("/result/type").and_then(Value::as_str) == Some("createSession")
            })
            .and_then(|a| a.pointer("/result/sessionId"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty());
        let Some(sid) = sid else {
            self.saved_workflow_launch_error(key, &launch, "Workflow target creation was rejected or is unknown; check the task list before retrying");
            return;
        };
        let args =
            (!launch.args.as_object().is_some_and(|o| o.is_empty())).then(|| launch.args.clone());
        let payload = start_saved_workflow_payload(&launch.name, Some(&launch.scope), args);
        // 已创建的目标才可接收 start；不能借用当前 chat，也不能用 create ACK 提前导航。
        if !self.send_command(
            key,
            Some(sid.into()),
            "startSavedWorkflow",
            payload,
            None,
            Pending::SavedWorkflowStart {
                launch: launch.clone(),
                session: sid.into(),
            },
        ) {
            self.cleanup_workflow_target(key, sid);
            self.saved_workflow_launch_error(
                key,
                &launch,
                "Workflow start could not be sent; check tasks if target cleanup also failed",
            );
        }
        cx.notify();
    }

    pub(crate) fn settle_saved_workflow_start(
        &mut self,
        key: &str,
        launch: SavedWorkflowLaunch,
        sid: &str,
        ack: Option<&Value>,
        cx: &mut Context<Self>,
    ) {
        let valid = ack.is_some_and(|a| {
            a["status"] == "accepted"
                && a.pointer("/result/type").and_then(Value::as_str) == Some("startSavedWorkflow")
                && ["/result/runId", "/result/toolCallId"].iter().all(|p| {
                    a.pointer(p)
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.trim().is_empty())
                })
        });
        if valid {
            if self.active_ws_key().as_deref() == Some(key)
                && self.navigation_generation == launch.generation
                && !self.is_read_only_view()
            {
                self.select_session(key, sid, cx);
            }
            self.push_log("Saved workflow started".into());
        } else {
            let explicit = ack
                .and_then(|a| a["status"].as_str())
                .is_some_and(|s| matches!(s, "failed" | "rejected" | "stale"));
            if explicit {
                self.cleanup_workflow_target(key, sid);
            }
            let message = ack
                .and_then(|a| a["message"].as_str())
                .unwrap_or(if explicit {
                    "Saved workflow start was rejected"
                } else {
                    "Workflow start outcome is unknown; check tasks before retrying"
                });
            self.saved_workflow_launch_error(key, &launch, message);
        }
        cx.notify();
    }

    pub(crate) fn cleanup_workflow_target(&mut self, key: &str, sid: &str) {
        if !self.send_command(
            key,
            Some(sid.into()),
            "deleteSession",
            json!({}),
            None,
            Pending::SavedWorkflowCleanup,
        ) {
            self.status_error(
                key,
                "Workflow target cleanup could not be sent; check the task list",
            );
            self.push_error(
                "Workflow target cleanup could not be sent; check the task list".into(),
            );
        }
    }
    pub(crate) fn saved_workflow_launch_error(
        &mut self,
        key: &str,
        launch: &SavedWorkflowLaunch,
        error: &str,
    ) {
        if let Some(ws) = self.ws_mut(key)
            && ws.saved_workflow_form.generation == launch.form_generation
        {
            ws.saved_workflow_form.error = Some(crate::shared::redact::scrub(error));
        }
        self.status_error(key, error);
    }
    pub(crate) fn saved_workflow_error(&mut self, key: &str, error: &str) {
        if let Some(ws) = self.ws_mut(key) {
            ws.saved_workflow_form.error = Some(crate::shared::redact::scrub(error));
        }
        self.status_error(key, error);
    }
}
