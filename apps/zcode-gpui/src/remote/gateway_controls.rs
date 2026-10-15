//! Settings › Phone controls for the desktop gateway: turning the listener
//! on and off, issuing pairing links and revoking the paired phone.

use crate::app::store::AppState;
use crate::remote::handshake::DEFAULT_PORT;
use futures::StreamExt;
use gpui::Context;
use std::net::SocketAddr;

impl AppState {
    /// Desktop startup: listen again if the user left it enabled.
    pub(crate) fn init_phone_gateway(&mut self, cx: &mut Context<Self>) {
        if self.phone.enabled()
            && let Err(error) = self.start_phone_listener(cx)
        {
            self.phone.error = Some(error);
        }
    }

    pub(crate) fn set_phone_gateway_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        let saved = self
            .phone
            .store
            .lock()
            .map_err(|_| "pairing store unavailable".to_string())
            .and_then(|mut s| s.set_enabled(enabled));
        self.phone.error = saved.err();
        if enabled {
            if let Err(error) = self.start_phone_listener(cx) {
                self.phone.error = Some(error);
            }
        } else {
            self.phone.listener = None;
            self.phone.link = None;
            self.close_all_phone_sockets();
        }
        cx.notify();
    }

    fn start_phone_listener(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if self.phone.listener.is_some() {
            return Ok(());
        }
        let data_root = crate::shared::data_paths::paths().data_root();
        let identity = crate::remote::tls::DesktopIdentity::load_or_create(&data_root)?;
        let config = identity.server_config()?;
        let ip = crate::remote::listener::lan_ip()
            .ok_or("no local network address found; is this computer on Wi-Fi or Ethernet?")?;
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        let handle = crate::remote::listener::start(
            SocketAddr::new(ip, DEFAULT_PORT),
            config,
            self.phone.store.clone(),
            tx,
        )
        .map_err(|e| format!("could not listen on {ip}:{DEFAULT_PORT}: {e}"))?;
        self.phone.fingerprint = Some(identity.fingerprint);
        self.phone.listener = Some(handle);
        self.phone.error = None;
        self.push_log(format!("phone gateway listening on {ip}:{DEFAULT_PORT}"));
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.next().await {
                let alive = this
                    .update(cx, |state, cx| state.handle_gateway_event(event, cx))
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
        Ok(())
    }

    /// A fresh single-use pairing link (5 minutes). Never logged.
    pub(crate) fn new_phone_pairing_link(&mut self, cx: &mut Context<Self>) {
        let (Some(addr), Some(fingerprint)) =
            (self.phone.address(), self.phone.fingerprint.clone())
        else {
            return;
        };
        let Ok(token) = self
            .phone
            .store
            .lock()
            .map(|mut s| s.issue_pairing_token(std::time::Instant::now()))
        else {
            return;
        };
        let link = crate::remote::link::PairLink {
            host: addr.ip().to_string(),
            port: addr.port(),
            fingerprint,
            token,
        };
        self.phone.link = Some(link.to_url());
        cx.notify();
    }

    pub(crate) fn revoke_phone(&mut self, cx: &mut Context<Self>) {
        let revoked = self
            .phone
            .store
            .lock()
            .map_err(|_| "pairing store unavailable".to_string())
            .and_then(|mut s| s.revoke());
        self.phone.error = revoked.err();
        self.phone.link = None;
        self.close_all_phone_sockets();
        cx.notify();
    }
}
