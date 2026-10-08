use super::{
    profile_recovery::observed,
    profile_state::ProfileMutation,
    subagent_profiles::{AgentSummary, AgentsListResult},
};
use serde_json::json;

fn row(name: &str) -> AgentSummary {
    serde_json::from_value(json!({"id":format!("user:{name}"),"name":name,"description":"d","systemPrompt":"p","path":format!("C:/scratch/{name}.md"),"scope":"user","source":"user","enabled":true})).unwrap()
}
fn inventory(rows: Vec<AgentSummary>) -> AgentsListResult {
    serde_json::from_value(json!({"agents":rows,"userAgents":[],"pluginAgents":[],"capability":{"userScopeAvailable":true}})).unwrap()
}
#[test]
fn rename_requires_new_supported_state_and_old_identity_absence() {
    let old = row("old");
    let new = row("new");
    let m = ProfileMutation {
        origin: Default::default(),
        receipt: "r".into(),
        scope: "user".into(),
        method: "updateAgent".into(),
        params: json!({"config":new.config}),
        baseline: Some(old.clone()),
    };
    assert!(observed(&m, &inventory(vec![new.clone()])));
    assert!(!observed(&m, &inventory(vec![old, new.clone()])));
    let mut partial = new;
    partial.config.system_prompt = "partial".into();
    assert!(!observed(&m, &inventory(vec![partial])));
}
#[test]
fn toggle_delete_and_clear_compare_fresh_state_without_rpc_receipt() {
    let old = row("a");
    let mut m = ProfileMutation {
        origin: Default::default(),
        receipt: "r".into(),
        scope: "user".into(),
        method: "setEnabled".into(),
        params: json!({"enabled":false}),
        baseline: Some(old.clone()),
    };
    assert!(!observed(&m, &inventory(vec![old.clone()])));
    let mut changed = old.clone();
    changed.enabled = false;
    assert!(observed(&m, &inventory(vec![changed])));
    m.method = "deleteAgent".into();
    assert!(!observed(&m, &inventory(vec![old.clone()])));
    assert!(observed(&m, &inventory(vec![])));
    m.method = "setBuiltInModelOverride".into();
    m.params = json!({});
    assert!(observed(&m, &inventory(vec![old])));
}
