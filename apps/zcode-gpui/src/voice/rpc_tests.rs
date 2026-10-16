use crate::app::store::AppState;
use crate::app::test_support::RUNTIME_TEST_LOCK;
use crate::shared::preferences::Preferences;
use crate::shared::settings::AppSettings;
use crate::shared::voice_settings::VoiceSettings;
use gpui::{Entity, TestApp};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::mpsc::Receiver;

fn install(voice: Option<VoiceSettings>) -> TestApp {
    let path = std::env::temp_dir()
        .join(format!("gpui-voice-rpc-{}", uuid::Uuid::now_v7()))
        .join("setting.json");
    let mut app = TestApp::with_text_system_and_assets(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(ely_gpui_component::Assets),
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        let settings = AppSettings {
            gpui_voice: voice,
            ..Default::default()
        };
        Preferences::install_at(settings, path, Arc::new(|| false), cx);
    });
    app
}

fn speak(
    app: &mut TestApp,
    service: bool,
    window_active: bool,
    message: &str,
) -> (Entity<AppState>, bool, Receiver<String>) {
    let state = app.new_entity(AppState::for_test);
    let (routed, rx) = app.update_entity(&state, |s, cx| {
        let (tx, rx) = std::sync::mpsc::channel();
        s.workspaces[0].inbound = Some(tx);
        s.workspaces[0].started = true;
        if service {
            s.voice = Some(crate::voice::VoiceService::new());
            s.set_voice_window_active(window_active);
        }
        let key = s.workspaces[0].key.clone();
        let params = json!({ "sessionId": "s1", "message": message, "kind": "done" });
        let routed = s.route_voice_rpc(&key, &json!(7), "interaction/speak", &params, cx);
        (routed, rx)
    });
    (state, routed, rx)
}

fn reply(rx: &Receiver<String>) -> Value {
    serde_json::from_str(&rx.try_recv().expect("a reply")).unwrap()
}

fn enabled(only_when_unfocused: bool) -> Option<VoiceSettings> {
    Some(VoiceSettings {
        enabled: true,
        only_when_unfocused,
        ..Default::default()
    })
}

#[test]
fn without_a_voice_service_the_generic_handler_answers() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = install(enabled(false));
    let (_, routed, rx) = speak(&mut app, false, true, "Done.");
    assert!(!routed);
    assert!(rx.try_recv().is_err());
}

#[test]
fn voice_off_rejects_as_disabled() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = install(None);
    let (_, routed, rx) = speak(&mut app, true, false, "Done.");
    assert!(routed);
    let line = reply(&rx);
    assert_eq!(line["id"], 7);
    assert_eq!(
        line["result"],
        json!({ "accepted": false, "reason": "disabled" })
    );
}

#[test]
fn focused_window_rejects_when_only_unfocused() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = install(enabled(true));
    let (_, _, rx) = speak(&mut app, true, true, "Done.");
    assert_eq!(reply(&rx)["result"]["reason"], "unfocusedOnly");
}

#[test]
fn blank_message_is_never_sent_to_the_engine() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut app = install(enabled(false));
    let (state, _, rx) = speak(&mut app, true, false, "   ");
    assert_eq!(reply(&rx)["result"]["reason"], "unavailable");
    app.read_entity(&state, |s, _| {
        assert!(!s.voice.as_ref().unwrap().running());
    });
}
