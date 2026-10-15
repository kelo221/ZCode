//! Gateway routing inside `AppState`: a phone socket shares a workspace CLI
//! with the desktop without either side seeing the other's traffic.

use crate::app::store::AppState;
use crate::backend::launcher::ConnEvent;
use crate::remote::listener::{GatewayEvent, PhoneOutbound};
use gpui::{AppContext, Entity, TestAppContext};
use serde_json::{Value, json};
use std::sync::mpsc::Receiver;

struct Rig {
    state: Entity<AppState>,
    key: String,
    cli: Receiver<String>,
    phone: Receiver<String>,
}

fn drain(rx: &Receiver<String>) -> Vec<Value> {
    rx.try_iter()
        .map(|l| serde_json::from_str(&l).unwrap())
        .collect()
}

/// A started workspace whose CLI stdin is `cli`, plus an authenticated
/// phone socket (conn 1) that has opened it.
fn rig(cx: &mut TestAppContext) -> Rig {
    let state = cx.new(AppState::for_test);
    let (cli_tx, cli) = std::sync::mpsc::channel();
    let (outbound, phone, _queued) = PhoneOutbound::new();
    let key = state.update(cx, |s, cx| {
        let ws = &mut s.workspaces[0];
        ws.inbound = Some(cli_tx);
        ws.started = true;
        ws.pumping = true;
        let key = ws.key.clone();
        s.handle_gateway_event(
            GatewayEvent::Authed {
                conn: 1,
                outbound,
                device: "Pixel".into(),
            },
            cx,
        );
        let open = json!({"type": "open", "workspaceKey": key}).to_string();
        s.handle_gateway_event(
            GatewayEvent::Line {
                conn: 1,
                line: open,
            },
            cx,
        );
        key
    });
    let replies = drain(&phone);
    assert_eq!(replies[0]["type"], json!("workspaces"));
    assert_eq!(replies[0]["workspaces"][0]["key"], json!(key));
    assert_eq!(replies[1]["type"], json!("opened"));
    assert_eq!(replies[2]["method"], json!("startup/storageState"));
    assert_eq!(replies[2]["params"]["phase"], json!("ready"));
    Rig {
        state,
        key,
        cli,
        phone,
    }
}

fn phone_line(rig: &Rig, cx: &mut TestAppContext, line: Value) {
    rig.state.update(cx, |s, cx| {
        s.handle_gateway_event(
            GatewayEvent::Line {
                conn: 1,
                line: line.to_string(),
            },
            cx,
        )
    });
}

fn cli_line(rig: &Rig, cx: &mut TestAppContext, line: Value) {
    let key = rig.key.clone();
    rig.state.update(cx, |s, cx| {
        s.handle_conn(&key, ConnEvent::Line(line.to_string()), cx);
    });
}

fn frame(sub: &str) -> Value {
    json!({"method": "v4/conversation/frame", "params": {
        "topic": "conversation/s1", "subscriptionId": sub, "kind": "complete"}})
}

#[gpui::test]
fn phone_subscription_round_trips_without_touching_the_desktop(cx: &mut TestAppContext) {
    let rig = rig(cx);
    phone_line(
        &rig,
        cx,
        json!({"id": 3, "method": "v4/conversation/subscribe",
               "params": {"topic": "conversation/s1", "connectionId": "phone"}}),
    );
    let sent = drain(&rig.cli);
    assert_eq!(sent.len(), 1);
    let remote_id = sent[0]["id"].clone();
    assert!(crate::remote::scope::is_remote_id(&remote_id));
    assert_eq!(
        sent[0]["params"]["clientMode"],
        json!("web-remote-replayable")
    );

    cli_line(
        &rig,
        cx,
        json!({"id": remote_id, "result": {"ack": {"subscriptionId": "sub-p", "logEpoch": "e"}}}),
    );
    let ack = drain(&rig.phone);
    assert_eq!(ack[0]["id"], json!(3));
    assert_eq!(ack[0]["result"]["ack"]["subscriptionId"], json!("sub-p"));

    cli_line(&rig, cx, frame("sub-p"));
    cli_line(&rig, cx, frame("sub-desktop"));
    let frames = drain(&rig.phone);
    assert_eq!(frames.len(), 1, "only the phone's own frame is forwarded");
    assert_eq!(frames[0]["params"]["subscriptionId"], json!("sub-p"));
    rig.state.read_with(cx, |s, _| {
        assert!(s.workspaces[0].subscriptions.is_empty());
        assert!(s.phone.has_open(&rig.key));
    });
}

