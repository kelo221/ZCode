//! Golden tests for `conversation/interactions.rs`: snapshot parsing and the
//! `resolveInteraction` answer shapes the CLI broker maps.

use super::*;

#[test]
fn parses_permission_with_full_access() {
    let v = json!({
        "interactionId": "req-1", "kind": "permission", "anchorRowId": 4, "createdAt": 1,
        "payload": {
            "kind": "permission", "toolCallId": "t1", "toolName": "Bash",
            "summary": "Run ls", "detail": null, "freeText": true,
            "options": [
                { "optionId": "allowOnce", "label": "Allow once", "kind": "allowOnce" },
                { "optionId": "allowAlways", "label": "Allow always", "kind": "allowAlways" },
                { "optionId": "deny", "label": "Deny", "kind": "deny" }
            ],
            "fullAccessOption": { "optionId": "fullAccess", "label": "Full access", "kind": "custom" }
        }
    });
    let pi = PendingInteraction::from_value(&v).unwrap();
    assert_eq!(pi.kind, "permission");
    assert_eq!(pi.tool_name, "Bash");
    assert_eq!(pi.text, "Run ls");
    assert_eq!(pi.options.len(), 3);
    assert!(pi.free_text);
    assert_eq!(pi.deny_option().unwrap().option_id, "deny");
    assert_eq!(
        pi.full_access.as_ref().unwrap().option_id,
        FULL_ACCESS_OPTION_ID
    );
}

#[test]
fn parses_ask_user_question() {
    let v = json!({
        "interactionId": "req-2", "kind": "userInput", "anchorRowId": null, "createdAt": 1,
        "payload": {
            "kind": "userInput", "prompt": "Pick", "freeText": false,
            "questions": [
                { "question": "Which DB?", "header": "DB",
                  "options": [ { "value": "pg", "label": "Postgres", "description": "SQL" },
                               { "value": "my", "label": "MySQL" } ] },
                { "question": "Extras?", "header": "Ext", "multiSelect": true,
                  "options": [ { "value": "a", "label": "Cache" }, { "value": "b", "label": "Queue" } ] }
            ]
        }
    });
    let pi = PendingInteraction::from_value(&v).unwrap();
    assert_eq!(pi.kind, "userInput");
    assert_eq!(pi.text, "Pick");
    assert!(pi.options.is_empty(), "no synthesized permission options");
    assert_eq!(pi.questions.len(), 2);
    assert_eq!(
        pi.questions[0].options[0],
        ("Postgres".into(), "SQL".into())
    );
    assert!(pi.questions[1].multi_select);
}

#[test]
fn answer_shapes_match_broker_mapping() {
    assert_eq!(
        option_answer("allowAlways"),
        json!({ "optionId": "allowAlways" })
    );
    assert_eq!(
        free_text_answer(Some("deny"), "use rg instead"),
        json!({ "optionId": "deny", "freeText": "use rg instead" })
    );
    assert_eq!(
        free_text_answer(None, "Postgres"),
        json!({ "freeText": "Postgres" })
    );
    assert_eq!(decline_answer(), json!({ "action": "decline" }));
}

#[test]
fn questions_answer_keys_by_question_text() {
    let questions = vec![
        Question {
            question: "Which DB?".into(),
            header: "DB".into(),
            options: vec![],
            multi_select: false,
        },
        Question {
            question: "Extras?".into(),
            header: "Ext".into(),
            options: vec![],
            multi_select: true,
        },
    ];
    let mut picks: HashMap<usize, Vec<String>> = HashMap::new();
    toggle_pick(picks.entry(0).or_default(), "MySQL", false);
    toggle_pick(picks.entry(0).or_default(), "Postgres", false);
    toggle_pick(picks.entry(1).or_default(), "Cache", true);
    toggle_pick(picks.entry(1).or_default(), "Queue", true);
    toggle_pick(picks.entry(1).or_default(), "Cache", true);
    assert_eq!(
        questions_answer(&questions, &picks),
        json!({ "action": "accept",
                "content": { "answers": { "Which DB?": "Postgres", "Extras?": "Queue" } } })
    );
}

#[test]
fn queue_items_use_queue_item_id() {
    let q = crate::conversation::queue::QueueState::from_value(&json!({
        "autoDrain": false, "pauseReason": "stopped",
        "items": [ { "queueItemId": "q-1", "sourceCommandId": "c-1", "text": "next",
                     "dispatch": { "state": "queued" } } ]
    }))
    .unwrap();
    assert_eq!(q.items[0].queue_item_id, "q-1");
    assert!(!q.auto_drain);
    assert_eq!(q.pause_reason.as_deref(), Some("stopped"));
}

#[test]
fn active_phases() {
    assert!(crate::conversation::model::phase_is_active("running"));
    assert!(crate::conversation::model::phase_is_active("prewarming"));
    assert!(!crate::conversation::model::phase_is_active("completedSuccess"));
    assert!(!crate::conversation::model::phase_is_active("starting"));
}
