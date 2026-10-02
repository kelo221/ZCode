//! Golden tests for `model.rs` (kept out-of-line for the 400-line cap).

use super::*;
use crate::catalog::{ModelOption, WorkspaceConfig};
use serde_json::json;

#[test]
fn parses_conversation_snapshot() {
    let mut c = ConversationState::default();
    let snap = json!({
        "logEpoch": "epoch-1", "seq": 5, "revision": 3,
        "control": { "phase": "running", "lastError": null, "apiRetry": null },
        "config": { "provider": "p1", "model": "m1", "thought": "high",
                    "thoughtLevels": ["low","high"], "mode": "build" },
        "rows": { "firstRowId": 40, "totalCount": 50, "window": [
            { "rowId": 40, "kind": "userInput", "text": "hi", "origin": "realUser" },
            { "rowId": 41, "kind": "assistantText", "text": "hello", "state": "streaming" }
        ]}
    });
    c.apply_snapshot(&snap);
    assert_eq!(c.revision, 3);
    assert_eq!(c.first_row_id, 40);
    assert_eq!(c.total_count, 50);
    assert!(c.has_more_history());
    assert_eq!(c.config.model, "m1");
    assert_eq!(
        c.config.thought_levels,
        vec!["low".to_string(), "high".to_string()]
    );
    assert!(c.phase_running());
    assert_eq!(c.rows.len(), 2);
}

#[test]
fn applies_chat_deltas() {
    let mut c = ConversationState::default();
    c.apply_deltas(&[json!({ "op": "row.appended", "row": {
        "rowId": 2, "kind": "assistantText", "text": "Hel", "state": "streaming" } })]);
    c.apply_deltas(&[json!({ "op": "row.delta", "rowId": 2, "path": "text", "append": "lo" })]);
    assert_eq!(
        c.rows[&2].clone(),
        Row::AssistantText {
            row_id: 2,
            entity_id: "".into(),
            text: "Hello".into(),
            state: "streaming".into(),
            can_retry: false,
        }
    );
    // Non-text paths (tool input streaming) must not touch chat text.
    c.apply_deltas(&[json!({ "op": "row.delta", "rowId": 2, "path": "inputText", "append": "X" })]);
    match &c.rows[&2] {
        Row::AssistantText { text, .. } => assert_eq!(text, "Hello"),
        other => panic!("wrong row: {other:?}"),
    }
    c.apply_deltas(&[
        json!({ "op": "row.upserted", "row": { "rowId": 2, "kind": "assistantText",
                 "text": "Hello world", "state": "complete" } }),
        json!({ "op": "state.updated", "patch": { "control": { "phase": "completedSuccess" },
                 "config": { "mode": "plan" } } }),
    ]);
    assert_eq!(c.phase, "completedSuccess");
    assert!(!c.phase_running());
    assert_eq!(c.config.mode, "plan");
    c.apply_deltas(&[json!({ "op": "row.removed", "rowId": 2 })]);
    assert!(!c.rows.contains_key(&2));
}

#[test]
fn surfaces_backend_errors() {
    let mut c = ConversationState::default();
    c.apply_control(&json!({ "phase": "error",
        "lastError": { "code": "model_request_failed", "message": "Model request failed." } }));
    assert_eq!(
        c.last_error,
        Some((
            "model_request_failed".into(),
            "Model request failed.".into()
        ))
    );
    c.apply_control(&json!({ "phase": "running", "lastError": null,
                             "apiRetry": { "attempt": 2 } }));
    assert!(c.last_error.is_none());
    assert_eq!(c.api_retry.unwrap()["attempt"], json!(2));
}

#[test]
fn merges_older_history_pages() {
    let mut c = ConversationState::default();
    c.apply_snapshot(&json!({ "rows": { "firstRowId": 40, "totalCount": 50,
        "window": [ { "rowId": 40, "kind": "userInput", "text": "newest" } ] } }));
    c.apply_older_rows(&[
        json!({ "rowId": 38, "kind": "userInput", "text": "older" }),
        json!({ "rowId": 39, "kind": "userInput", "text": "between" }),
    ]);
    assert_eq!(c.first_row_id, 38);
    assert_eq!(c.rows.len(), 3);
    // rowId ordering is guaranteed by the BTreeMap.
    let ids: Vec<u64> = c.rows.keys().copied().collect();
    assert_eq!(ids, vec![38, 39, 40]);
}

#[test]
fn parses_session_index_entries() {
    let e = SessionEntry::from_value(&json!({
        "sessionId": "s1", "title": "Fix bug", "phase": "completedSuccess",
        "sessionEnded": true, "lastActivityAt": 1000, "lastAssistantPreview": "done"
    }))
    .unwrap();
    assert_eq!(e.title, "Fix bug");
    assert_eq!(e.last_activity_at, Some(1000));
}

#[test]
fn workspace_config_never_clobbers_models_with_empty() {
    let mut cfg = WorkspaceConfig::default();
    cfg.apply_settings(&json!({ "model": { "available": [
        { "ref": { "providerId": "prov-uuid", "modelId": "glm-x" }, "label": "GLM-X",
          "providerLabel": "Zai",
          "reasoning": { "levels": [ { "value": "high", "label": "High" } ],
                         "defaultLevel": "high" } }
    ] } }));
    assert_eq!(cfg.models.len(), 1);
    // An empty workspace-config catalog must not wipe the harvested list.
    cfg.apply_state(&json!({ "configOptions": [] }));
    assert_eq!(cfg.models.len(), 1);
    // A filled catalog replaces it.
    cfg.apply_state(&json!({ "configOptions": [
        { "id": "model", "type": "select", "currentValue": "p/m$high",
          "options": [ { "value": "p/m$high", "name": "Model M",
                         "modelProviderId": "p", "modelProviderName": "P",
                         "modelThoughtLevels": ["low","high"] } ] },
        { "id": "mode", "type": "select", "currentValue": "yolo",
          "options": [] }
    ] }));
    assert_eq!(cfg.models.len(), 1);
    assert_eq!(cfg.models[0].provider, "p");
    assert_eq!(cfg.models[0].default_thought, "high");
    assert_eq!(cfg.default_mode, "yolo");
}

#[test]
fn parses_model_values_with_and_without_thought() {
    let o = ModelOption::from_value(&json!({
        "value": "prov/m1$high", "name": "M1", "modelProviderName": "Prov" }))
    .unwrap();
    assert_eq!(o.provider, "prov");
    assert_eq!(o.model, "m1");
    assert_eq!(o.default_thought, "high");
    let o = ModelOption::from_value(&json!({ "value": "prov/m2", "name": "M2" })).unwrap();
    assert_eq!(o.default_thought, "");
    // Registry refs from the legacy settings snapshot use uuid providers.
    let o = ModelOption::from_settings(&json!({
        "ref": { "providerId": "05c1eaa8-0000", "modelId": "grok-4.6" },
        "label": "grok-4.6", "providerLabel": "Grok",
        "reasoning": { "levels": [ { "value": "high", "label": "high" } ],
                       "defaultLevel": "high" } }))
    .unwrap();
    assert_eq!(o.provider, "05c1eaa8-0000");
    assert_eq!(o.model, "grok-4.6");
    assert_eq!(o.thought_levels, vec!["high".to_string()]);
    assert_eq!(o.value, "05c1eaa8-0000/grok-4.6");
}
