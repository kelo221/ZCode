use super::*;
use serde_json::json;

pub(crate) fn fixture(url: Option<&str>) -> Value {
    let mut value = json!({"statuses":{"fixture":{"status":"connecting","transport":"http",
        "toolCount":0,"updatedAt":"2026-10-06T10:00:00Z"}}});
    if let Some(url) = url {
        value["statuses"]["fixture"]["authorization"] = json!({
            "type":"oauth_authorization_code","authorizationUrl":url,"startedAt":"2026-10-06T10:00:00Z"
        });
    }
    value
}

#[test]
fn mcp_authorization_projection_preserves_current_receipt_without_debug_or_error_secrets() {
    let mut value = fixture(Some(
        "https://auth.example.test/authorize?state=sentinel-secret",
    ));
    value["statuses"]["fixture"]["error"] = json!("unlabelled-sentinel-secret");
    let list = McpServerSnapshot::list_from_value(&value).unwrap();
    let authorization = list[0].authorization.as_ref().unwrap();
    assert_eq!(
        authorization.url(),
        "https://auth.example.test/authorize?state=sentinel-secret"
    );
    assert_eq!(authorization.started_at, "2026-10-06T10:00:00Z");
    assert!(!format!("{list:?}").contains("sentinel-secret"));
    assert!(!list[0].error.as_ref().unwrap().contains("sentinel-secret"));
    assert!(
        McpServerSnapshot::list_from_value(&fixture(None)).unwrap()[0]
            .authorization
            .is_none()
    );
}

fn list(entries: &[(&str, &str)]) -> Vec<McpServerSnapshot> {
    let mut statuses = serde_json::Map::new();
    for (name, status) in entries {
        statuses.insert(
            (*name).into(),
            json!({"status":status,"transport":"stdio","toolCount":3,
                "updatedAt":"2026-10-22T10:00:00Z"}),
        );
    }
    McpServerSnapshot::list_from_value(&json!({ "statuses": statuses })).unwrap()
}

#[test]
fn mcp_projection_keeps_failure_kind_and_maps_status_like_desktop() {
    let mut value = fixture(None);
    value["statuses"]["fixture"]["status"] = json!("failed");
    value["statuses"]["fixture"]["failureKind"] = json!("process_start_failed");
    value["statuses"]["fixture"]["error"] = json!("spawn secret-path failed");
    let failed = &McpServerSnapshot::list_from_value(&value).unwrap()[0];
    assert_eq!(failed.failure_kind.as_deref(), Some("process_start_failed"));
    assert_eq!(failed.display_status(), McpDisplayStatus::Error);
    assert_eq!(
        failed.failure_key(),
        "settings.mcp.failure.process_start_failed"
    );
    assert_eq!(failed.status_reason_key(), failed.failure_key());
    assert!(failed.needs_attention());
    for (status, expected) in [
        ("connected", McpDisplayStatus::Connected),
        ("connecting", McpDisplayStatus::Connecting),
        ("disconnected", McpDisplayStatus::Disconnected),
        ("disabled", McpDisplayStatus::Unknown),
        ("untrusted", McpDisplayStatus::Unknown),
    ] {
        assert_eq!(
            list(&[("s", status)])[0].display_status(),
            expected,
            "{status}"
        );
    }
    let mut no_kind = failed.clone();
    no_kind.failure_kind = None;
    assert_eq!(no_kind.failure_key(), "settings.mcp.status.errorReason");
}

#[test]
fn mcp_groups_put_attention_first_and_split_plugin_servers() {
    let servers = list(&[
        ("alpha", "connected"),
        ("beta", "failed"),
        ("gamma", "disconnected"),
        ("plugin:pdf:pdf", "connected"),
        ("plugin:documents:documents", "connected"),
        ("plugin:documents:extra", "failed"),
    ]);
    let (installed, plugins) = group_servers(&servers);
    let names: Vec<_> = installed.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["beta", "alpha", "gamma"]);
    let groups: Vec<_> = plugins
        .iter()
        .map(|(plugin, rows)| (plugin.as_str(), rows.len()))
        .collect();
    assert_eq!(groups, [("documents", 2), ("pdf", 1)]);
    assert_eq!(plugins[0].1[1].plugin_parts(), Some(("documents", "extra")));
    assert_eq!(list(&[("plain", "connected")])[0].plugin_parts(), None);
}

