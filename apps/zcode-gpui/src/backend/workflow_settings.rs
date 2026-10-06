use crate::app::store::AppState;
use crate::backend::{workflow_cmds::amend_workflow_run_settings_payload, workspace::Pending};
use crate::composer::input::Composer;
use crate::conversation::workflows_types::WorkflowRunState;
use gpui::{App, AppContext, Context, Entity};
use serde_json::Value;

#[derive(Clone, PartialEq)]
pub(crate) struct WorkflowSettingValues {
    pub model: Option<String>,
    pub limit: Option<u64>,
    pub ceiling: Option<u64>,
}
impl WorkflowSettingValues {
    pub(crate) fn from_run(run: &WorkflowRunState) -> Self {
        let ceiling = run
            .concurrency_ceiling
            .or_else(|| run.concurrency.as_ref().map(|c| c.ceiling));
        Self {
            model: run.subagent_model.clone(),
            limit: run
                .concurrency
                .as_ref()
                .and_then(|c| c.limit)
                .filter(|v| ceiling.is_none_or(|c| *v < c)),
            ceiling,
        }
    }
}
pub(crate) struct WorkflowSettingsDraft {
    pub model: Entity<Composer>,
    pub concurrency: Entity<Composer>,
    pub baseline: WorkflowSettingValues,
    pub awaiting: Option<WorkflowSettingValues>,
    pub needs_snapshot: bool,
    pub error: Option<String>,
}
pub(crate) fn configurable(run: &WorkflowRunState) -> bool {
    matches!(
        run.status.as_str(),
        "pending" | "running" | "errored" | "stopped"
    ) && run.superseded_by.is_none()
        && run.stop_reason.as_deref() != Some("superseded")
}
impl AppState {
    pub(crate) fn workflow_settings_allowed(&self, key: &str, sid: &str, id: &str) -> bool {
        self.active_ws_key().as_deref() == Some(key)
            && self.active.as_deref() == Some(sid)
            && !self.is_read_only_view()
            && self.conversations.get(sid).is_some_and(|c| {
                c.workflow_runs
                    .runs
                    .iter()
                    .any(|r| r.run_id == id && configurable(r))
            })
    }
    pub(crate) fn workflow_settings_pending(&self, key: &str, sid: &str, id: &str) -> bool {
        self.ws(key).is_some_and(|ws| ws.pending.values().any(|p| matches!(p, Pending::WorkflowSettings { session, run } if session == sid && run == id)))
    }
    pub(crate) fn open_workflow_settings(
        &mut self,
        key: &str,
        sid: &str,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.workflow_settings_allowed(key, sid, id) {
            return;
        }
        let cache_key = format!("{sid}\0{id}");
        let Some(ws) = self.ws(key) else { return };
        if ws.workflow_settings.contains_key(&cache_key) {
            return;
        }
        if ws.workflow_settings.len() >= 32 {
            let removable = ws.workflow_settings.keys().find(|k| !ws.pending.values().any(|p| matches!(p, Pending::WorkflowSettings { session, run } if **k == format!("{session}\0{run}")))).cloned();
            let Some(removable) = removable else { return };
            self.ws_mut(key)
                .unwrap()
                .workflow_settings
                .remove(&removable);
        }
        let run = self.conversations[sid]
            .workflow_runs
            .runs
            .iter()
            .find(|r| r.run_id == id)
            .unwrap();
        let baseline = WorkflowSettingValues::from_run(run);
        let model = cx.new(|cx| Composer::new_single_line("Blank uses session model", cx));
        model.update(cx, |c, _| {
            c.set_text(baseline.model.as_deref().unwrap_or(""))
        });
        let concurrency = cx.new(|cx| Composer::new_single_line("Blank removes run limit", cx));
        concurrency.update(cx, |c, _| {
            c.set_text(&baseline.limit.map(|v| v.to_string()).unwrap_or_default())
        });
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        cx.observe(&concurrency, |_, _, cx| cx.notify()).detach();
        self.ws_mut(key).unwrap().workflow_settings.insert(
            cache_key,
            WorkflowSettingsDraft {
                model,
                concurrency,
                baseline,
                awaiting: None,
                needs_snapshot: false,
                error: None,
            },
        );
        cx.notify();
    }
    pub(crate) fn reload_workflow_settings(
        &mut self,
        key: &str,
        sid: &str,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.workflow_settings_allowed(key, sid, id)
            || self.workflow_settings_pending(key, sid, id)
            || self
                .ws(key)
                .and_then(|w| w.workflow_settings.get(&format!("{sid}\0{id}")))
                .is_some_and(|f| f.needs_snapshot)
        {
            return;
        }
        self.ws_mut(key)
            .unwrap()
            .workflow_settings
            .remove(&format!("{sid}\0{id}"));
        self.open_workflow_settings(key, sid, id, cx);
    }
    fn workflow_settings_payload(
        &self,
        key: &str,
        sid: &str,
        id: &str,
        cx: &App,
    ) -> Result<Option<(Value, WorkflowSettingValues)>, String> {
        let ws = self.ws(key).ok_or("Workspace unavailable")?;
        let form = ws
            .workflow_settings
            .get(&format!("{sid}\0{id}"))
            .ok_or("Open settings first")?;
        if form.needs_snapshot {
            return Err("Waiting for a fresh conversation projection".into());
        }
        if form.awaiting.is_some() {
            return Err(
                "Settings outcome is awaiting confirmation; reload before another Apply".into(),
            );
        }
        let run = self
            .conversations
            .get(sid)
            .and_then(|c| c.workflow_runs.runs.iter().find(|r| r.run_id == id))
            .ok_or("Run unavailable")?;
        if WorkflowSettingValues::from_run(run) != form.baseline {
            return Err("Run settings changed; reload before applying".into());
        }
        let model = form.model.read(cx).text().trim();
        let model = (!model.is_empty()).then(|| model.to_owned());
        let text = form.concurrency.read(cx).text().trim();
        let limit = if text.is_empty() {
            None
        } else {
            Some(
                text.parse::<u64>()
                    .ok()
                    .filter(|v| *v > 0)
                    .ok_or("Concurrency must be a positive integer")?,
            )
        }
        .filter(|v| form.baseline.ceiling.is_none_or(|c| *v < c));
        let model_changed = model != form.baseline.model;
        let limit_changed = limit != form.baseline.limit;
        if !model_changed && !limit_changed {
            return Ok(None);
        }
        if model_changed && let Some(model) = &model {
            let valid = self.workspace_configs.get(key).is_some_and(|c| {
                c.models.iter().any(|m| {
                    let base = format!("{}/{}", m.provider, m.model);
                    model == &base
                        || m.thought_levels
                            .iter()
                            .any(|level| model == &format!("{base}${level}"))
                })
            });
            if !valid {
                return Err("Choose an available provider/model[$level]".into());
            }
        }
        Ok(Some((
            amend_workflow_run_settings_payload(
                id,
                model_changed.then_some(model.clone()),
                limit_changed.then_some(limit),
            ),
            WorkflowSettingValues {
                model,
                limit,
                ceiling: form.baseline.ceiling,
            },
        )))
    }
    pub(crate) fn workflow_settings_can_apply(
        &self,
        key: &str,
        sid: &str,
        id: &str,
        cx: &App,
    ) -> bool {
        self.workflow_settings_allowed(key, sid, id)
            && !self.workflow_settings_pending(key, sid, id)
            && self
                .ws(key)
                .is_some_and(|w| w.started && w.inbound.is_some())
            && matches!(
                self.workflow_settings_payload(key, sid, id, cx),
                Ok(Some(_))
            )
    }
    pub(crate) fn workflow_settings_feedback(
        &self,
        key: &str,
        sid: &str,
        id: &str,
        cx: &App,
    ) -> Option<String> {
        let form = self
            .ws(key)?
            .workflow_settings
            .get(&format!("{sid}\0{id}"))?;
        form.error
            .clone()
            .or_else(|| self.workflow_settings_payload(key, sid, id, cx).err())
    }
    pub(crate) fn apply_workflow_settings(
        &mut self,
        key: &str,
        sid: &str,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.workflow_settings_allowed(key, sid, id)
            || self.workflow_settings_pending(key, sid, id)
            || !self
                .ws(key)
                .is_some_and(|w| w.started && w.inbound.is_some())
        {
            return;
        }
        let (payload, values) = match self.workflow_settings_payload(key, sid, id, cx) {
            Ok(Some(value)) => value,
            Ok(None) => return,
            Err(error) => {
                if let Some(form) = self
                    .ws_mut(key)
                    .and_then(|w| w.workflow_settings.get_mut(&format!("{sid}\0{id}")))
                {
                    form.error = Some(error);
                }
                cx.notify();
                return;
            }
        };
        let form = self
            .ws_mut(key)
            .unwrap()
            .workflow_settings
            .get_mut(&format!("{sid}\0{id}"))
            .unwrap();
        form.error = None;
        form.awaiting = Some(values);
        self.begin_workflow_follow(key, sid, id);
        if !self.send_command(
            key,
            Some(sid.into()),
            "amendWorkflowRunSettings",
            payload,
            None,
            Pending::WorkflowSettings {
                session: sid.into(),
                run: id.into(),
            },
        ) {
            self.workflow_settings_error(key, sid, id, "Workflow settings could not be sent");
        }
        cx.notify();
    }
    pub(crate) fn workflow_settings_error(&mut self, key: &str, sid: &str, id: &str, error: &str) {
        self.settle_workflow_follow(key, sid, id, None);
        if let Some(form) = self
            .ws_mut(key)
            .and_then(|ws| ws.workflow_settings.get_mut(&format!("{sid}\0{id}")))
        {
            form.error = Some(crate::shared::redact::scrub(error));
            form.awaiting = None;
        }
    }
    pub(crate) fn settle_workflow_settings(
        &mut self,
        key: &str,
        sid: &str,
        id: &str,
        ack: Option<&Value>,
    ) {
        self.settle_workflow_follow(key, sid, id, ack);
        let valid = ack.is_some_and(|a| {
            a["status"] == "accepted"
                && a.pointer("/result/type").and_then(Value::as_str)
                    == Some("amendWorkflowRunSettings")
                && ["/result/runId", "/result/toolCallId"].iter().all(|p| {
                    a.pointer(p)
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.trim().is_empty())
                })
        });
        if !valid {
            let error = ack
                .and_then(|a| a["message"].as_str())
                .unwrap_or("Workflow settings were rejected or the outcome is unknown");
            if ack.is_some_and(|a| {
                matches!(a["status"].as_str(), Some("failed" | "rejected" | "stale"))
            }) {
                self.workflow_settings_error(key, sid, id, error);
            } else if let Some(form) = self
                .ws_mut(key)
                .and_then(|w| w.workflow_settings.get_mut(&format!("{sid}\0{id}")))
            {
                form.error = Some(crate::shared::redact::scrub(error));
            }
        }
        // ACK 不携权威 run 状态；只有 stream 确认能推进 baseline，避免重复派发或抹掉新编辑。
    }
}
