use crate::{
    app::store::AppState,
    backend::{inspection::QueryState, workflow_navigation::WorkflowFocus, workspace::Pending},
    shared::workflow_artifact_content::{CONTENT_ERROR, MARKDOWN_MAX_BYTES, decode_markdown},
};
use gpui::Context;
use serde_json::{Value, json};

pub(crate) struct ArtifactContent {
    pub id: String,
    pub version: u64,
    pub selection: String,
    pub query: QueryState<String>,
}
#[derive(Clone)]
pub(crate) struct ContentReceipt {
    pub origin: WorkflowFocus,
    pub id: String,
    pub version: u64,
    pub selection: String,
    pub revision: u64,
}

impl AppState {
    fn markdown_member(&self, origin: &WorkflowFocus, id: &str, version: u64) -> bool {
        self.artifact_origin_current(origin)
            && self.active_workflow_artifacts().is_some_and(|q| {
                !q.query.loading
                    && q.query.error.is_none()
                    && q.query.value.as_ref().is_some_and(|rows| {
                        rows.iter().any(|r| {
                            r.id == id
                                && r.version == version
                                && r.kind == "markdown"
                                && r.content_type.as_deref() == Some("text/markdown")
                        })
                    })
            })
    }
    pub(crate) fn view_workflow_markdown(
        &mut self,
        origin: &WorkflowFocus,
        id: &str,
        version: u64,
        cx: &mut Context<Self>,
    ) {
        if !self.markdown_member(origin, id, version) {
            return;
        }
        let Some(ws) = self.ws_mut(&origin.workspace) else {
            return;
        };
        if !ws.started || ws.inbound.is_none() {
            return;
        }
        let key = (origin.session.clone(), origin.run.clone());
        let query = ws.workflow_artifacts.get_mut(&key).unwrap();
        if query
            .content
            .as_ref()
            .is_some_and(|c| c.id == id && c.version == version)
        {
            return;
        }
        query.content = Some(ArtifactContent {
            id: id.into(),
            version,
            selection: uuid::Uuid::now_v7().to_string(),
            query: Default::default(),
        });
        ws.pending.retain(|_, pending| !matches!(pending, Pending::WorkflowArtifactContent(r) if r.origin == *origin));
        self.request_workflow_markdown(origin, cx);
    }
    fn request_workflow_markdown(&mut self, origin: &WorkflowFocus, cx: &mut Context<Self>) {
        let Some(ws) = self.ws_mut(&origin.workspace) else {
            return;
        };
        if !ws.started || ws.inbound.is_none() {
            return;
        }
        let Some(content) = ws
            .workflow_artifacts
            .get_mut(&(origin.session.clone(), origin.run.clone()))
            .and_then(|q| q.content.as_mut())
        else {
            return;
        };
        content.query.start();
        let receipt = ContentReceipt {
            origin: origin.clone(),
            id: content.id.clone(),
            version: content.version,
            selection: content.selection.clone(),
            revision: content.query.revision,
        };
        let id = ws.next_id();
        let line = json!({"id":id,"method":"v4/conversation/workflowRunArtifactRead","params":{"sessionId":origin.session,"runId":origin.run,"artifactId":receipt.id,"version":receipt.version,"offset":0,"limit":MARKDOWN_MAX_BYTES}}).to_string();
        ws.pending
            .insert(id, Pending::WorkflowArtifactContent(receipt));
        if !ws.send_pending_line(id, line) {
            ws.workflow_artifacts
                .get_mut(&(origin.session.clone(), origin.run.clone()))
                .unwrap()
                .content
                .as_mut()
                .unwrap()
                .query
                .fail(CONTENT_ERROR);
        }
        cx.notify();
    }
    pub(crate) fn retry_workflow_markdown(
        &mut self,
        origin: &WorkflowFocus,
        id: &str,
        version: u64,
        cx: &mut Context<Self>,
    ) {
        if !self.markdown_member(origin, id, version)
            || !self
                .active_workflow_artifacts()
                .and_then(|q| q.content.as_ref())
                .is_some_and(|c| {
                    c.id == id
                        && c.version == version
                        && !c.query.loading
                        && c.query.error.is_some()
                })
        {
            return;
        }
        self.request_workflow_markdown(origin, cx);
    }
    pub(crate) fn close_workflow_markdown(
        &mut self,
        origin: &WorkflowFocus,
        id: &str,
        version: u64,
        cx: &mut Context<Self>,
    ) {
        if !self.artifact_origin_current(origin)
            || !self
                .active_workflow_artifacts()
                .and_then(|q| q.content.as_ref())
                .is_some_and(|c| c.id == id && c.version == version)
        {
            return;
        }
        self.clear_workflow_artifact_content(origin);
        cx.notify();
    }
    pub(crate) fn markdown_selection_current(
        &self,
        origin: &WorkflowFocus,
        selection: &str,
    ) -> bool {
        self.artifact_origin_current(origin)
            && self
                .active_workflow_artifacts()
                .and_then(|q| q.content.as_ref())
                .is_some_and(|c| c.selection == selection)
    }
    pub(crate) fn retire_stale_workflow_artifact_contents(&mut self) {
        let stale = self
            .workspaces
            .iter()
            .flat_map(|ws| ws.workflow_artifacts.values())
            .filter(|q| q.content.is_some() && !self.artifact_origin_current(&q.origin))
            .map(|q| q.origin.clone())
            .collect::<Vec<_>>();
        for origin in stale {
            self.clear_workflow_artifact_content(&origin);
        }
    }
    pub(crate) fn retire_workflow_artifact_contents(&mut self) {
        for ws in &mut self.workspaces {
            for query in ws.workflow_artifacts.values_mut() {
                query.content = None;
            }
            ws.pending
                .retain(|_, pending| !matches!(pending, Pending::WorkflowArtifactContent(_)));
        }
    }
    pub(crate) fn clear_workflow_artifact_content(&mut self, origin: &WorkflowFocus) {
        if let Some(ws) = self.ws_mut(&origin.workspace) {
            if let Some(q) = ws
                .workflow_artifacts
                .get_mut(&(origin.session.clone(), origin.run.clone()))
            {
                q.content = None;
            }
            ws.pending.retain(|_, pending| !matches!(pending, Pending::WorkflowArtifactContent(r) if r.origin == *origin));
        }
    }
    pub(crate) fn handle_workflow_content_response(
        &mut self,
        workspace: &str,
        id: u64,
        result: &Option<Value>,
        error: &Option<Value>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.ws(workspace).is_some_and(|ws| {
            matches!(
                ws.pending.get(&id),
                Some(Pending::WorkflowArtifactContent(_))
            )
        }) {
            return false;
        }
        let Some(Pending::WorkflowArtifactContent(receipt)) =
            self.ws_mut(workspace).and_then(|ws| ws.pending.remove(&id))
        else {
            return true;
        };
        if receipt.origin.workspace != workspace
            || !self.markdown_member(&receipt.origin, &receipt.id, receipt.version)
        {
            return true;
        }
        let Some(content) = self
            .ws_mut(workspace)
            .and_then(|ws| {
                ws.workflow_artifacts
                    .get_mut(&(receipt.origin.session.clone(), receipt.origin.run.clone()))
            })
            .and_then(|q| q.content.as_mut())
            .filter(|c| {
                c.id == receipt.id
                    && c.version == receipt.version
                    && c.selection == receipt.selection
                    && c.query.revision == receipt.revision
                    && c.query.loading
            })
        else {
            return true;
        };
        // 内容读错误可能含 store 路径或正文；专用关联只保存固定错误，绝不走通用 banner。
        match error.as_ref().map_or_else(
            || {
                result
                    .as_ref()
                    .ok_or(CONTENT_ERROR)
                    .and_then(decode_markdown)
            },
            |_| Err(CONTENT_ERROR),
        ) {
            Ok(text) => content.query.finish(text),
            Err(error) => content.query.fail(error),
        }
        cx.notify();
        true
    }
}
