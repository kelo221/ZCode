//! Browser Use through the user's own browser (spec: BROWSER_BRIDGE.md).
//! The ZCode extension runs browser commands; Chromium starts
//! `zcode-browser-host`, which relays them to this loopback bridge.
//!
//! - `native`: frames and endpoint file, shared with the host binary.
//! - `listener`: loopback socket threads (accept, token check, line pump).
//! - `state`: connected browsers and in-flight requests (pure).
//! - `rpc`: the `AppState` glue for `interaction/browserList|Execute`.
//! - `registration`: native messaging host registration (Settings › Browser).

mod commands;
pub(crate) mod listener;
// The frame codec is used by the host binary, which includes this file by path.
#[allow(dead_code)]
pub(crate) mod native;
pub(crate) mod registration;
mod rpc;
mod setup;
pub(crate) mod state;

pub(crate) use state::BrowserBridge;

/// API members the extension does not implement yet, hidden from the agent
/// through the descriptor (the manifest interpreter drops them). Recording
/// is already off by default for `extension` backends.
pub(crate) const API_SUPPORT_OVERRIDES: &[(&str, bool)] = &[
    ("BrowserUser.history", false),
    ("PlaywrightAPI.waitForEvent", false),
    ("PlaywrightDownload.path", false),
    ("PlaywrightFileChooser.setFiles", false),
    ("PlaywrightLocator.downloadMedia", false),
    ("CUAAPI.downloadMedia", false),
    ("DomCUAAPI.downloadMedia", false),
];

#[cfg(test)]
#[path = "state_tests.rs"]
mod state_tests;

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod bridge_tests;

#[cfg(test)]
#[path = "registration_tests.rs"]
mod registration_tests;
