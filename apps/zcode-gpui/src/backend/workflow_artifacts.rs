use crate::{
    app::store::AppState,
    backend::{inspection::QueryState, workflow_navigation::WorkflowFocus, workspace::Pending},
    shared::workflow_artifacts::{
        ARTIFACT_ERROR, ARTIFACT_UNAVAILABLE, RunArtifactMetadata, parse_artifacts,
    },
};
use gpui::Context;
use serde_json::{Value, json};
use std::collections::HashMap;

type ArtifactKey = (String, String);
pub(crate) type RunArtifactQueries = HashMap<ArtifactKey, RunArtifactQuery>;

pub(crate) struct RunArtifactQuery {
    pub origin: WorkflowFocus,
    pub query: QueryState<Vec<RunArtifactMetadata>>,
    pub content: Option<crate::backend::workflow_artifact_content::ArtifactContent>,
}
#[derive(Clone)]
pub(crate) struct ArtifactQueryReceipt {
    pub origin: WorkflowFocus,
    pub revision: u64,
}

fn key(origin: &WorkflowFocus) -> ArtifactKey {
    (origin.session.clone(), origin.run.clone())
}

impl AppState {
    pub(crate) fn artifact_origin_current(&self, origin: &WorkflowFocus) -> bool {
        self.viewing_child.is_none()
            && self.workflow_focus() == Some(origin)
            && self.conversations.get(&origin.session).is_some_and(|c| {
                c.workflow_runs.runs.iter().any(|r| {
                    r.run_id == origin.run && r.tool_call_id.as_deref() == Some(&origin.tool)
                })
            })
    }
    pub(crate) fn active_workflow_artifacts(&self) -> Option<&RunArtifactQuery> {
        let origin = self.workflow_focus()?;
        if !self.artifact_origin_current(origin) {
            return None;
        }
        self.ws(&origin.workspace)?
            .workflow_artifacts
            .get(&key(origin))
            .filter(|q| q.origin == *origin)
    }
    pub(crate) fn refresh_workflow_artifacts(
        &mut self,
        origin: &WorkflowFocus,
        cx: &mut Context<Self>,
    ) {
        if self.artifact_origin_current(origin) {
            self.ensure_workflow_artifacts(true, cx);
        }
    }
    pub(crate) fn ensure_workflow_artifacts(&mut self, refresh: bool, cx: &mut Context<Self>) {
        let Some(origin) = self.workflow_focus().cloned() else {
            return;
        };
        if !self.artifact_origin_current(&origin) {
            return;
        }
        if refresh {
            self.clear_workflow_artifact_content(&origin);
        }
        let Some(ws) = self.ws_mut(&origin.workspace) else {
            return;
        };
        if !ws.started || ws.inbound.is_none() {
            return;
        }
        let cache_key = key(&origin);
        if ws
            .workflow_artifacts
            .get(&cache_key)
            .is_some_and(|q| q.origin != origin)
        {
            // 同 run 的重新进入也有新 selection；旧请求不能填入新详情页。
            ws.workflow_artifacts.remove(&cache_key);
            ws.pending.retain(|_, pending| match pending {
                Pending::WorkflowArtifacts(r) => key(&r.origin) != cache_key,
                Pending::WorkflowArtifactContent(r) => key(&r.origin) != cache_key,
                _ => true,
            });
        }
        if !ws.workflow_artifacts.contains_key(&cache_key) && ws.workflow_artifacts.len() >= 32 {
            let Some(removable) = ws.workflow_artifacts.keys().next().cloned() else {
                return;
            };
            ws.workflow_artifacts.remove(&removable);
            ws.pending.retain(|_, pending| match pending {
                Pending::WorkflowArtifacts(r) => key(&r.origin) != removable,
                Pending::WorkflowArtifactContent(r) => key(&r.origin) != removable,
                _ => true,
            });
        }
        let query = ws
            .workflow_artifacts
            .entry(cache_key.clone())
            .or_insert_with(|| RunArtifactQuery {
                origin: origin.clone(),
                query: Default::default(),
                content: None,
            });
        if query.query.loading || query.query.attempted && !refresh {
            return;
        }
        query.query.start();
        let receipt = ArtifactQueryReceipt {
            origin: origin.clone(),
            revision: query.query.revision,
        };
        let id = ws.next_id();
        ws.pending.insert(id, Pending::WorkflowArtifacts(receipt));
        if !ws.send_pending_line(id, json!({"id":id,"method":"v4/conversation/workflowRunArtifacts","params":{"sessionId":origin.session,"runId":origin.run}}).to_string()) {
            ws.workflow_artifacts.get_mut(&cache_key).unwrap().query.fail(ARTIFACT_ERROR);
        }
        cx.notify();
    }
    pub(crate) fn handle_workflow_artifact_response(
        &mut self,
        workspace: &str,
        id: u64,
        result: &Option<Value>,
        error: &Option<Value>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .ws(workspace)
            .is_some_and(|ws| matches!(ws.pending.get(&id), Some(Pending::WorkflowArtifacts(_))))
        {
            return false;
        }
        let Some(Pending::WorkflowArtifacts(receipt)) =
            self.ws_mut(workspace).and_then(|ws| ws.pending.remove(&id))
        else {
            return true;
        };
        if receipt.origin.workspace != workspace || !self.artifact_origin_current(&receipt.origin) {
            return true;
        }
        let Some(query) = self
            .ws_mut(workspace)
            .and_then(|ws| ws.workflow_artifacts.get_mut(&key(&receipt.origin)))
            .filter(|q| {
                q.origin == receipt.origin
                    && q.query.revision == receipt.revision
                    && q.query.loading
            })
        else {
            return true;
        };
        if let Some(error) = error {
            // 通用 RPC 没有保留 capability reasonCode；只看该查询的错误类，不保存 message/stack。
            let unsupported = error["code"] == -32601
                || error["code"] == -32603
                    && error["data"]["name"] == "V4CapabilityUnsupportedError";
            query.query.fail(if unsupported {
                ARTIFACT_UNAVAILABLE
            } else {
                ARTIFACT_ERROR
            });
        } else {
            match result
                .as_ref()
                .ok_or(ARTIFACT_ERROR)
                .and_then(parse_artifacts)
            {
                Ok(rows) => query.query.finish(rows),
                Err(error) => query.query.fail(error),
            }
        }
        cx.notify();
        true
    }
}
