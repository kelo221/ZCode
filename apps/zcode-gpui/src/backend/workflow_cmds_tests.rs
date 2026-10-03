use super::*;
use serde_json::json;

#[test]
fn test_start_saved_workflow_payload() {
    let p =
        start_saved_workflow_payload("test-flow", Some("project"), Some(json!({ "arg1": "val" })));
    assert_eq!(p["name"], "test-flow");
    assert_eq!(p["scope"], "project");
    assert_eq!(p["args"]["arg1"], "val");

    let p_simple = start_saved_workflow_payload("simple-flow", None, None);
    assert_eq!(p_simple["name"], "simple-flow");
    assert!(p_simple.get("scope").is_none());
    assert!(p_simple.get("args").is_none());
}

#[test]
fn test_resume_workflow_run_payload() {
    let p = resume_workflow_run_payload("run-123", Some("Resume label"));
    assert_eq!(p["workId"], "run-123");
    assert_eq!(p["name"], "Resume label");

    let p_no_name = resume_workflow_run_payload("run-456", None);
    assert_eq!(p_no_name["workId"], "run-456");
    assert!(p_no_name.get("name").is_none());
}

#[test]
fn test_amend_workflow_run_settings_payload() {
    // Model set, concurrency set
    let p = amend_workflow_run_settings_payload(
        "run-123",
        Some(Some("openai/gpt-4o".into())),
        Some(Some(4)),
    );
    assert_eq!(p["workId"], "run-123");
    assert_eq!(p["subagentModel"], "openai/gpt-4o");
    assert_eq!(p["maxConcurrency"], 4);

    // Reset to defaults: null
    let p_null = amend_workflow_run_settings_payload("run-123", Some(None), Some(None));
    assert_eq!(p_null["workId"], "run-123");
    assert_eq!(p_null["subagentModel"], Value::Null);
    assert_eq!(p_null["maxConcurrency"], Value::Null);

    // Unchanged: omitted
    let p_omitted = amend_workflow_run_settings_payload("run-123", None, None);
    assert_eq!(p_omitted["workId"], "run-123");
    assert!(p_omitted.get("subagentModel").is_none());
    assert!(p_omitted.get("maxConcurrency").is_none());
}
