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
