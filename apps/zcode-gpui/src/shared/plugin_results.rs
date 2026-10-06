//! Validation of plugin result boundaries before committing query/action feedback.

use serde_json::{Map, Value};

type Object = Map<String, Value>;
type Check = Result<(), String>;

fn object<'a>(v: &'a Value, allowed: &[&str]) -> Result<&'a Object, String> {
    let o = v.as_object().ok_or("Malformed plugin response")?;
    if o.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("Unexpected plugin response field".into());
    }
    Ok(o)
}
fn string(v: &Value) -> Check {
    v.as_str()
        .map(|_| ())
        .ok_or("Malformed plugin string".into())
}
fn name(v: &Value) -> Check {
    string(v)?;
    if v.as_str().unwrap().is_empty() {
        return Err("Missing plugin identity".into());
    }
    Ok(())
}
fn boolean(v: &Value) -> Check {
    v.as_bool()
        .map(|_| ())
        .ok_or("Malformed plugin boolean".into())
}
fn number(v: &Value) -> Check {
    v.as_u64()
        .map(|_| ())
        .ok_or("Malformed plugin count".into())
}
fn array(v: &Value, check: fn(&Value) -> Check) -> Check {
    for item in v.as_array().ok_or("Malformed plugin array")? {
        check(item)?;
    }
    Ok(())
}
fn field(o: &Object, key: &str, check: fn(&Value) -> Check) -> Check {
    check(o.get(key).ok_or("Missing plugin response field")?)
}
fn optional(o: &Object, key: &str, check: fn(&Value) -> Check) -> Check {
    if let Some(v) = o.get(key) {
        check(v)?;
    }
    Ok(())
}
fn text_fields(o: &Object, keys: &[&str]) -> Check {
    for key in keys {
        optional(o, key, string)?;
    }
    Ok(())
}
fn strings(v: &Value) -> Check {
    array(v, string)
}
fn diagnostic(v: &Value) -> Check {
    let o = object(v, &["code", "message", "severity", "pluginId"])?;
    field(o, "code", string)?;
    field(o, "message", string)?;
    optional(o, "pluginId", string)?;
    if let Some(s) = o.get("severity")
        && !matches!(s.as_str(), Some("error" | "warning"))
    {
        return Err("Malformed plugin severity".into());
    }
    Ok(())
}
fn diagnostics(v: &Value) -> Check {
    array(v, diagnostic)
}
fn json_object(v: &Value) -> Check {
    v.as_object()
        .map(|_| ())
        .ok_or("Malformed plugin object".into())
}
fn refresh_failure(v: &Value) -> Check {
    let o = object(v, &["code", "failedAt", "message"])?;
    for key in ["code", "failedAt", "message"] {
        field(o, key, string)?;
    }
    Ok(())
}
fn marketplace(v: &Value) -> Check {
    let o = object(
        v,
        &[
            "id",
            "name",
            "source",
            "pluginCount",
            "description",
            "lastUpdated",
            "isOfficial",
            "featured",
            "refreshFailure",
        ],
    )?;
    field(o, "id", name)?;
    field(o, "name", name)?;
    field(o, "source", json_object)?;
    field(o, "pluginCount", number)?;
    text_fields(o, &["description", "lastUpdated"])?;
    optional(o, "isOfficial", boolean)?;
    optional(o, "featured", strings)?;
    optional(o, "refreshFailure", refresh_failure)
}
fn marketplaces(v: &Value) -> Check {
    array(v, marketplace)
}
fn listing(v: &Value) -> Check {
    let o = object(
        v,
        &[
            "displayName",
            "displayNameI18n",
            "descriptionI18n",
            "icon",
            "category",
            "author",
            "authorUrl",
            "homepage",
            "privacyPolicy",
            "termsOfService",
            "heroImage",
            "examplePrompts",
            "examplePromptsI18n",
            "requiresPaidPlan",
        ],
    )?;
    text_fields(
        o,
        &[
            "displayName",
            "icon",
            "category",
            "author",
            "authorUrl",
            "homepage",
            "privacyPolicy",
            "termsOfService",
            "heroImage",
        ],
    )?;
    optional(o, "examplePrompts", strings)?;
    optional(o, "requiresPaidPlan", boolean)?;
    for key in ["displayNameI18n", "descriptionI18n", "examplePromptsI18n"] {
        if let Some(v) = o.get(key) {
            for value in v.as_object().ok_or("Malformed plugin locale map")?.values() {
                if key == "examplePromptsI18n" {
                    strings(value)?;
                } else {
                    string(value)?;
                }
            }
        }
    }
    Ok(())
}
fn available(v: &Value) -> Check {
    let o = object(
        v,
        &[
            "id",
            "name",
            "marketplace",
            "description",
            "version",
            "installed",
            "componentTypes",
            "listing",
        ],
    )?;
    for key in ["id", "name", "marketplace"] {
        field(o, key, name)?;
    }
    field(o, "installed", boolean)?;
    text_fields(o, &["description", "version"])?;
    optional(o, "componentTypes", strings)?;
    optional(o, "listing", listing)
}
fn scope(v: &Value) -> Check {
    if matches!(v.as_str(), Some("user" | "workspace")) {
        Ok(())
    } else {
        Err("Malformed plugin scope".into())
    }
}
fn installed(v: &Value) -> Check {
    let o = object(
        v,
        &[
            "id",
            "name",
            "marketplace",
            "description",
            "version",
            "enabled",
            "scope",
            "installPath",
            "installedAt",
            "componentTypes",
            "hookDetails",
            "updateStatus",
            "latestVersion",
            "listing",
        ],
    )?;
    for key in ["id", "name", "marketplace"] {
        field(o, key, name)?;
    }
    field(o, "enabled", boolean)?;
    field(o, "scope", scope)?;
    text_fields(
        o,
        &[
            "description",
            "version",
            "installPath",
            "installedAt",
            "latestVersion",
        ],
    )?;
    optional(o, "componentTypes", strings)?;
    optional(o, "listing", listing)?;
    if let Some(status) = o.get("updateStatus")
        && !matches!(
            status.as_str(),
            Some("none" | "update-available" | "version-changed")
        )
    {
        return Err("Malformed plugin update status".into());
    }
    optional(o, "hookDetails", |v| array(v, json_object))
}
fn installed_plugins(v: &Value) -> Check {
    array(v, installed)
}
fn plugin_info(v: &Value) -> Check {
    let o = object(
        v,
        &[
            "id",
            "name",
            "description",
            "version",
            "enabled",
            "source",
            "marketplace",
            "author",
            "authorUrl",
            "homepage",
            "skillCount",
            "skillRootCount",
            "commandRootCount",
            "components",
            "declaredMcpServerNames",
            "hostMcpServerNames",
            "mcpServerNames",
            "hookDetails",
            "rootPath",
            "userConfig",
            "configuredOptions",
            "packageStatus",
            "rootSource",
            "enabledSource",
            "optionSources",
        ],
    )?;
    for key in ["id", "name", "source", "marketplace"] {
        field(o, key, name)?;
    }
    field(o, "enabled", boolean)?;
    field(o, "rootPath", string)?;
    field(o, "skillRootCount", number)?;
    field(o, "commandRootCount", number)?;
    field(o, "mcpServerNames", strings)
}
fn errors(v: &Value) -> Vec<String> {
    let mut messages: Vec<_> = v
        .get("diagnostics")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|d| d["severity"] == "error")
        .filter_map(|d| d["message"].as_str())
        .map(crate::shared::redact::scrub)
        .collect();
    for m in v.get("marketplace").into_iter().chain(
        v.get("marketplaces")
            .and_then(Value::as_array)
            .into_iter()
            .flatten(),
    ) {
        if let Some(message) = m.pointer("/refreshFailure/message").and_then(Value::as_str) {
            messages.push(crate::shared::redact::scrub(message));
        }
    }
    messages.sort();
    messages.dedup();
    messages
}

