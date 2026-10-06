//! Strict connection-local projection of the current mcp/list status contract.
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

const INVALID: &str = "Invalid MCP status response";
pub(crate) const MCP_ERROR: &str = "MCP request failed; refresh status to retry";

#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpAuthorization {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "authorizationUrl")]
    authorization_url: String,
    #[serde(rename = "startedAt")]
    pub started_at: String,
}
impl std::fmt::Debug for McpAuthorization {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("McpAuthorization([redacted])")
    }
}
impl McpAuthorization {
    pub(crate) fn url(&self) -> &str {
        &self.authorization_url
    }
    fn valid(&self) -> bool {
        let value = self.url();
        self.kind == "oauth_authorization_code"
            && !self.started_at.trim().is_empty()
            && value.trim() == value
            && (value.to_ascii_lowercase().starts_with("https://")
                || value.to_ascii_lowercase().starts_with("http://"))
            && !value.contains('\\')
            && !value.chars().any(char::is_control)
            && url::Url::parse(value).is_ok_and(|url| {
                matches!(url.scheme(), "http" | "https")
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
            })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct McpServerSnapshot {
    pub name: String,
    pub status: String,
    pub transport: String,
    pub tool_count: u64,
    pub error: Option<String>,
    pub updated_at: Option<String>,
    pub authorization: Option<McpAuthorization>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Status {
    status: String,
    transport: String,
    tool_count: u64,
    updated_at: String,
    error: Option<String>,
    failure_kind: Option<String>,
    server_request_id: Option<String>,
    protocol_era: Option<String>,
    authorization: Option<McpAuthorization>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusList {
    statuses: BTreeMap<String, Status>,
}

impl McpServerSnapshot {
    pub(crate) fn list_from_value(value: &Value) -> Result<Vec<Self>, String> {
        let parsed: StatusList = serde_json::from_value(value.clone()).map_err(|_| INVALID)?;
        let mut list = Vec::new();
        for (name, mut item) in parsed.statuses {
            let raw = &value["statuses"][&name];
            let null = [
                "error",
                "failureKind",
                "serverRequestId",
                "protocolEra",
                "authorization",
            ]
            .iter()
            .any(|key| raw.get(key).is_some_and(Value::is_null));
            if null
                || !matches!(
                    item.status.as_str(),
                    "connecting"
                        | "connected"
                        | "disabled"
                        | "disconnected"
                        | "failed"
                        | "untrusted"
                )
                || !matches!(item.transport.as_str(), "stdio" | "http" | "sse")
                || item.updated_at.trim().is_empty()
                || item
                    .server_request_id
                    .as_ref()
                    .is_some_and(|v| v.trim().is_empty())
                || item
                    .protocol_era
                    .as_deref()
                    .is_some_and(|v| !matches!(v, "legacy" | "modern"))
                || item.failure_kind.as_deref().is_some_and(|v| {
                    !matches!(
                        v,
                        "config_invalid"
                            | "runtime_unavailable"
                            | "process_start_failed"
                            | "network_unreachable"
                            | "connection_timeout"
                            | "protocol_negotiation_failed"
                            | "tool_list_failed"
                            | "unexpected_disconnect"
                            | "oauth_authorization_failed"
                            | "official_origin_untrusted"
                            | "not_authenticated"
                            | "coding_plan_required"
                            | "server_not_found"
                            | "server_unavailable"
                            | "rate_limited"
                            | "server_internal_error"
                            | "protocol_error"
                            | "status_unavailable"
                            | "connection_failed"
                    )
                })
                || item.authorization.as_ref().is_some_and(|a| !a.valid())
            {
                return Err(INVALID.into());
            }
            if let Some(authorization) = &mut item.authorization {
                authorization.authorization_url = url::Url::parse(authorization.url())
                    .map_err(|_| INVALID)?
                    .to_string();
            }
            list.push(Self {
                name,
                status: item.status,
                transport: item.transport,
                tool_count: item.tool_count,
                // OAuth URL 的任意 token 可能被原样回显；通用错误避免凭据进入状态和日志。
                error: item.error.map(|_| MCP_ERROR.into()),
                updated_at: Some(item.updated_at),
                authorization: item.authorization,
            });
        }
        Ok(list)
    }
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
pub(crate) mod authorization_tests;
