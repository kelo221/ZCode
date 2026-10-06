use super::*;
use crate::conversation::model::ConversationState;
use gpui::{AppContext, TestAppContext};

#[test]
fn snapshot_and_patch_replace_goal_facts_and_availability() {
    let mut conv = ConversationState::default();
    conv.apply_snapshot(&json!({"revision":7,"goal":{"objective":"Finish","status":"paused","iteration":2},"availability":{"resumeGoal":{"allowed":true}}}));
    assert_eq!(conv.goal.as_ref().unwrap().status, "paused");
    assert!(conv.goal_availability.resume);
    assert!(!conv.goal_availability.pause);
    conv.apply_deltas(&[json!({"op":"state.updated","patch":{"goal":null,"availability":{}}})]);
    assert!(conv.goal.is_none());
    assert!(!conv.goal_availability.resume);
}

#[gpui::test]
fn goal_actions_use_snapshot_guard_and_revision(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let key = s.workspaces[0].key.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        s.active_workspace = Some(key.clone());
        s.active = Some("parent".into());
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        let mut conv = ConversationState::default();
        conv.apply_snapshot(&json!({"revision":7,"goal":{"objective":"Finish","status":"paused","iteration":2},"availability":{"resumeGoal":{"allowed":true}}}));
        s.conversations.insert("parent".into(), conv);
        assert!(!s.change_goal(&key, "parent", false, cx));
        assert!(rx.try_recv().is_err());
        assert!(s.change_goal(&key, "parent", true, cx));
        assert!(!s.change_goal(&key, "parent", true, cx));
        let request: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(request["params"]["type"], "resumeGoal");
        assert_eq!(request["params"]["baseRevision"], 7);
        assert!(rx.try_recv().is_err());
        s.handle_response(&key, request["id"].as_u64().unwrap(), Some(json!({"status":"rejected"})), None, cx);
        assert_eq!(s.conversations["parent"].goal.as_ref().unwrap().status, "paused");
    });
}
