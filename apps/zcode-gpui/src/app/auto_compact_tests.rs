//! Tests for `app/auto_compact.rs`: the maintenance pass picks eligible
//! conversations, resumes released ones first and settles the ack.

use crate::app::{store::AppState, test_support::RUNTIME_TEST_LOCK};
use crate::backend::workspace::RouteSubscription;
use crate::conversation::auto_compact::AutoCompactFlight;
use crate::conversation::context_usage::ContextWindow;
use crate::conversation::model::ConversationState;
use crate::shared::auto_compact_settings::AutoCompactEntry;
use crate::shared::preferences::Preferences;
use crate::shared::settings::AppSettings;
use gpui::{Entity, TestApp};
use serde_json::{Value, json};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

const SID: &str = "s1";

/// `state` is declared before `app` so the entity handle drops first.
struct Harness {
    state: Entity<AppState>,
    app: TestApp,
    rx: Receiver<String>,
    t0: Instant,
}

/// One started workspace holding an idle conversation on `p/m` with a large
/// context, and the `p/m` auto-compact entry set to `enabled`.
fn harness(enabled: bool, subscribed: bool) -> Harness {
    let mut app = TestApp::with_text_system_and_assets(
        std::sync::Arc::new(gpui::NoopTextSystem::new()),
        std::sync::Arc::new(ely_gpui_component::Assets),
    );
    let dir = std::env::temp_dir().join(format!("gpui-auto-compact-{}", uuid::Uuid::now_v7()));
    let mut settings = AppSettings::default();
    settings.set_auto_compact(
        "p/m",
        AutoCompactEntry {
            enabled,
            minutes: 50,
        },
    );
    app.update(|cx| {
        ely_gpui_component::init(cx);
        Preferences::install_at(
            settings,
            dir.join("setting.json"),
            std::sync::Arc::new(|| false),
            cx,
        );
    });
    let state = app.new_entity(AppState::for_test);
    let (tx, rx) = std::sync::mpsc::channel();
    let t0 = Instant::now();
    app.update_entity(&state, |s, _| {
        let ws = &mut s.workspaces[0];
        ws.inbound = Some(tx);
        ws.started = true;
        ws.pumping = true;
        ws.subscriptions.insert(
            format!("conversation/{SID}"),
            RouteSubscription {
                id: "sub".into(),
                log_epoch: "e".into(),
            },
        );
        let mut c = ConversationState {
            subscribed,
            context_window: Some(ContextWindow {
                used: 90_000.,
                max: 200_000.,
                cache_hit_rate: None,
                segments: Vec::new(),
            }),
            ..Default::default()
        };
        c.config.provider = "p".into();
        c.config.model = "m".into();
        c.note_control(false, t0);
        s.conversations.insert(SID.into(), c);
    });
    Harness { state, app, rx, t0 }
}

impl Harness {
    fn tick(&mut self, minutes: u64) {
        let now = self.t0 + Duration::from_secs(minutes * 60);
        self.app
            .update_entity(&self.state, |s, cx| s.run_auto_compaction(now, cx));
    }

    fn sent(&self) -> Vec<Value> {
        self.rx
            .try_iter()
            .map(|l| serde_json::from_str::<Value>(&l).unwrap())
            .collect()
    }

    fn flight(&mut self) -> Option<AutoCompactFlight> {
        self.app.update_entity(&self.state, |s, _| {
            s.conversations[SID].auto_compact.clone()
        })
    }
}

fn compacts(lines: &[Value]) -> usize {
    lines
        .iter()
        .filter(|v| v["method"] == "v4/command" && v["params"]["type"] == "compact")
        .count()
}

#[test]
fn an_idle_subscribed_conversation_is_compacted_once() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = harness(true, true);
    h.tick(49);
    assert_eq!(compacts(&h.sent()), 0, "not idle long enough");
    h.tick(50);
    let lines = h.sent();
    assert_eq!(compacts(&lines), 1);
    assert_eq!(lines[0]["params"]["sessionId"], SID);
    let command_id = lines[0]["params"]["commandId"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(h.flight(), Some(AutoCompactFlight::Sent { command_id }));
    h.tick(51);
    assert_eq!(compacts(&h.sent()), 0, "no second send while in flight");
}

#[test]
fn a_disabled_model_is_left_alone() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = harness(false, true);
    h.tick(500);
    assert!(h.sent().is_empty());
    assert_eq!(h.flight(), None);
}

#[test]
fn a_released_conversation_is_resubscribed_before_compacting() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = harness(true, false);
    h.tick(50);
    let lines = h.sent();
    assert_eq!(compacts(&lines), 0);
    assert!(
        lines
            .iter()
            .any(|v| v["method"] == "v4/conversation/subscribe"
                && v["params"]["topic"] == format!("conversation/{SID}"))
    );
    // The subscribe ack arrives; the next pass sends the compact.
    h.app.update_entity(&h.state, |s, _| {
        let ws = &mut s.workspaces[0];
        ws.pending.clear();
        s.conversations.get_mut(SID).unwrap().subscribed = true;
    });
    h.tick(51);
    assert_eq!(compacts(&h.sent()), 1);
}

#[test]
fn a_failed_subscription_gives_up_without_retrying() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = harness(true, false);
    h.tick(50);
    h.app
        .update_entity(&h.state, |s, _| s.workspaces[0].pending.clear());
    h.tick(51);
    assert_eq!(h.flight(), None);
    let _ = h.sent();
    h.tick(120);
    assert!(h.sent().is_empty(), "waits for new activity");
}

#[test]
fn a_rejected_ack_is_logged_not_bannered_and_ends_the_flight() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = harness(true, true);
    h.tick(50);
    let id = h.sent()[0]["id"].as_u64().unwrap();
    h.app.update_entity(&h.state, |s, cx| {
        let key = s.workspaces[0].key.clone();
        s.handle_response(
            &key,
            id,
            Some(json!({ "status": "rejected", "reasonCode": "compactOperationLock" })),
            None,
            cx,
        );
        assert!(s.errors.is_empty());
        assert!(s.log.iter().any(|l| l.contains("compactOperationLock")));
    });
    assert_eq!(h.flight(), None);
}

#[test]
fn an_accepted_ack_runs_until_its_terminal_marker_however_long_it_takes() {
    let _guard = RUNTIME_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut h = harness(true, true);
    h.tick(50);
    let line = h.sent().remove(0);
    let id = line["id"].as_u64().unwrap();
    let command_id = line["params"]["commandId"].as_str().unwrap().to_string();
    h.app.update_entity(&h.state, |s, cx| {
        let key = s.workspaces[0].key.clone();
        s.handle_response(&key, id, Some(json!({ "status": "accepted" })), None, cx);
    });
    let running = Some(AutoCompactFlight::Running {
        command_id: command_id.clone(),
    });
    assert_eq!(h.flight(), running);
    // No timeout: a slow compaction stays in flight across many ticks.
    h.tick(90);
    assert_eq!(h.flight(), running);
    h.app.update_entity(&h.state, |s, _| {
        s.conversations.get_mut(SID).unwrap().apply_deltas(&[json!({
            "op": "row.upserted",
            "row": {
                "rowId": 40,
                "kind": "timelineMarker",
                "sourceCommandId": command_id,
                "marker": { "type": "compact", "origin": "manual", "status": "success" },
            },
        })]);
    });
    assert_eq!(h.flight(), None);
    h.tick(91);
    h.app.update_entity(&h.state, |s, _| {
        assert!(s.log.iter().any(|l| l.contains("finished (success)")));
    });
}
