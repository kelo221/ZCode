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
    /// One of the validated `failureKind` codes; safe to display, unlike `error`.
    pub failure_kind: Option<String>,
    pub updated_at: Option<String>,
    pub authorization: Option<McpAuthorization>,
}

/// Desktop `mcpStoreStatusList` display status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum McpDisplayStatus {
    Connected,
    Connecting,
    Error,
    Disconnected,
    Unknown,
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
                failure_kind: item.failure_kind,
                updated_at: Some(item.updated_at),
                authorization: item.authorization,
            });
        }
        Ok(list)
    }

    pub(crate) fn display_status(&self) -> McpDisplayStatus {
        match self.status.as_str() {
            "connected" => McpDisplayStatus::Connected,
            "connecting" => McpDisplayStatus::Connecting,
            "failed" => McpDisplayStatus::Error,
            "disconnected" => McpDisplayStatus::Disconnected,
            _ => McpDisplayStatus::Unknown,
        }
    }

    /// Desktop sorts failed rows and pending authorizations to the top.
    pub(crate) fn needs_attention(&self) -> bool {
        self.authorization.is_some() || self.display_status() == McpDisplayStatus::Error
    }

    /// `plugin:<plugin>:<server>` names split into (plugin, server).
    pub(crate) fn plugin_parts(&self) -> Option<(&str, &str)> {
        self.name.strip_prefix("plugin:")?.split_once(':')
    }

    /// Translation key of the localized failure message (desktop
    /// `resolveMcpFailureMessageId`).
    pub(crate) fn failure_key(&self) -> String {
        self.failure_kind.as_deref().map_or_else(
            || "settings.mcp.status.errorReason".into(),
            |kind| format!("settings.mcp.failure.{kind}"),
        )
    }

    /// Translation key of the status-dot tooltip.
    pub(crate) fn status_reason_key(&self) -> String {
        match self.display_status() {
            McpDisplayStatus::Error => self.failure_key(),
            McpDisplayStatus::Connected => "settings.mcp.status.connectedReason".into(),
            McpDisplayStatus::Connecting => "settings.mcp.status.connectingReason".into(),
            McpDisplayStatus::Disconnected => "settings.mcp.status.disconnectedReason".into(),
            McpDisplayStatus::Unknown => "settings.mcp.status.unknownReason".into(),
        }
    }
}

/// Installed servers, attention first, then each plugin's servers grouped by
/// plugin name; order within a group is the CLI's.
pub(crate) fn group_servers(
    servers: &[McpServerSnapshot],
) -> (
    Vec<McpServerSnapshot>,
    Vec<(String, Vec<McpServerSnapshot>)>,
) {
    let mut installed: Vec<McpServerSnapshot> = Vec::new();
    let mut plugins: Vec<(String, Vec<McpServerSnapshot>)> = Vec::new();
    for server in servers {
        match server.plugin_parts() {
            Some((plugin, _)) => match plugins.iter_mut().find(|(name, _)| name == plugin) {
                Some((_, rows)) => rows.push(server.clone()),
                None => plugins.push((plugin.to_owned(), vec![server.clone()])),
            },
            None => installed.push(server.clone()),
        }
    }
    installed.sort_by_key(|server| !server.needs_attention());
    (installed, plugins)
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
pub(crate) mod authorization_tests;
