//! MCP server status data models for mcp/list inspection.
//!
//! Spec source: packages/shared/src/zcode-protocol/index.ts (zcodeMcpListResultSchema).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct McpServerSnapshot {
    pub name: String,
    pub status: String,
    pub transport: String,
    pub tool_count: u64,
    pub error: Option<String>,
    pub updated_at: Option<String>,
}

impl McpServerSnapshot {
    pub fn list_from_value(v: &Value) -> Vec<Self> {
        let Some(statuses) = v.get("statuses").and_then(Value::as_object) else {
            return Vec::new();
        };

        let mut list = Vec::new();
        for (name, item) in statuses {
            let status = item
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let transport = item
                .get("transport")
                .and_then(Value::as_str)
                .unwrap_or("stdio")
                .to_string();
            let tool_count = item.get("toolCount").and_then(Value::as_u64).unwrap_or(0);
            let error = item
                .get("error")
                .and_then(Value::as_str)
                .map(str::to_string);
            let updated_at = item
                .get("updatedAt")
                .and_then(Value::as_str)
                .map(str::to_string);

            list.push(Self {
                name: name.clone(),
                status,
                transport,
                tool_count,
                error,
                updated_at,
            });
        }
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_mcp_server_snapshot_parsing() {
        let val = json!({
            "statuses": {
                "fetch": {
                    "status": "connected",
                    "transport": "stdio",
                    "toolCount": 2,
                    "updatedAt": "2026-10-03T10:00:00Z"
                },
                "github": {
                    "status": "failed",
                    "transport": "http",
                    "toolCount": 0,
                    "error": "connection refused"
                }
            }
        });

        let list = McpServerSnapshot::list_from_value(&val);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "fetch");
        assert_eq!(list[0].status, "connected");
        assert_eq!(list[0].tool_count, 2);
        assert_eq!(list[1].name, "github");
        assert_eq!(list[1].status, "failed");
        assert_eq!(list[1].error.as_deref(), Some("connection refused"));
    }
}
