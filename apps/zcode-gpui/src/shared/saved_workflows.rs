use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct SavedWorkflowArg {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub description: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub required: Option<bool>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub default: Option<Value>,
}
#[derive(Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct SavedWorkflowEntry {
    pub name: String,
    pub description: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub when_to_use: Option<String>,
    #[serde(default)]
    pub args: BTreeMap<String, SavedWorkflowArg>,
    pub scope: String,
    pub path: String,
}
#[derive(Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct InvalidWorkflow {
    pub path: String,
    pub reason: String,
}
#[derive(Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedWorkflowList {
    pub workflows: Vec<SavedWorkflowEntry>,
    pub invalid: Vec<InvalidWorkflow>,
    pub dir: String,
}
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    // JSON default:null 是已声明默认值；Option 的默认解码会把它误当作缺失字段。
    T::deserialize(deserializer).map(Some)
}
impl SavedWorkflowList {
    pub(crate) fn parse(v: &Value, scope: &str) -> Result<Self, String> {
        let mut list: Self =
            serde_json::from_value(v.clone()).map_err(|_| "Malformed saved workflow list")?;
        if list.dir.is_empty()
            || list
                .workflows
                .iter()
                .any(|w| w.name.is_empty() || w.path.is_empty() || w.scope != scope)
            || list
                .invalid
                .iter()
                .any(|i| i.path.is_empty() || i.reason.is_empty())
        {
            return Err("Invalid saved workflow list".into());
        }
        let mut names = std::collections::HashSet::new();
        for w in &list.workflows {
            if !names.insert(&w.name) {
                return Err("Duplicate saved workflow identity".into());
            }
            for arg in w.args.values() {
                if !matches!(arg.kind.as_str(), "string" | "number" | "boolean" | "json") {
                    return Err("Invalid workflow argument type".into());
                }
                if let Some(default) = &arg.default {
                    check_type(&arg.kind, default)?;
                }
            }
        }
        for invalid in &mut list.invalid {
            invalid.reason = crate::shared::redact::scrub(&invalid.reason);
        }
        Ok(list)
    }
}
fn check_type(kind: &str, value: &Value) -> Result<(), String> {
    let valid = match kind {
        "string" => value.is_string(),
        "number" => value.as_f64().is_some_and(f64::is_finite),
        "boolean" => value.is_boolean(),
        "json" => true,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err("Workflow argument type mismatch".into())
    }
}
pub(crate) fn parse_args(
    entry: &SavedWorkflowEntry,
    input: &BTreeMap<String, String>,
) -> Result<Value, String> {
    if input.keys().any(|k| !entry.args.contains_key(k)) {
        return Err("Unknown workflow argument".into());
    }
    let mut result = Map::new();
    for (key, arg) in &entry.args {
        let text = input.get(key).map(String::as_str).unwrap_or("");
        if text.trim().is_empty() && arg.kind != "string" || text.is_empty() {
            if arg.required == Some(true) && arg.default.is_none() {
                return Err(format!("Required argument: {key}"));
            }
            continue;
        }
        let value = match arg.kind.as_str() {
            "string" => Value::String(text.into()),
            "number" | "boolean" | "json" => serde_json::from_str(text)
                .map_err(|_| format!("Invalid {} argument: {key}", arg.kind))?,
            _ => return Err("Invalid workflow argument type".into()),
        };
        check_type(&arg.kind, &value)
            .map_err(|_| format!("Invalid {} argument: {key}", arg.kind))?;
        result.insert(key.clone(), value);
    }
    Ok(Value::Object(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn explicit_json_null_default_is_present_and_other_null_declarations_fail() {
        let value = json!({"workflows":[{"name":"test","description":"","scope":"project","path":"/test","args":{"j":{"type":"json","required":true,"default":null}}}],"invalid":[],"dir":"/dir"});
        let list = SavedWorkflowList::parse(&value, "project").unwrap();
        assert_eq!(list.workflows[0].args["j"].default, Some(Value::Null));
        assert_eq!(
            parse_args(&list.workflows[0], &BTreeMap::new()).unwrap(),
            json!({})
        );
        for (field, null) in [
            ("required", Value::Null),
            ("description", Value::Null),
            ("default", json!(true)),
        ] {
            let mut malformed = value.clone();
            malformed["workflows"][0]["args"]["j"][field] = null;
            if field == "default" {
                malformed["workflows"][0]["args"]["j"]["type"] = json!("string");
            }
            assert!(SavedWorkflowList::parse(&malformed, "project").is_err());
        }
    }
    #[test]
    fn typed_arguments_omit_defaults_and_reject_unknown_or_malformed_values() {
        let list = SavedWorkflowList::parse(&json!({"workflows":[{"name":"test","description":"","scope":"project","path":"/test","args":{"n":{"type":"number","required":true},"b":{"type":"boolean","default":false},"j":{"type":"json"},"s":{"type":"string"}}}],"invalid":[],"dir":"/dir"}), "project").unwrap();
        let entry = &list.workflows[0];
        assert!(parse_args(entry, &BTreeMap::new()).is_err());
        let mut inputs = BTreeMap::from([
            ("n".into(), "4".into()),
            ("j".into(), "null".into()),
            ("s".into(), " value ".into()),
        ]);
        assert_eq!(
            parse_args(entry, &inputs).unwrap(),
            json!({"n":4,"j":null,"s":" value "})
        );
        inputs.insert("b".into(), "yes".into());
        assert!(parse_args(entry, &inputs).is_err());
        inputs.insert("b".into(), "true".into());
        assert_eq!(parse_args(entry, &inputs).unwrap()["b"], true);
        inputs.insert("n".into(), "1e9999".into());
        assert!(parse_args(entry, &inputs).is_err());
        assert!(
            SavedWorkflowList::parse(&json!({"workflows":[],"invalid":[],"dir":"/dir"}), "global")
                .is_ok()
        );
    }
}