#[gpui::test]
fn rejected_and_locally_answered_requests_never_reach_the_cli(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.state.update(cx, |s, _| {
        s.workspaces[0].model_settings = Some(json!({"models": ["m"]}));
    });
    phone_line(
        &rig,
        cx,
        json!({"id": 1, "method": "plugins/list", "params": {}}),
    );
    phone_line(
        &rig,
        cx,
        json!({"id": 2, "method": "session/create", "params": {}}),
    );
    assert!(drain(&rig.cli).is_empty());
    let replies = drain(&rig.phone);
    assert_eq!(replies[0]["error"]["code"], json!(-32601));
    assert_eq!(replies[1]["result"]["settings"]["models"], json!(["m"]));
}

#[gpui::test]
fn closing_the_socket_releases_the_cli_and_drops_late_frames(cx: &mut TestAppContext) {
    let rig = rig(cx);
    phone_line(
        &rig,
        cx,
        json!({"id": 3, "method": "v4/conversation/subscribe",
               "params": {"topic": "conversation/s1"}}),
    );
    let remote_id = drain(&rig.cli)[0]["id"].clone();
    cli_line(
        &rig,
        cx,
        json!({"id": remote_id, "result": {"ack": {"subscriptionId": "sub-p", "logEpoch": "e"}}}),
    );
    rig.state.update(cx, |s, cx| {
        s.handle_gateway_event(GatewayEvent::Closed { conn: 1 }, cx)
    });
    let released = drain(&rig.cli);
    assert_eq!(released[0]["params"]["state"], json!("closed"));
    assert_eq!(released[1]["method"], json!("v4/conversation/unsubscribe"));
    assert_eq!(released[1]["params"]["subscriptionId"], json!("sub-p"));
    let key = rig.key.clone();
    rig.state.update(cx, |s, _| {
        assert!(!s.phone.has_open(&key));
        assert!(
            s.route_cli_line_to_phone(&key, &frame("sub-p").to_string()),
            "late frames of a closed phone scope are consumed"
        );
        assert!(!s.route_cli_line_to_phone(&key, &frame("sub-desktop").to_string()));
    });
}

#[gpui::test]
fn cli_teardown_closes_phone_sockets(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let key = rig.key.clone();
    rig.state.update(cx, |s, _| {
        s.reset_connection_state(&key);
        assert!(!s.phone.has_open(&key));
    });
}

#[gpui::test]
fn opening_an_unknown_workspace_is_refused(cx: &mut TestAppContext) {
    let state = cx.new(AppState::for_test);
    let (outbound, phone, _queued) = PhoneOutbound::new();
    state.update(cx, |s, cx| {
        s.handle_gateway_event(
            GatewayEvent::Authed {
                conn: 9,
                outbound,
                device: "Pixel".into(),
            },
            cx,
        );
        let open = json!({"type": "open", "workspaceKey": "C:\\nope"}).to_string();
        s.handle_gateway_event(
            GatewayEvent::Line {
                conn: 9,
                line: open,
            },
            cx,
        );
        assert_eq!(s.phone.connected_sockets(), 0);
    });
    let replies = drain(&phone);
    assert_eq!(replies[1]["code"], json!("unknown_workspace"));
}

#[gpui::test]
fn early_catalog_probe_waits_for_the_desktop_catalog(cx: &mut TestAppContext) {
    let rig = rig(cx);
    phone_line(
        &rig,
        cx,
        json!({"id": 6, "method": "session/create", "params": {}}),
    );
    assert!(drain(&rig.phone).is_empty(), "no empty catalog is sent");
    assert!(drain(&rig.cli).is_empty());
    let key = rig.key.clone();
    rig.state.update(cx, |s, _| {
        s.cache_model_settings(&key, &json!({"models": ["m"]}));
    });
    let replies = drain(&rig.phone);
    assert_eq!(replies[0]["id"], json!(6));
    assert_eq!(replies[0]["result"]["settings"]["models"], json!(["m"]));
}
