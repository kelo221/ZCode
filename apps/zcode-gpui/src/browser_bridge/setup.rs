//! `AppState` actions behind Settings › Browser: read, register and remove
//! the native messaging host registration.

use super::native::bridge_dir;
use super::registration::{self, SystemRegistry};
use crate::app::store::AppState;
use gpui::Context;

/// Registration is per user (HKCU and the real home), which a scratch
/// instance must leave alone.
const SCRATCH: &str = "registration is unavailable in scratch instances";

impl AppState {
    pub(crate) fn refresh_browser_registration(&mut self, cx: &mut Context<Self>) {
        self.browser.registration =
            registration::host_exe_path().map(|host| registration::status(&SystemRegistry, &host));
        cx.notify();
    }

    pub(crate) fn register_browser_host(&mut self, cx: &mut Context<Self>) {
        let outcome = match (bridge_dir(), registration::host_exe_path()) {
            _ if crate::shared::isolation::active().is_some() => Err(SCRATCH.into()),
            (Some(dir), Some(host)) => registration::register(&SystemRegistry, &dir, &host),
            _ => Err("no home directory or executable path".into()),
        };
        self.finish_registration("registered", outcome, cx);
    }

    pub(crate) fn unregister_browser_host(&mut self, cx: &mut Context<Self>) {
        let outcome = match bridge_dir() {
            _ if crate::shared::isolation::active().is_some() => Err(SCRATCH.into()),
            Some(dir) => registration::unregister(&SystemRegistry, &dir),
            None => Err("no home directory".into()),
        };
        self.finish_registration("unregistered", outcome, cx);
    }

    fn finish_registration(
        &mut self,
        done: &str,
        outcome: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            Ok(()) => {
                self.push_log(format!("browser host {done}"));
                self.browser.registration_error = None;
            }
            Err(error) => self.browser.registration_error = Some(error),
        }
        self.refresh_browser_registration(cx);
    }
}
