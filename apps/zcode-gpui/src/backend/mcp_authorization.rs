use crate::app::store::AppState;
use crate::shared::mcp::McpAuthorization;

#[derive(Clone)]
pub(crate) struct McpAuthorizationReceipt {
    workspace: String,
    generation: u64,
    query_revision: u64,
    server: String,
    authorization: McpAuthorization,
}

impl AppState {
    pub(crate) fn open_mcp_authorization(
        &self,
        receipt: &McpAuthorizationReceipt,
        open: impl FnOnce(&str),
    ) -> bool {
        let Some(url) = self.current_mcp_authorization_url(receipt) else {
            return false;
        };
        open(url);
        true
    }

    pub(crate) fn mcp_authorization_open_failed(&mut self, receipt: &McpAuthorizationReceipt) {
        if self.matching_mcp_authorization_url(receipt).is_some() {
            self.ws_mut(&receipt.workspace)
                .unwrap()
                .inspection
                .mcp
                .fail("Authorization page could not be opened; refresh status and retry");
        }
    }

    pub(crate) fn mcp_authorization_receipt(
        &self,
        server: &str,
    ) -> Option<McpAuthorizationReceipt> {
        let workspace = self.active_ws_key()?;
        let ws = self.ws(&workspace)?;
        let query = &ws.inspection.mcp;
        if !ws.started || ws.inbound.is_none() || query.loading || query.error.is_some() {
            return None;
        }
        let authorization = query
            .value
            .as_ref()?
            .iter()
            .find(|s| s.name == server)?
            .authorization
            .clone()?;
        Some(McpAuthorizationReceipt {
            workspace,
            generation: ws.generation,
            query_revision: query.revision,
            server: server.into(),
            authorization,
        })
    }

    pub(crate) fn current_mcp_authorization_url<'a>(
        &'a self,
        receipt: &McpAuthorizationReceipt,
    ) -> Option<&'a str> {
        if self.active_ws_key().as_deref() != Some(&receipt.workspace) {
            return None;
        }
        self.matching_mcp_authorization_url(receipt)
    }

    fn matching_mcp_authorization_url<'a>(
        &'a self,
        receipt: &McpAuthorizationReceipt,
    ) -> Option<&'a str> {
        let ws = self.ws(&receipt.workspace)?;
        let query = &ws.inspection.mcp;
        // 刷新和重连会使已渲染按钮失效；旧 OAuth URL 不能绕过当前 owner 和查询版本。
        if ws.generation != receipt.generation
            || query.revision != receipt.query_revision
            || !ws.started
            || ws.inbound.is_none()
            || query.loading
            || query.error.is_some()
        {
            return None;
        }
        let current = query
            .value
            .as_ref()?
            .iter()
            .find(|s| s.name == receipt.server)?
            .authorization
            .as_ref()?;
        (current == &receipt.authorization).then(|| current.url())
    }
}
