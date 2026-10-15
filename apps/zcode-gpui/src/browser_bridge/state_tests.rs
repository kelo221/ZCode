use super::commands::DEFAULT_TIMEOUT;
use super::state::{BrowserBridge, Reply};
use crate::remote::listener::PhoneOutbound;
use serde_json::{Value, json};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

struct Ext {
    conn: u64,
    rx: Receiver<String>,
}

impl Ext {
    fn sent(&self) -> Vec<Value> {
        self.rx
            .try_iter()
            .map(|l| serde_json::from_str(&l).unwrap())
            .collect()
    }
}

fn connect(bridge: &mut BrowserBridge, conn: u64, profile: &str, now: Instant) -> Ext {
    let (outbound, rx, _) = PhoneOutbound::new();
    bridge.connected(conn, outbound);
    let hello = json!({
        "op": "hello", "profileId": profile, "browser": "Helium",
        "profile": "Work", "extensionVersion": "0.1.0"
    });
    assert!(bridge.handle_line(conn, &hello.to_string(), now).is_empty());
    Ext { conn, rx }
}

fn execute_params(request_id: &str, command: Value) -> Value {
    json!({ "requestId": request_id, "sessionId": "s1", "workspaceKey": "ws", "command": command })
}

fn result_of(line: &str) -> (Value, Value) {
    let v: Value = serde_json::from_str(line).unwrap();
    (v["id"].clone(), v["result"].clone())
}

fn reply_result(replies: &[Reply]) -> Value {
    assert_eq!(replies.len(), 1, "{replies:?}");
    result_of(&replies[0].line).1
}

#[test]
fn hello_registers_an_extension_descriptor_and_welcomes_it() {
    let mut bridge = BrowserBridge::default();
    assert!(bridge.descriptors().is_empty());
    let ext = connect(&mut bridge, 1, "p-1", Instant::now());
    let welcome = ext.sent();
    assert_eq!(welcome[0]["op"], "welcome");
    assert_eq!(welcome[0]["browserId"], "extension:p-1");
    let list = bridge.descriptors();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["type"], "extension");
    assert_eq!(list[0]["id"], "extension:p-1");
    assert_eq!(list[0]["generation"], welcome[0]["generation"]);
    assert_eq!(list[0]["name"], "Helium (Work)");
    assert_eq!(list[0]["apiSupportOverrides"]["BrowserUser.history"], false);
}

