use super::*;
use serde_json::json;

#[test]
fn test_workflows_state_snapshot_deserialization() {
    let snap = json!({
        "revision": 42,
        "runs": [
            {
                "runId": "run-1",
                "status": "running",
                "usage": {
                    "spentTokens": 1200,
                    "nodesUsed": 5
                },
                "lastEventSequence": 10,
                "actors": [
                    {
                        "siteId": "siteA",
                        "ordinal": 0,
                        "name": "explorer",
                        "status": "running"
                    }
                ],
                "nodes": [
                    {
                        "siteId": "siteA",
                        "ordinal": 0,
                        "phase": "executing",
                        "kind": "ask"
                    }
                ]
            }
        ]
    });

    let state = WorkflowRunsState::from_value(&snap);
    assert_eq!(state.revision, 42);
    assert_eq!(state.runs.len(), 1);
    assert_eq!(state.runs[0].run_id, "run-1");
    assert_eq!(state.runs[0].status, "running");
    assert_eq!(state.runs[0].usage.spent_tokens, 1200);
    assert_eq!(state.runs[0].actors.len(), 1);
    assert_eq!(state.runs[0].actors[0].name.as_deref(), Some("explorer"));
    assert_eq!(state.runs[0].nodes.len(), 1);
    assert_eq!(state.runs[0].nodes[0].phase, "executing");
}

#[test]
fn test_delta_reducer_order_invariant() {
    // Test that removals happen BEFORE upserts:
    // If an item is in removedActors AND in actors, it should be removed then added back.
    let mut state = WorkflowRunsState {
        revision: 1,
        runs: vec![WorkflowRunState {
            run_id: "run-1".into(),
            status: "running".into(),
            actors: vec![
                WorkflowRunActor {
                    site_id: "siteA".into(),
                    ordinal: 0,
                    name: Some("initial-name".into()),
                    session_id: None,
                    status: "waiting".into(),
                    phase_name: None,
                },
                WorkflowRunActor {
                    site_id: "siteB".into(),
                    ordinal: 1,
                    name: Some("to-remove".into()),
                    session_id: None,
                    status: "completed".into(),
                    phase_name: None,
                },
            ],
            nodes: vec![WorkflowRunNode {
                site_id: "siteA".into(),
                ordinal: 0,
                kind: Some("ask".into()),
                phase: "queued".into(),
                outcome: None,
                cached: None,
                actor_site_id: None,
                actor_ordinal: None,
                phase_name: None,
                instructions_head: None,
                turn: None,
                tool_calls: None,
                last_tool: None,
            }],
            ..Default::default()
        }],
    };

    let delta = json!({
        "op": "workflowRun.updated",
        "runId": "run-1",
        "revision": 2,
        "run": {
            "status": "completed",
            "lastEventSequence": 15
        },
        "removedActors": [
            { "siteId": "siteB", "ordinal": 1 },
            { "siteId": "siteA", "ordinal": 0 }
        ],
        "actors": [
            {
                "siteId": "siteA",
                "ordinal": 0,
                "name": "reborn-actor",
                "status": "running"
            }
        ],
        "nodes": [
            {
                "siteId": "siteA",
                "ordinal": 0,
                "phase": "settled",
                "outcome": "ok"
            }
        ]
    });

    state.apply_updated(&delta);

    assert_eq!(state.revision, 2);
    let run = &state.runs[0];
    assert_eq!(run.status, "completed");
    assert_eq!(run.last_event_sequence, 15);

    // siteB should be removed
    assert!(!run.actors.iter().any(|a| a.site_id == "siteB"));

    // siteA should exist with "reborn-actor" because it was removed then added back
    assert_eq!(run.actors.len(), 1);
    assert_eq!(run.actors[0].name.as_deref(), Some("reborn-actor"));
    assert_eq!(run.actors[0].status, "running");

    // node siteA should be updated in-place to settled
    assert_eq!(run.nodes.len(), 1);
    assert_eq!(run.nodes[0].phase, "settled");
    assert_eq!(run.nodes[0].outcome.as_deref(), Some("ok"));
}

