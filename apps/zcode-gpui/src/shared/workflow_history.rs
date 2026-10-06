use serde::{Deserialize, Deserializer};
use serde_json::Value;

fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct WorkflowHistoryRun {
    pub run_id: String,
    #[serde(default, deserialize_with = "present")]
    pub name: Option<String>,
    pub status: String,
    #[serde(default, deserialize_with = "present")]
    pub stop_reason: Option<String>,
    pub created_at: f64,
    pub updated_at: f64,
    pub spent_tokens: f64,
    #[serde(default, deserialize_with = "present")]
    pub parent_session_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub tool_call_id: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub args: Option<std::collections::BTreeMap<String, Value>>,
    #[serde(default, deserialize_with = "present")]
    pub cwd: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub artifacts: Option<Vec<WorkflowHistoryArtifact>>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct WorkflowHistoryArtifact {
    pub id: String,
    pub kind: String,
    #[serde(default, deserialize_with = "present")]
    pub title: Option<String>,
    pub version: f64,
    #[serde(default, deserialize_with = "present")]
    pub content_type: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkflowHistory {
    pub runs: Vec<WorkflowHistoryRun>,
    #[serde(default, deserialize_with = "present")]
    pub truncated: Option<bool>,
}
impl WorkflowHistory {
    pub(crate) fn parse(v: &Value, name: &str) -> Result<Self, String> {
        let history: Self =
            serde_json::from_value(v.clone()).map_err(|_| "Malformed workflow history")?;
        if history.truncated == Some(false) || history.runs.len() > 50 {
            return Err("Invalid workflow history page".into());
        }
        let mut ids = std::collections::HashSet::new();
        for run in &history.runs {
            if run.run_id.is_empty()
                || !ids.insert(&run.run_id)
                || run.name.as_deref().is_some_and(|n| n != name)
                || !matches!(
                    run.status.as_str(),
                    "pending" | "running" | "completed" | "errored" | "stopped"
                )
                || run.stop_reason.as_deref().is_some_and(|s| {
                    !matches!(
                        s,
                        "user" | "model" | "provider" | "interrupted" | "superseded"
                    )
                })
                || ![run.created_at, run.updated_at, run.spent_tokens]
                    .iter()
                    .all(|n| n.is_finite())
                || run.artifacts.as_ref().is_some_and(|a| {
                    a.len() > 8
                        || a.iter().any(|a| {
                            a.id.is_empty()
                                || !a.version.is_finite()
                                || !matches!(
                                    a.kind.as_str(),
                                    "file" | "markdown" | "chart" | "table" | "metrics" | "board"
                                )
                        })
                })
            {
                return Err("Invalid workflow history entry".into());
            }
        }
        Ok(history)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn history_rejects_wrong_identity_unknown_status_and_optional_null() {
        let good = json!({"runs":[{"runId":"run","name":"test","status":"completed","createdAt":1,"updatedAt":2,"spentTokens":0}],"truncated":true});
        assert!(WorkflowHistory::parse(&good, "test").is_ok());
        assert!(WorkflowHistory::parse(&good, "other").is_err());
        for (field, value) in [
            ("status", json!("unknown")),
            ("parentSessionId", Value::Null),
        ] {
            let mut bad = good.clone();
            bad["runs"][0][field] = value;
            assert!(WorkflowHistory::parse(&bad, "test").is_err());
        }
        assert!(WorkflowHistory::parse(&json!({"runs":[],"truncated":false}), "test").is_err());
    }
}
