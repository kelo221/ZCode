//! Real TLS sockets on loopback: the listener and the phone client against
//! each other, with a stand-in gateway answering the handshake.

use super::*;
use crate::remote::client::{RemoteTarget, list_workspaces, pair};
use crate::remote::handshake::RemoteWorkspace;
use crate::remote::link::PairLink;
use crate::remote::tls::DesktopIdentity;
use futures::StreamExt;

struct Desktop {
    _handle: ListenerHandle,
    port: u16,
    fingerprint: String,
    store: Arc<Mutex<PairingStore>>,
}

fn workspaces() -> Vec<RemoteWorkspace> {
    vec![RemoteWorkspace {
        key: "C:\\w".into(),
        display: "w".into(),
        purpose: "project".into(),
    }]
}

/// Start a listener plus a gateway thread that answers `Authed` with the
/// workspace list, like `AppState` does.
fn desktop() -> Desktop {
    let dir = std::env::temp_dir().join(format!("zcode-listener-{}", uuid::Uuid::now_v7()));
    let identity = DesktopIdentity::load_or_create(&dir).unwrap();
    let again = DesktopIdentity::load_or_create(&dir).unwrap();
    assert_eq!(identity.fingerprint, again.fingerprint, "identity persists");
    let store = Arc::new(Mutex::new(PairingStore::load(None)));
    let (tx, mut rx) = futures::channel::mpsc::unbounded();
    let handle = start(
        "127.0.0.1:0".parse().unwrap(),
        identity.server_config().unwrap(),
        store.clone(),
        tx,
    )
    .unwrap();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        while let Some(event) = futures::executor::block_on(rx.next()) {
            if let GatewayEvent::Authed { outbound, .. } = event {
                let line = DesktopReply::Workspaces {
                    workspaces: workspaces(),
                }
                .line();
                assert!(outbound.send(line));
                held.push(outbound);
            }
        }
    });
    let _ = std::fs::remove_dir_all(dir);
    Desktop {
        port: handle.addr.port(),
        _handle: handle,
        fingerprint: identity.fingerprint,
        store,
    }
}

fn link(desktop: &Desktop, token: String) -> PairLink {
    PairLink {
        host: "127.0.0.1".into(),
        port: desktop.port,
        fingerprint: desktop.fingerprint.clone(),
        token,
    }
}

#[test]
fn pair_then_auth_lists_workspaces() {
    let desktop = desktop();
    let token = desktop
        .store
        .lock()
        .unwrap()
        .issue_pairing_token(Instant::now());
    let (target, listed) = pair(&link(&desktop, token.clone()), "Pixel").unwrap();
    assert_eq!(listed, workspaces());
    assert_eq!(
        desktop
            .store
            .lock()
            .unwrap()
            .file
            .device
            .as_ref()
            .unwrap()
            .name,
        "Pixel"
    );
    assert_eq!(list_workspaces(&target).unwrap(), workspaces());
    // The pairing token was single use.
    assert!(pair(&link(&desktop, token), "Pixel").is_err());
}

#[test]
fn wrong_fingerprint_is_refused_by_the_phone() {
    let desktop = desktop();
    let target = RemoteTarget {
        host: "127.0.0.1".into(),
        port: desktop.port,
        fingerprint: "00".repeat(32),
        device_token: "x".into(),
    };
    let error = list_workspaces(&target).unwrap_err();
    assert!(error.contains("desktop link failed"), "{error}");
}

#[test]
fn revoked_or_unknown_devices_are_refused_and_rate_limited() {
    let desktop = desktop();
    let token = desktop
        .store
        .lock()
        .unwrap()
        .issue_pairing_token(Instant::now());
    let (target, _) = pair(&link(&desktop, token), "Pixel").unwrap();
    desktop.store.lock().unwrap().revoke().unwrap();
    let error = list_workspaces(&target).unwrap_err();
    assert!(error.contains("unauthorized"), "{error}");
    for _ in 0..4 {
        let _ = list_workspaces(&target);
    }
    let error = list_workspaces(&target).unwrap_err();
    assert!(error.contains("rate_limited"), "{error}");
}

#[test]
fn outbound_queue_is_bounded() {
    let (outbound, _rx, _queued) = PhoneOutbound::new();
    assert!(outbound.send("x".repeat(MAX_QUEUED_BYTES - 1)));
    assert!(!outbound.send("yy".into()));
}
