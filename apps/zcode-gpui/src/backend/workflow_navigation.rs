use crate::app::store::AppState;
use crate::conversation::workflows_types::WorkflowRunState;
use serde_json::Value;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct WorkflowFocus {
    pub workspace: String,
    pub session: String,
    pub run: String,
    pub tool: String,
    generation: u64,
    navigation: u64,
    selection: u64,
}
#[derive(Clone)]
pub(crate) struct WorkflowFollow {
    origin: WorkflowFocus,
    target: Option<(String, String)>,
}

impl AppState {
    pub(crate) fn workflow_focus(&self) -> Option<&WorkflowFocus> {
        self.workflow_focus
            .as_ref()
            .filter(|focus| self.workflow_focus_matches(focus))
    }
    pub(crate) fn workflow_link_current(&self, focus: &WorkflowFocus) -> bool {
        self.workflow_focus_matches(focus)
    }
    fn workflow_focus_matches(&self, focus: &WorkflowFocus) -> bool {
        self.active_ws_key().as_deref() == Some(&focus.workspace)
            && self.active.as_deref() == Some(&focus.session)
            && self.navigation_generation == focus.navigation
            && self.workflow_navigation_generation == focus.selection
            && self
                .ws(&focus.workspace)
                .is_some_and(|w| w.generation == focus.generation)
    }
    fn projected_workflow(&self, sid: &str, run: &str, tool: &str) -> Option<&WorkflowRunState> {
        self.conversations
            .get(sid)?
            .workflow_runs
            .runs
            .iter()
            .find(|r| r.run_id == run && r.tool_call_id.as_deref() == Some(tool))
    }
    pub(crate) fn focus_workflow(&mut self, key: &str, sid: &str, run: &str, tool: &str) -> bool {
        if self.active_ws_key().as_deref() != Some(key)
            || self.active.as_deref() != Some(sid)
            || run.trim().is_empty()
            || tool.trim().is_empty()
            || self.projected_workflow(sid, run, tool).is_none()
        {
            return false;
        }
        let Some(generation) = self.ws(key).map(|ws| ws.generation) else {
            return false;
        };
        self.retire_workflow_artifact_contents();
        self.workflow_navigation_generation = self
            .workflow_navigation_generation
            .checked_add(1)
            .expect("workflow navigation generation exhausted");
        self.workflow_focus = Some(WorkflowFocus {
            workspace: key.into(),
            session: sid.into(),
            run: run.into(),
            tool: tool.into(),
            generation,
            navigation: self.navigation_generation,
            selection: self.workflow_navigation_generation,
        });
        self.workflow_follow = None;
        true
    }
    pub(crate) fn workflow_successor(
        &self,
        key: &str,
        sid: &str,
        source: &str,
    ) -> Option<(String, String)> {
        if self.active_ws_key().as_deref() != Some(key) || self.active.as_deref() != Some(sid) {
            return None;
        }
        let runs = &self.conversations.get(sid)?.workflow_runs.runs;
        let id = runs
            .iter()
            .find(|r| r.run_id == source)?
            .superseded_by
            .as_deref()?;
        if id == source {
            return None;
        }
        let target = runs.iter().find(|r| r.run_id == id)?;
        let tool = target
            .tool_call_id
            .as_ref()
            .filter(|t| !t.trim().is_empty())?;
        Some((id.into(), tool.clone()))
    }
    pub(crate) fn open_workflow_successor(
        &mut self,
        origin: &WorkflowFocus,
        source: &str,
        run: &str,
        tool: &str,
    ) -> bool {
        if !self.workflow_focus_matches(origin)
            || self.workflow_successor(&origin.workspace, &origin.session, source)
                != Some((run.into(), tool.into()))
        {
            return false;
        }
        self.focus_workflow(&origin.workspace, &origin.session, run, tool)
    }
    pub(crate) fn workflow_link_origin(&self, key: &str, sid: &str) -> Option<WorkflowFocus> {
        let ws = self.ws(key)?;
        Some(WorkflowFocus {
            workspace: key.into(),
            session: sid.into(),
            run: String::new(),
            tool: String::new(),
            generation: ws.generation,
            navigation: self.navigation_generation,
            selection: self.workflow_navigation_generation,
        })
    }
    pub(crate) fn all_workflow_runs(&mut self) {
        self.retire_workflow_artifact_contents();
        self.workflow_navigation_generation = self
            .workflow_navigation_generation
            .checked_add(1)
            .expect("workflow navigation generation exhausted");
        self.workflow_focus = None;
        self.workflow_follow = None;
    }
    pub(crate) fn begin_workflow_follow(&mut self, key: &str, sid: &str, run: &str) {
        self.workflow_follow = self
            .workflow_focus()
            .filter(|f| f.workspace == key && f.session == sid && f.run == run)
            .cloned()
            .map(|origin| WorkflowFollow {
                origin,
                target: None,
            });
    }
    pub(crate) fn settle_workflow_follow(
        &mut self,
        key: &str,
        sid: &str,
        source: &str,
        ack: Option<&Value>,
    ) {
        let Some(follow) = self.workflow_follow.as_ref().filter(|f| {
            f.origin.workspace == key && f.origin.session == sid && f.origin.run == source
        }) else {
            return;
        };
        let target = ack
            .filter(|a| {
                a["status"] == "accepted" && a["result"]["type"] == "amendWorkflowRunSettings"
            })
            .and_then(|a| {
                Some((
                    a["result"]["runId"].as_str()?.to_owned(),
                    a["result"]["toolCallId"].as_str()?.to_owned(),
                ))
            });
        if target.as_ref().is_none_or(|(run, tool)| {
            run == source || run.trim().is_empty() || tool.trim().is_empty()
        }) || !self.workflow_focus_matches(&follow.origin)
        {
            self.workflow_follow = None;
            return;
        }
        self.workflow_follow.as_mut().unwrap().target = target;
        self.reconcile_workflow_follow();
    }
    pub(crate) fn reconcile_workflow_follow(&mut self) {
        let Some(follow) = self.workflow_follow.clone() else {
            return;
        };
        if !self.workflow_focus_matches(&follow.origin)
            || self
                .workflow_focus()
                .is_none_or(|f| f.run != follow.origin.run || f.tool != follow.origin.tool)
        {
            self.workflow_follow = None;
            return;
        }
        let Some((run, tool)) = follow.target else {
            return;
        };
        if self
            .projected_workflow(&follow.origin.session, &run, &tool)
            .is_some()
        {
            // ACK 只提供导航 hint；目标出现在原 owner 的权威投影后才切换，避免跨会话抢焦点。
            self.focus_workflow(
                &follow.origin.workspace,
                &follow.origin.session,
                &run,
                &tool,
            );
        }
    }
}

#[cfg(test)]
#[path = "workflow_navigation_tests.rs"]
mod tests;
