use crate::shared::saved_workflows::{SavedWorkflowArg, SavedWorkflowList};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct WorkflowMetadata {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when_to_use: Option<String>,
    #[serde(default)]
    pub args: BTreeMap<String, SavedWorkflowArg>,
}
#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkflowDefinition {
    pub ok: bool,
    pub name: String,
    pub path: String,
    pub scope: String,
    pub meta: WorkflowMetadata,
    pub script: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Failure {
    ok: bool,
    reason: String,
    detail: Option<String>,
}
impl WorkflowDefinition {
    pub(crate) fn parse(v: &Value, scope: &str, name: &str) -> Result<Self, String> {
        if v["ok"] == false {
            let failure: Failure =
                serde_json::from_value(v.clone()).map_err(|_| "Malformed workflow failure")?;
            if failure.ok
                || !matches!(
                    failure.reason.as_str(),
                    "invalid_name" | "not_found" | "parse_error" | "read_error"
                )
                || v.get("detail").is_some_and(|d| !d.is_string())
            {
                return Err("Malformed workflow failure".into());
            }
            return Err(crate::shared::redact::scrub(&format!(
                "{}{}",
                failure.reason,
                failure.detail.map(|d| format!(": {d}")).unwrap_or_default()
            )));
        }
        let definition: Self =
            serde_json::from_value(v.clone()).map_err(|_| "Malformed workflow definition")?;
        if !definition.ok
            || definition.scope != scope
            || definition.name != name
            || definition.path.trim().is_empty()
            || definition.meta.description.is_empty()
            || v["meta"]
                .get("whenToUse")
                .is_some_and(|s| !s.as_str().is_some_and(|s| !s.is_empty()))
        {
            return Err("Invalid workflow definition identity or metadata".into());
        }
        let mut entry = json!({"name":name,"path":definition.path,"scope":scope,"description":definition.meta.description});
        if let Some(args) = v["meta"].get("args") {
            entry["args"] = args.clone();
        }
        SavedWorkflowList::parse(
            &json!({"workflows":[entry],"invalid":[],"dir":"definition"}),
            scope,
        )?;
        Ok(definition)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn definition_checks_identity_metadata_and_failure_shape() {
        let good = json!({"ok":true,"name":"test","scope":"project","path":"/test","meta":{"description":"Test","args":{"j":{"type":"json","required":true,"default":null}}},"script":"return args.j"});
        let parsed = WorkflowDefinition::parse(&good, "project", "test").unwrap();
        assert_eq!(parsed.meta.args["j"].default, Some(Value::Null));
        assert!(WorkflowDefinition::parse(&good, "global", "test").is_err());
        assert!(WorkflowDefinition::parse(&good, "project", "other").is_err());
        let mut bad = good.clone();
        bad["meta"]["whenToUse"] = Value::Null;
        assert!(WorkflowDefinition::parse(&bad, "project", "test").is_err());
        assert!(
            !WorkflowDefinition::parse(
                &json!({"ok":false,"reason":"read_error","detail":"token=secret"}),
                "project",
                "test"
            )
            .err()
            .unwrap()
            .contains("secret")
        );
        assert!(
            WorkflowDefinition::parse(
                &json!({"ok":false,"reason":"not_found","extra":true}),
                "project",
                "test"
            )
            .is_err()
        );
    }
}
