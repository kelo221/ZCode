use crate::app::store::AppState;
use gpui::{AppContext, TestAppContext};
use serde_json::{Value, json};
use std::sync::mpsc::Receiver;

fn connect(s: &mut AppState) -> (String, Receiver<String>) {
    let key = s.workspaces[0].key.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    s.workspaces[0].inbound = Some(tx);
    s.workspaces[0].started = true;
    s.active_workspace = Some(key.clone());
    (key, rx)
}

fn next(rx: &Receiver<String>) -> Value {
    serde_json::from_str(&rx.try_recv().unwrap()).unwrap()
}

#[gpui::test]
fn storage_ready_sync_sends_only_an_existing_override(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, _| {
        let (key, rx) = connect(s);
        s.sync_system_prompt(&key);
        assert!(rx.try_recv().is_err(), "the default needs no request");

        s.system_prompt.override_text = Some("Be brief.".into());
        s.sync_system_prompt(&key);
        let sent = next(&rx);
        assert_eq!(sent["method"], "workspace/updateSystemPrompt");
        assert_eq!(sent["params"]["prompt"], "Be brief.");
        assert_eq!(sent["params"]["workspace"]["workspaceKey"], key);
    });
}

#[gpui::test]
fn apply_and_reset_push_to_running_agents(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = connect(s);
        s.apply_system_prompt(Some("Talk like a pirate.".into()), cx);
        let applied = next(&rx);
        assert_eq!(applied["params"]["prompt"], "Talk like a pirate.");
        assert_eq!(
            s.system_prompt.override_text.as_deref(),
            Some("Talk like a pirate.")
        );
        let id = applied["id"].as_u64().unwrap();
        s.handle_response(
            &key,
            id,
            Some(json!({"override": "Talk like a pirate."})),
            None,
            cx,
        );
        assert!(s.ws(&key).unwrap().pending.is_empty());

        s.apply_system_prompt(None, cx);
        assert_eq!(next(&rx)["params"]["prompt"], Value::Null);
        assert_eq!(s.system_prompt.override_text, None);
    });
}

#[gpui::test]
fn reads_the_default_and_flags_backends_without_the_method(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    state.update(cx, |s, cx| {
        let (key, rx) = connect(s);
        s.read_system_prompt_default(cx);
        assert!(s.system_prompt.reading);
        let read = next(&rx);
        assert_eq!(read["method"], "workspace/readSystemPrompt");
        let result =
            json!({"defaultPrompt": "Default body", "defaultSource": "session", "override": null});
        s.handle_response(&key, read["id"].as_u64().unwrap(), Some(result), None, cx);
        let default = s.system_prompt.default_prompt.clone().unwrap();
        assert_eq!(default.text, "Default body");
        assert!(default.from_session);
        assert!(!s.system_prompt.reading);

        s.read_system_prompt_default(cx);
        let read = next(&rx);
        let error = json!({"code": -32601, "message": "Method not found"});
        s.handle_response(&key, read["id"].as_u64().unwrap(), None, Some(error), cx);
        assert!(s.system_prompt.unavailable);
        assert_eq!(
            s.system_prompt.error, None,
            "a missing method is not an error banner"
        );
        assert!(s.errors.is_empty());
    });
}