#[test]
fn a_socket_without_hello_is_not_a_browser() {
    let mut bridge = BrowserBridge::default();
    let (outbound, _rx, _) = PhoneOutbound::new();
    bridge.connected(1, outbound);
    bridge.handle_line(1, r#"{"op":"hello","profileId":"!!!"}"#, Instant::now());
    assert!(bridge.descriptors().is_empty());
    let line = bridge
        .execute(
            "ws",
            &json!(7),
            &execute_params("r1", json!({"method":"getState"})),
            Instant::now(),
        )
        .unwrap();
    let (id, result) = result_of(&line);
    assert_eq!(id, 7);
    assert_eq!(result["error"]["code"], "backend_unavailable");
}

#[test]
fn execute_round_trips_through_the_extension() {
    let mut bridge = BrowserBridge::default();
    let now = Instant::now();
    let ext = connect(&mut bridge, 1, "p", now);
    ext.sent();
    let mut params = execute_params("r1", json!({"method":"getState"}));
    params["browserId"] = json!("extension:p");
    assert!(bridge.execute("ws", &json!(9), &params, now).is_none());
    assert!(bridge.has_pending("ws"));
    let sent = ext.sent();
    assert_eq!(sent[0]["op"], "execute");
    assert_eq!(sent[0]["requestId"], "r1");
    assert_eq!(sent[0]["context"]["sessionId"], "s1");
    assert!(sent[0]["context"].get("browserId").is_none());

    let answer = json!({"op":"result","requestId":"r1","result":{"ok":true,"elapsedMs":3}});
    let replies = bridge.handle_line(ext.conn, &answer.to_string(), now);
    assert_eq!(replies[0].ws_key, "ws");
    let (id, result) = result_of(&replies[0].line);
    assert_eq!((id, result["ok"].clone()), (json!(9), json!(true)));
    assert!(!bridge.has_pending("ws"));
    // A second answer for the same request is dropped.
    assert!(
        bridge
            .handle_line(ext.conn, &answer.to_string(), now)
            .is_empty()
    );
}

#[test]
fn stale_generations_and_duplicate_request_ids_fail_without_sending() {
    let mut bridge = BrowserBridge::default();
    let now = Instant::now();
    let ext = connect(&mut bridge, 1, "p", now);
    ext.sent();
    let mut stale = execute_params("r1", json!({"method":"getState"}));
    stale["browserGeneration"] = json!(1);
    let (_, result) = result_of(&bridge.execute("ws", &json!(1), &stale, now).unwrap());
    assert_eq!(result["error"]["code"], "backend_unavailable");
    assert!(
        result["error"]["message"]
            .as_str()
            .unwrap()
            .contains("stale")
    );

    let params = execute_params("r2", json!({"method":"getState"}));
    assert!(bridge.execute("ws", &json!(2), &params, now).is_none());
    let (_, dup) = result_of(&bridge.execute("ws", &json!(3), &params, now).unwrap());
    assert_eq!(dup["error"]["code"], "duplicate_request_id");
    assert_eq!(dup["error"]["sideEffect"], "none");
    assert_eq!(ext.sent().len(), 1);

    let mut gone = execute_params("r3", json!({"method":"getState"}));
    gone["browserId"] = json!("extension:other");
    let (_, result) = result_of(&bridge.execute("ws", &json!(4), &gone, now).unwrap());
    assert_eq!(result["error"]["code"], "backend_unavailable");
}

#[test]
fn timeouts_answer_the_agent_and_cancel_in_the_browser() {
    let mut bridge = BrowserBridge::default();
    let now = Instant::now();
    let ext = connect(&mut bridge, 1, "p", now);
    ext.sent();
    let click = json!({"method":"click","ref":"e1"});
    assert!(
        bridge
            .execute("ws", &json!(5), &execute_params("r1", click), now)
            .is_none()
    );
    ext.sent();
    assert!(
        bridge
            .expire(now + DEFAULT_TIMEOUT - Duration::from_millis(1))
            .is_empty()
    );
    let replies = bridge.expire(now + DEFAULT_TIMEOUT);
    let result = reply_result(&replies);
    assert_eq!(result["error"]["code"], "timeout");
    assert_eq!(result["error"]["sideEffect"], "uncertain");
    let cancel = ext.sent();
    assert_eq!(cancel[0]["command"]["method"], "cancelRequest");
    assert_eq!(cancel[0]["command"]["requestId"], "r1");
    // The late answer after the timeout is ignored.
    let late = json!({"op":"result","requestId":"r1","result":{"ok":true,"elapsedMs":1}});
    assert!(
        bridge
            .handle_line(ext.conn, &late.to_string(), now)
            .is_empty()
    );
}

#[test]
fn fixed_waits_get_their_duration_plus_slack() {
    let mut bridge = BrowserBridge::default();
    let now = Instant::now();
    let _ext = connect(&mut bridge, 1, "p", now);
    let wait = json!({"method":"playwrightWaitForTimeout","timeoutMs":45_000});
    assert!(
        bridge
            .execute("ws", &json!(1), &execute_params("r1", wait), now)
            .is_none()
    );
    assert!(bridge.expire(now + Duration::from_secs(46)).is_empty());
    let replies = bridge.expire(now + Duration::from_secs(47));
    assert_eq!(reply_result(&replies)["error"]["sideEffect"], "none");
}

#[test]
fn a_disconnect_fails_in_flight_requests_by_side_effect() {
    let mut bridge = BrowserBridge::default();
    let now = Instant::now();
    let _ext = connect(&mut bridge, 1, "p", now);
    let nav = json!({"method":"navigate","url":"https://example.com"});
    bridge.execute("ws", &json!(1), &execute_params("nav", nav), now);
    bridge.execute(
        "ws2",
        &json!(2),
        &execute_params("state", json!({"method":"getState"})),
        now,
    );
    let mut replies = bridge.closed(1, now);
    replies.sort_by(|a, b| a.ws_key.cmp(&b.ws_key));
    let nav = result_of(&replies[0].line).1;
    let state = result_of(&replies[1].line).1;
    assert_eq!(nav["error"]["code"], "backend_unavailable");
    assert_eq!(nav["error"]["sideEffect"], "uncertain");
    assert_eq!(state["error"]["sideEffect"], "none");
    assert!(bridge.descriptors().is_empty());
}

#[test]
fn a_reconnect_of_the_same_profile_bumps_the_generation_and_replaces_the_socket() {
    let mut bridge = BrowserBridge::default();
    let now = Instant::now();
    let first = connect(&mut bridge, 1, "p", now);
    let g1 = first.sent()[0]["generation"].as_u64().unwrap();
    bridge.execute(
        "ws",
        &json!(1),
        &execute_params("r1", json!({"method":"getState"})),
        now,
    );

    let (outbound, rx, _) = PhoneOutbound::new();
    bridge.connected(2, outbound);
    let hello = json!({"op":"hello","profileId":"p","browser":"Helium"});
    let replies = bridge.handle_line(2, &hello.to_string(), now);
    assert_eq!(
        reply_result(&replies)["error"]["code"],
        "backend_unavailable"
    );
    let welcome: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
    assert!(welcome["generation"].as_u64().unwrap() > g1);
    assert_eq!(bridge.descriptors().len(), 1);
    // The old socket's answers no longer count.
    let late = json!({"op":"result","requestId":"r1","result":{"ok":true}});
    assert!(bridge.handle_line(1, &late.to_string(), now).is_empty());
}

#[test]
fn the_newest_browser_is_listed_first_and_is_the_default_target() {
    let mut bridge = BrowserBridge::default();
    let now = Instant::now();
    let chrome = connect(&mut bridge, 1, "chrome", now);
    let helium = connect(&mut bridge, 2, "helium", now + Duration::from_secs(1));
    let ids: Vec<Value> = bridge
        .descriptors()
        .iter()
        .map(|d| d["id"].clone())
        .collect();
    assert_eq!(
        ids,
        vec![json!("extension:helium"), json!("extension:chrome")]
    );
    chrome.sent();
    helium.sent();
    bridge.execute(
        "ws",
        &json!(1),
        &execute_params("a", json!({"method":"getState"})),
        now,
    );
    assert_eq!(helium.sent().len(), 1);
    let mut params = execute_params("b", json!({"method":"getState"}));
    params["browserId"] = json!("extension:chrome");
    bridge.execute("ws", &json!(2), &params, now);
    assert_eq!(chrome.sent().len(), 1);
}

#[test]
fn malformed_results_and_dropped_workspaces() {
    let mut bridge = BrowserBridge::default();
    let now = Instant::now();
    let ext = connect(&mut bridge, 1, "p", now);
    bridge.execute(
        "ws",
        &json!(1),
        &execute_params("r1", json!({"method":"click"})),
        now,
    );
    let bad = json!({"op":"result","requestId":"r1","result":"nope"});
    let result = reply_result(&bridge.handle_line(1, &bad.to_string(), now));
    assert_eq!(result["error"]["code"], "execution_error");
    assert_eq!(result["error"]["sideEffect"], "uncertain");

    bridge.execute(
        "ws",
        &json!(2),
        &execute_params("r2", json!({"method":"getState"})),
        now,
    );
    ext.sent();
    bridge.drop_workspace("ws");
    assert!(!bridge.has_pending("ws"));
    assert_eq!(ext.sent()[0]["command"]["method"], "cancelRequest");
    assert!(bridge.expire(now + Duration::from_secs(60)).is_empty());
}
