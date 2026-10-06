use crate::app::store::AppState;
use crate::backend::workspace::Pending;
use crate::composer::references::{CatalogKind, CatalogState, parse_catalog};
use gpui::Context;
use serde_json::{Value, json};

#[cfg(test)]
#[path = "reference_cmds_tests.rs"]
mod tests;

impl AppState {
    pub(crate) fn ensure_reference_catalog(
        &mut self,
        kind: CatalogKind,
        refresh: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.active_ws_key() else {
            return;
        };
        if self.is_read_only_view() {
            return;
        }
        let session = self.active.clone();
        let Some(ws) = self.ws_mut(&key) else {
            return;
        };
        if !ws.started || ws.inbound.is_none() {
            return;
        }
        let state = ws
            .reference_catalogs
            .entry(session.clone())
            .or_default()
            .state_mut(kind);
        if matches!(state, CatalogState::Loading)
            || (!refresh && !matches!(state, CatalogState::Idle))
        {
            return;
        }
        *state = CatalogState::Loading;
        let workspace = json!({"workspacePath": ws.path.to_string_lossy(), "workspaceKey": ws.key});
        let mut params = json!({"workspace": workspace});
        if let Some(sid) = &session {
            params["sessionId"] = json!(sid);
        }
        let id = ws.next_id();
        ws.pending.insert(
            id,
            Pending::ReferenceCatalog {
                session: session.clone(),
                kind,
            },
        );
        if !ws.send_pending_line(
            id,
            json!({"id": id, "method": kind.method(), "params": params}).to_string(),
        ) {
            *ws.reference_catalogs
                .entry(session)
                .or_default()
                .state_mut(kind) =
                CatalogState::Failed("Reference catalog request could not be sent".into());
        }
        cx.notify();
    }

    pub(crate) fn settle_reference_catalog(
        &mut self,
        workspace: &str,
        session: Option<String>,
        kind: CatalogKind,
        result: Result<Value, String>,
    ) {
        let parsed = result.and_then(|value| parse_catalog(value, kind, session.is_some()));
        if let Some(ws) = self.ws_mut(workspace) {
            *ws.reference_catalogs
                .entry(session)
                .or_default()
                .state_mut(kind) = match parsed {
                Ok(entries) => CatalogState::Ready(entries),
                Err(error) => CatalogState::Failed(crate::shared::redact::scrub(&error)),
            };
        }
    }
}
