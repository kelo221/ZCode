use super::*;
use serde_json::json;

#[test]
fn parses_subagents_state_snapshot() {
    let data = json!({
        "revision": 42,
        "childSessionIds": ["child-1", "child-2"],
        "running": [
            {
                "childSessionId": "child-1",
                "agentId": "agent-a",
                "toolCallId": "call-1",
                "subagentType": "explore",
                "title": "Exploring files",
                "summary": "found 5 files",
                "status": "running",
                "startedAt": 1000
            }
        ],
        "endedTotal": 3
    });

    let state = SubagentsState::from_value(&data).expect("should parse");
    assert_eq!(state.revision, 42);
    assert_eq!(state.child_session_ids, vec!["child-1", "child-2"]);
    assert_eq!(state.ended_total, 3);
    assert_eq!(state.running.len(), 1);

    let sub = &state.running[0];
    assert_eq!(sub.child_session_id, "child-1");
    assert_eq!(sub.agent_id.as_deref(), Some("agent-a"));
    assert_eq!(sub.tool_call_id.as_deref(), Some("call-1"));
    assert_eq!(sub.subagent_type, "explore");
    assert_eq!(sub.title, "Exploring files");
    assert_eq!(sub.summary.as_deref(), Some("found 5 files"));
    assert_eq!(sub.status, "running");
    assert_eq!(sub.started_at, Some(1000));
}

#[test]
fn parses_unknown_enum_values_gracefully() {
    let sub_val = json!({
        "childSessionId": "child-unk",
        "subagentType": "custom-agent-type",
        "title": "Custom Agent",
        "status": "someNewFutureStatus"
    });
    let sub = RunningSubagent::from_value(&sub_val).expect("should parse");
    assert_eq!(sub.status, "someNewFutureStatus");
    assert_eq!(sub.subagent_type, "custom-agent-type");

    let work_val = json!({
        "workId": "work-unk",
        "kind": "future_kind",
        "title": "Future Work",
        "status": "future_status",
        "startedAt": 500
    });
    let work = BackgroundWork::from_value(&work_val).expect("should parse");
    assert_eq!(work.kind, "future_kind");
    assert_eq!(work.status, "future_status");
}

#[test]
fn joins_running_subagents_with_background_works() {
    let subagents = SubagentsState {
        revision: 1,
        child_session_ids: vec!["child-1".into(), "child-2".into()],
        running: vec![
            RunningSubagent {
                child_session_id: "child-1".into(),
                agent_id: Some("agent-1".into()),
                tool_call_id: Some("tool-1".into()),
                subagent_type: "bash".into(),
                title: "Run build".into(),
                summary: None,
                status: "running".into(),
                started_at: Some(1000),
            },
            RunningSubagent {
                child_session_id: "child-2".into(),
                agent_id: None,
                tool_call_id: None,
                subagent_type: "subagent".into(),
                title: "Explore codebase".into(),
                summary: Some("searching".into()),
                status: "running".into(),
                started_at: Some(2000),
            },
        ],
        ended_total: 0,
    };

    let background_works = vec![
        BackgroundWork {
            work_id: "work-1".into(),
            kind: "subagent".into(),
            title: "Run build".into(),
            status: "running".into(),
            started_at: 1000,
            ended_at: None,
            cancellable: Some(true),
            blocked: None,
            anchor_row_id: None,
            child_session_id: Some("child-1".into()),
        },
        BackgroundWork {
            work_id: "work-2".into(),
            kind: "subagent".into(),
            title: "Explore codebase".into(),
            status: "running".into(),
            started_at: 2000,
            ended_at: None,
            cancellable: Some(false),
            blocked: None,
            anchor_row_id: None,
            child_session_id: Some("child-2".into()),
        },
    ];

    let joined = join_running_subagents(Some(&subagents), &background_works);
    assert_eq!(joined.len(), 2);

    assert_eq!(joined[0].child_session_id, "child-1");
    assert_eq!(joined[0].work_id.as_deref(), Some("work-1"));
    assert!(joined[0].cancellable);

    assert_eq!(joined[1].child_session_id, "child-2");
    assert_eq!(joined[1].work_id.as_deref(), Some("work-2"));
    assert!(!joined[1].cancellable);
}

#[test]
fn joins_falls_back_when_work_not_yet_in_subagents_running() {
    let background_works = vec![BackgroundWork {
        work_id: "work-orphaned".into(),
        kind: "subagent".into(),
        title: "Cold transition work".into(),
        status: "running".into(),
        started_at: 3000,
        ended_at: None,
        cancellable: None,
        blocked: Some(true),
        anchor_row_id: None,
        child_session_id: Some("child-orphaned".into()),
    }];

    let joined = join_running_subagents(None, &background_works);
    assert_eq!(joined.len(), 1);
    assert_eq!(joined[0].child_session_id, "child-orphaned");
    assert_eq!(joined[0].work_id.as_deref(), Some("work-orphaned"));
    assert_eq!(joined[0].status, "blocked");
    assert!(joined[0].cancellable); // default is cancellable when not explicitly false
}

#[test]
fn duplicate_child_session_ids_in_background_works_are_not_cancellable() {
    let subagents = SubagentsState {
        revision: 1,
        child_session_ids: vec!["child-dup".into()],
        running: vec![RunningSubagent {
            child_session_id: "child-dup".into(),
            agent_id: None,
            tool_call_id: None,
            subagent_type: "subagent".into(),
            title: "Ambiguous task".into(),
            summary: None,
            status: "running".into(),
            started_at: Some(100),
        }],
        ended_total: 0,
    };

    let background_works = vec![
        BackgroundWork {
            work_id: "work-a".into(),
            kind: "subagent".into(),
            title: "Task A".into(),
            status: "running".into(),
            started_at: 100,
            ended_at: None,
            cancellable: Some(true),
            blocked: None,
            anchor_row_id: None,
            child_session_id: Some("child-dup".into()),
        },
        BackgroundWork {
            work_id: "work-b".into(),
            kind: "subagent".into(),
            title: "Task B".into(),
            status: "running".into(),
            started_at: 100,
            ended_at: None,
            cancellable: Some(true),
            blocked: None,
            anchor_row_id: None,
            child_session_id: Some("child-dup".into()),
        },
    ];

    let joined = join_running_subagents(Some(&subagents), &background_works);
    assert_eq!(joined.len(), 1);
    assert_eq!(joined[0].work_id, None);
    assert!(!joined[0].cancellable);
}
