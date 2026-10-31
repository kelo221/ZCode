//! Phone remote control over the local network (spec: REMOTE_CONTROL.md,
//! "LAN slice"). The desktop build runs a TLS gateway that lets a paired
//! phone share its workspace CLIs; the Android build connects to it instead
//! of launching a CLI itself.
//!
//! - `scope`: per phone socket allowlist, id remap and subscription routing.
//! - `pairing`, `tls`, `listener`, `gateway`: the desktop side.
//! - `staging`: desktop end of phone attachment uploads.
//! - `file_read`: desktop end of phone image fetches.
//! - `client`, `phone_state`, `phone_files`, `desktop_images`: the phone side.
//! - `line_socket`, `link`, `handshake`: shared by both.

pub(crate) mod client;
pub(crate) mod desktop_images;
pub(crate) mod file_read;
pub(crate) mod gateway;
mod gateway_controls;
pub(crate) mod handshake;
pub(crate) mod line_socket;
pub(crate) mod link;
pub(crate) mod listener;
pub(crate) mod pairing;
pub mod phone_files;
pub(crate) mod phone_state;
pub(crate) mod scope;
pub(crate) mod staging;
pub(crate) mod tls;

use std::sync::Mutex;

/// Pairing link delivered by the platform (Android intent) before or after
/// the window exists; `AppState` polls it on the phone build.
static PENDING_LINK: Mutex<Option<String>> = Mutex::new(None);

/// Hand a `zcode-gpui://pair?...` link to the app. Called by the Android
/// entry when the OS opens such a link.
pub fn submit_pair_link(link: String) {
    if let Ok(mut slot) = PENDING_LINK.lock() {
        *slot = Some(link);
    }
}

pub(crate) fn take_pending_link() -> Option<String> {
    PENDING_LINK.lock().ok().and_then(|mut slot| slot.take())
}