pub(crate) fn validate_overview(v: &Value) -> Check {
    let o = object(
        v,
        &[
            "marketplaces",
            "availablePlugins",
            "installedPlugins",
            "restorableBuiltins",
            "diagnostics",
            "capability",
        ],
    )?;
    field(o, "marketplaces", marketplaces)?;
    field(o, "availablePlugins", |v| array(v, available))?;
    field(o, "restorableBuiltins", |v| array(v, available))?;
    field(o, "installedPlugins", installed_plugins)?;
    field(o, "diagnostics", diagnostics)?;
    let capability = object(
        o.get("capability").ok_or("Missing plugin capability")?,
        &["supported", "reason"],
    )?;
    field(capability, "supported", boolean)?;
    optional(capability, "reason", string)
}

pub(crate) fn operation_result(method: &str, v: Option<&Value>) -> Result<Vec<String>, String> {
    let v = v.ok_or("Missing plugin operation response")?;
    match method {
        "plugins/marketplace/add" | "plugins/marketplace/update" | "plugins/marketplace/remove" => {
            let o = object(v, &["marketplace", "marketplaces", "diagnostics"])?;
            optional(o, "marketplace", marketplace)?;
            optional(o, "marketplaces", marketplaces)?;
            optional(o, "diagnostics", diagnostics)?;
        }
        "plugins/install" | "plugins/update" => {
            let o = object(v, &["installedPlugins", "dependencyClosure", "diagnostics"])?;
            field(o, "installedPlugins", installed_plugins)?;
            field(o, "dependencyClosure", strings)?;
            field(o, "diagnostics", diagnostics)?;
        }
        "plugins/uninstall" => {
            let o = object(v, &["removedPlugin", "diagnostics"])?;
            optional(o, "removedPlugin", installed)?;
            field(o, "diagnostics", diagnostics)?;
        }
        "plugins/setEnabled" => {
            let o = object(v, &["plugin", "enabled"])?;
            field(o, "plugin", plugin_info)?;
            field(o, "enabled", boolean)?;
        }
        "plugins/restoreBuiltin" => {
            let o = object(v, &["pluginId", "diagnostics"])?;
            field(o, "pluginId", name)?;
            field(o, "diagnostics", diagnostics)?;
        }
        _ => return Err("Unknown plugin operation response".into()),
    }
    Ok(errors(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn operation_results_fail_closed_and_sanitize_nested_errors() {
        assert!(operation_result("plugins/install", Some(&json!({}))).is_err());
        assert!(
            operation_result(
                "plugins/marketplace/add",
                Some(&json!({"diagnostics":null}))
            )
            .is_err()
        );
        assert!(operation_result("plugins/marketplace/remove", Some(&json!({}))).is_ok());
        assert!(
            operation_result(
                "plugins/setEnabled",
                Some(&json!({"enabled":true,"plugin":{}}))
            )
            .is_err()
        );
        let v = json!({"marketplaces":[{"id":"m","name":"M","source":{},"pluginCount":0,"refreshFailure":{"code":"refresh","failedAt":"now","message":"token=sentinel-secret"}}],"diagnostics":[]});
        let failures = operation_result("plugins/marketplace/update", Some(&v)).unwrap();
        assert_eq!(failures.len(), 1);
        assert!(!failures[0].contains("sentinel-secret"));
        assert!(validate_overview(&json!({})).is_err());
    }
}