#[test]
fn test_workflow_run_removed() {
    let mut state = WorkflowRunsState {
        revision: 5,
        runs: vec![
            WorkflowRunState {
                run_id: "run-1".into(),
                ..Default::default()
            },
            WorkflowRunState {
                run_id: "run-2".into(),
                ..Default::default()
            },
        ],
    };

    let delta = json!({
        "op": "workflowRun.removed",
        "runId": "run-1",
        "revision": 6
    });

    state.apply_removed(&delta);
    assert_eq!(state.revision, 6);
    assert_eq!(state.runs.len(), 1);
    assert_eq!(state.runs[0].run_id, "run-2");
}

fn actor(site: &str, ordinal: u64) -> serde_json::Value {
    json!({ "siteId": site, "ordinal": ordinal, "status": "waiting" })
}

/// delta.ts: an entry removed and re-added in one op lands at the table
/// tail, matching sequential application of two ops.
#[test]
fn removed_then_readded_entry_moves_to_tail() {
    let mut state = WorkflowRunsState::from_value(&json!({
        "revision": 1,
        "runs": [{ "runId": "r", "status": "running", "usage": {},
                   "actors": [actor("A", 0), actor("B", 0), actor("C", 0)] }]
    }));
    state.apply_updated(&json!({
        "op": "workflowRun.updated", "runId": "r", "revision": 2,
        "removedActors": [{ "siteId": "A", "ordinal": 0 }],
        "actors": [actor("A", 0)]
    }));
    let order: Vec<_> = state.runs[0]
        .actors
        .iter()
        .map(|a| a.site_id.as_str())
        .collect();
    assert_eq!(order, vec!["B", "C", "A"]);
}

#[test]
fn unknown_run_with_partial_header_is_noop_but_revision_advances() {
    let mut state = WorkflowRunsState::default();
    state.apply_updated(&json!({
        "op": "workflowRun.updated", "runId": "ghost", "revision": 7,
        "run": { "status": "running" }
    }));
    assert!(state.runs.is_empty());
    assert_eq!(state.revision, 7);
    // Removing an unknown run is also a no-op that only advances revision.
    state.apply_removed(&json!({ "op": "workflowRun.removed", "runId": "x", "revision": 9 }));
    assert_eq!(state.revision, 9);
}

/// Closed enums break whole frames on the TS side; ours must keep parsing.
#[test]
fn unknown_enum_values_survive_parsing() {
    let mut state = WorkflowRunsState::from_value(&json!({
        "revision": 1,
        "runs": [{ "runId": "r", "status": "hibernating", "usage": {},
                   "actors": [{ "siteId": "A", "ordinal": 0, "status": "teleporting" }],
                   "nodes": [{ "siteId": "A", "ordinal": 0, "phase": "quantum", "kind": "newKind" }] }]
    }));
    assert_eq!(state.runs[0].status, "hibernating");
    assert_eq!(state.runs[0].actors[0].status, "teleporting");
    assert_eq!(state.runs[0].nodes[0].phase, "quantum");
    state.apply_updated(&json!({
        "op": "workflowRun.updated", "runId": "r", "revision": 2,
        "nodes": [{ "siteId": "A", "ordinal": 0, "phase": "futurePhase" }]
    }));
    assert_eq!(state.runs[0].nodes[0].phase, "futurePhase");
}

#[test]
fn conversation_snapshot_with_unknown_enums_still_applies() {
    let mut c = crate::conversation::model::ConversationState::default();
    c.apply_snapshot(&json!({
        "logEpoch": "e1", "seq": 3, "revision": 4,
        "control": { "phase": "someNewPhase" },
        "backgroundWorks": [{ "workId": "w", "kind": "unknownKind" }],
        "rows": { "firstRowId": 0, "totalCount": 1, "window": [
            { "rowId": 0, "kind": "turnHeader", "origin": "brandNewOrigin" },
            { "rowId": 1, "kind": "futureRowKind" }
        ]},
        "workflowRuns": { "revision": 1, "runs": [{ "runId": "r", "status": "weird", "usage": {} }] }
    }));
    assert_eq!(c.revision, 4);
    assert_eq!(c.log_epoch.as_deref(), Some("e1"));
    assert_eq!(c.phase, "someNewPhase");
    assert!(!c.phase_running());
}
