use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginComponentItem {
    pub name: String,
    pub description: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginComponentGroup {
    pub kind: String,
    pub items: Vec<PluginComponentItem>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct PluginDescriptionMetadata {
    pub author: Option<String>,
    pub author_url: Option<String>,
    pub homepage: Option<String>,
    pub version: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginDescriptionDiagnostic {
    code: String,
    message: String,
    severity: Option<String>,
    #[serde(rename = "pluginId")]
    plugin_id: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginDescription {
    pub components: Vec<PluginComponentGroup>,
    pub diagnostics: Option<Vec<PluginDescriptionDiagnostic>>,
    pub metadata: Option<PluginDescriptionMetadata>,
}
impl PluginDescription {
    pub(crate) fn parse(v: &Value) -> Result<Self, String> {
        let result: Self =
            serde_json::from_value(v.clone()).map_err(|_| "Malformed plugin description")?;
        if ["diagnostics", "metadata"]
            .iter()
            .any(|k| v.get(k).is_some_and(Value::is_null))
        {
            return Err("Malformed plugin description".into());
        }
        for group in &result.components {
            if !matches!(
                group.kind.as_str(),
                "agent" | "command" | "skill" | "hook" | "mcp"
            ) {
                return Err("Unknown plugin component kind".into());
            }
        }
        for group in v["components"].as_array().unwrap() {
            for item in group["items"].as_array().unwrap() {
                if item.get("description").is_some_and(|v| !v.is_string()) {
                    return Err("Malformed plugin item description".into());
                }
            }
        }
        if let Some(meta) = v.get("metadata")
            && meta.as_object().unwrap().values().any(|v| !v.is_string())
        {
            return Err("Malformed plugin metadata".into());
        }
        if let Some(diagnostics) = &result.diagnostics {
            for d in diagnostics {
                if d.severity
                    .as_deref()
                    .is_some_and(|s| !matches!(s, "error" | "warning"))
                {
                    return Err("Malformed plugin diagnostic".into());
                }
                let _ = (&d.code, &d.plugin_id);
            }
        }
        if let Some(diagnostics) = v.get("diagnostics").and_then(Value::as_array)
            && diagnostics.iter().any(|d| {
                ["pluginId", "severity"]
                    .iter()
                    .any(|k| d.get(k).is_some_and(|v| !v.is_string()))
            })
        {
            return Err("Malformed plugin diagnostic".into());
        }
        if let Some(error) = result
            .diagnostics
            .as_ref()
            .into_iter()
            .flatten()
            .find(|d| d.severity.as_deref() == Some("error"))
        {
            return Err(crate::shared::redact::scrub(&error.message));
        }
        Ok(result)
    }
    pub(crate) fn feedback(&self) -> Vec<String> {
        self.diagnostics
            .as_ref()
            .into_iter()
            .flatten()
            .map(|d| crate::shared::redact::scrub(&d.message))
            .collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn descriptions_are_strict_and_feedback_redacted() {
        let good = json!({"components":[{"kind":"skill","items":[{"name":"Test","description":"Fixture"}]}],"diagnostics":[{"code":"error","message":"token=secret","severity":"warning"}],"metadata":{"version":"1"}});
        let parsed = PluginDescription::parse(&good).unwrap();
        assert!(!parsed.feedback()[0].contains("secret"));
        let mut failure = good.clone();
        failure["diagnostics"][0]["severity"] = json!("error");
        assert!(
            !PluginDescription::parse(&failure)
                .err()
                .unwrap()
                .contains("secret")
        );
        assert!(PluginDescription::parse(&json!({"components":[],"metadata":null})).is_err());
        assert!(
            PluginDescription::parse(&json!({"components":[{"kind":"unknown","items":[]}]}))
                .is_err()
        );
        assert!(
            PluginDescription::parse(
                &json!({"components":[],"diagnostics":[{"code":"x","message":"x","severity":null}]})
            )
            .is_err()
        );
        assert!(PluginDescription::parse(&json!({"components":[],"extra":true})).is_err());
    }
}
