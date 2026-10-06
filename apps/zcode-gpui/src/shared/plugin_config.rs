use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ConfigScope {
    User,
    Workspace,
}
impl ConfigScope {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Workspace => "workspace",
        }
    }
}
#[derive(Clone, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OptionKind {
    String,
    Number,
    Boolean,
    Directory,
    File,
}
#[derive(Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OptionDeclaration {
    pub default: Option<Value>,
    pub description: Option<String>,
    pub required: Option<bool>,
    pub sensitive: Option<bool>,
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<OptionKind>,
}
#[derive(Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ConfigPlugin {
    pub id: String,
    pub name: String,
    pub marketplace: String,
    pub enabled: bool,
    pub user_config: Option<BTreeMap<String, OptionDeclaration>>,
    pub configured_options: Option<BTreeMap<String, Value>>,
    pub option_sources: Option<BTreeMap<String, ConfigScope>>,
    pub enabled_source: Option<ConfigScope>,
    #[serde(rename = "description")]
    _description: Option<String>,
    #[serde(rename = "version")]
    _version: Option<String>,
    #[serde(rename = "source")]
    _source: String,
    #[serde(rename = "author")]
    _author: Option<String>,
    #[serde(rename = "authorUrl")]
    _author_url: Option<String>,
    #[serde(rename = "homepage")]
    _homepage: Option<String>,
    #[serde(rename = "skillCount")]
    _skill_count: Option<u64>,
    #[serde(rename = "skillRootCount")]
    _skill_root_count: u64,
    #[serde(rename = "commandRootCount")]
    _command_root_count: u64,
    #[serde(rename = "components")]
    _components: Option<Value>,
    #[serde(rename = "declaredMcpServerNames")]
    _declared_mcp: Option<Vec<String>>,
    #[serde(rename = "hostMcpServerNames")]
    _host_mcp: Option<Vec<String>>,
    #[serde(rename = "mcpServerNames")]
    _mcp: Vec<String>,
    #[serde(rename = "hookDetails")]
    _hooks: Option<Vec<Value>>,
    #[serde(rename = "rootPath")]
    _root: String,
    #[serde(rename = "packageStatus")]
    _package_status: Option<String>,
    #[serde(rename = "rootSource")]
    _root_source: Option<ConfigScope>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PluginConfigList {
    pub plugins: Vec<ConfigPlugin>,
    diagnostics: Vec<ConfigDiagnostic>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ConfigDiagnostic {
    code: String,
    message: String,
    severity: Option<String>,
    plugin_id: Option<String>,
}
fn diagnostics(values: &[ConfigDiagnostic]) -> Result<(), String> {
    for d in values {
        let _ = (&d.code, &d.plugin_id, &d.message);
        if d.severity
            .as_deref()
            .is_some_and(|v| !matches!(v, "warning" | "error"))
        {
            return Err("Malformed plugin diagnostic".into());
        }
        if d.severity.as_deref() == Some("error") {
            return Err("Plugin configuration reported an error; Reload before writing".into());
        }
    }
    Ok(())
}
fn scalar(value: &Value) -> bool {
    value.is_string() || value.is_number() || value.is_boolean()
}
impl PluginConfigList {
    pub(crate) fn parse(v: &Value) -> Result<Self, String> {
        let parsed: Self =
            serde_json::from_value(v.clone()).map_err(|_| "Malformed scoped plugin list")?;
        if v["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d.as_object().unwrap().values().any(Value::is_null))
        {
            return Err("Malformed plugin diagnostic".into());
        }
        diagnostics(&parsed.diagnostics)?;
        for (raw, plugin) in v["plugins"].as_array().unwrap().iter().zip(&parsed.plugins) {
            if raw.as_object().unwrap().values().any(Value::is_null)
                || plugin.id.trim().is_empty()
                || plugin.name.trim().is_empty()
                || plugin.marketplace.trim().is_empty()
                || plugin
                    ._package_status
                    .as_deref()
                    .is_some_and(|v| v != "missing")
            {
                return Err("Malformed scoped plugin info".into());
            }
            if let Some(options) = &plugin.user_config {
                for (key, declaration) in options {
                    if raw["userConfig"][key]
                        .as_object()
                        .unwrap()
                        .values()
                        .any(Value::is_null)
                        || declaration.default.as_ref().is_some_and(|v| !scalar(v))
                    {
                        return Err("Malformed plugin option declaration".into());
                    }
                }
            }
            if let Some(options) = &plugin.configured_options {
                for (key, value) in options {
                    if !scalar(value)
                        || plugin
                            .declaration(key)
                            .is_some_and(|d| d.sensitive == Some(true))
                    {
                        return Err(
                            "Invalid scoped plugin options; sensitive readback is not supported"
                                .into(),
                        );
                    }
                }
            }
            if let Some(components) = &plugin._components {
                crate::shared::plugin_description::PluginDescription::parse(
                    &json!({"components": components}),
                )?;
            }
        }
        Ok(parsed)
    }
}
#[derive(Clone, PartialEq)]
pub(crate) enum ConfigEdit {
    Text(String),
    Bool(bool),
    Clear,
}
pub(crate) struct ConfigPatch {
    pub options: Value,
    pub clear_keys: Vec<String>,
}
impl ConfigPatch {
    pub(crate) fn is_empty(&self) -> bool {
        self.options.as_object().unwrap().is_empty() && self.clear_keys.is_empty()
    }
}
impl ConfigPlugin {
    pub(crate) fn declaration(&self, key: &str) -> Option<&OptionDeclaration> {
        self.user_config.as_ref()?.get(key)
    }
    pub(crate) fn effective(&self, key: &str) -> Value {
        let Some(d) = self.declaration(key) else {
            return Value::Null;
        };
        if d.sensitive == Some(true) {
            return json!("");
        }
        self.configured_options
            .as_ref()
            .and_then(|o| o.get(key))
            .cloned()
            .or_else(|| d.default.clone())
            .unwrap_or_else(|| {
                if d.kind == Some(OptionKind::Boolean) {
                    json!(false)
                } else {
                    json!("")
                }
            })
    }
    pub(crate) fn patch(
        &self,
        edits: &BTreeMap<String, ConfigEdit>,
    ) -> Result<ConfigPatch, String> {
        let mut options = serde_json::Map::new();
        let mut clear_keys = Vec::new();
        for (key, edit) in edits {
            let d = self
                .declaration(key)
                .ok_or("Plugin declarations changed; Reload")?;
            let value = match edit {
                ConfigEdit::Clear => {
                    clear_keys.push(key.clone());
                    continue;
                }
                ConfigEdit::Bool(v) if d.kind == Some(OptionKind::Boolean) => json!(v),
                ConfigEdit::Text(v) if d.sensitive == Some(true) && v.is_empty() => continue,
                ConfigEdit::Text(v)
                    if d.kind == Some(OptionKind::Boolean) && d.sensitive == Some(true) =>
                {
                    json!(
                        v.trim()
                            .parse::<bool>()
                            .map_err(|_| "Boolean options require true or false")?
                    )
                }
                ConfigEdit::Text(v) if d.kind == Some(OptionKind::Number) => {
                    let n = v
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|n| n.is_finite())
                        .ok_or("Number options require a finite value")?;
                    json!(n)
                }
                ConfigEdit::Text(v) if d.kind != Some(OptionKind::Boolean) => json!(v),
                _ => return Err("Invalid plugin option type".into()),
            };
            options.insert(key.clone(), value);
        }
        Ok(ConfigPatch {
            options: Value::Object(options),
            clear_keys,
        })
    }
}
pub(crate) fn validate_mutation(v: &Value, plugin: &str) -> Result<(), String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    struct Receipt {
        plugin_id: String,
        diagnostics: Vec<ConfigDiagnostic>,
    }
    let parsed: Receipt =
        serde_json::from_value(v.clone()).map_err(|_| "Malformed plugin configuration receipt")?;
    if parsed.plugin_id != plugin {
        return Err("Plugin configuration receipt identity mismatch".into());
    }
    if v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d.as_object().unwrap().values().any(Value::is_null))
    {
        return Err("Malformed plugin diagnostic".into());
    }
    diagnostics(&parsed.diagnostics)
}
#[cfg(test)]
#[path = "plugin_config_tests.rs"]
pub(crate) mod tests;