#[test]
fn mcp_display_translation_keys_resolve() {
    use crate::shared::i18n::{Locale, t_locale};
    let mut keys: Vec<String> = [
        "settings.plugin.mcp.installed",
        "settings.mcp.emptyTitle",
        "settings.mcp.emptyDescription",
        "settings.mcp.status.toolCount",
        "settings.mcp.status.errorReason",
        "settings.mcp.status.connectedReason",
        "settings.mcp.status.connectingReason",
        "settings.mcp.status.disconnectedReason",
        "settings.mcp.status.unknownReason",
        "settings.mcp.plugin.connectedDescription",
        "settings.mcp.plugin.connectingDescription",
        "settings.mcp.plugin.disconnectedDescription",
        "settings.mcp.plugin.unavailableDescription",
        "settings.mcp.plugin.authorizationRequiredDescription",
        "settings.resourceGroup.item.one",
        "settings.resourceGroup.item.other",
    ]
    .map(String::from)
    .to_vec();
    keys.extend(
        [
            "config_invalid",
            "runtime_unavailable",
            "process_start_failed",
            "network_unreachable",
            "connection_timeout",
            "protocol_negotiation_failed",
            "tool_list_failed",
            "unexpected_disconnect",
            "oauth_authorization_failed",
            "official_origin_untrusted",
            "not_authenticated",
            "coding_plan_required",
            "server_not_found",
            "server_unavailable",
            "rate_limited",
            "server_internal_error",
            "protocol_error",
            "status_unavailable",
            "connection_failed",
        ]
        .map(|kind| format!("settings.mcp.failure.{kind}")),
    );
    for key in keys {
        for locale in [Locale::EnUs, Locale::ZhCn] {
            assert_ne!(t_locale(&key, locale), key, "{key}");
        }
    }
}

#[test]
fn mcp_authorization_rejects_unsafe_urls_without_echoing_them() {
    for url in [
        "javascript:alert(1)",
        "file:///C:/secret",
        "data:text/html,test",
        "/relative",
        "https://user:private@auth.example.test/",
        "https://auth.example.test/\nprivate",
        "https://auth.example.test/\0private",
        " https://auth.example.test/",
        "https://",
        "https:auth.example.test",
        "https:/auth.example.test",
        "https:\\auth.example.test",
    ] {
        let error = McpServerSnapshot::list_from_value(&fixture(Some(url))).unwrap_err();
        assert_eq!(error, "Invalid MCP status response");
    }
    for url in [
        "https://auth.example.test/authorize?state=a%26b",
        "http://127.0.0.1:1234/authorize",
    ] {
        assert!(McpServerSnapshot::list_from_value(&fixture(Some(url))).is_ok());
    }
}

#[test]
fn mcp_status_projection_rejects_malformed_required_optional_and_unknown_fields() {
    for (field, bad) in [
        ("status", json!("ready")),
        ("transport", json!("local")),
        ("toolCount", json!(-1)),
        ("updatedAt", json!("")),
        ("error", Value::Null),
        ("authorization", Value::Null),
        ("failureKind", json!("other")),
        ("protocolEra", json!("future")),
        ("serverRequestId", json!(" ")),
        ("newField", json!(true)),
    ] {
        let mut value = fixture(None);
        value["statuses"]["fixture"][field] = bad;
        assert!(
            McpServerSnapshot::list_from_value(&value).is_err(),
            "{field}"
        );
    }
    let mut value = fixture(Some("https://auth.example.test/"));
    value["statuses"]["fixture"]["authorization"]["type"] = json!("other");
    assert!(McpServerSnapshot::list_from_value(&value).is_err());
    assert!(McpServerSnapshot::list_from_value(&json!({"statuses":{},"extra":true})).is_err());
    assert!(McpServerSnapshot::list_from_value(&json!({})).is_err());
    assert!(
        McpServerSnapshot::list_from_value(&json!({"statuses":{}}))
            .unwrap()
            .is_empty()
    );
}
