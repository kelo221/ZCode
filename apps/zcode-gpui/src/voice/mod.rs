//! Voice: agents speak to the user on the desktop (spec: VOICE.md).
//!
//! `VoiceService` lives on `AppState` only in the desktop branch, so the
//! phone build runs none of it; reverse RPCs never reach the phone anyway.
//!
//! Event order for one Speak call: agent `interaction/speak` (stdio) ->
//! `route_voice_rpc` checks settings and focus -> marks the session as spoken
//! this turn -> sends `{"op":"speak"}` to the sidecar (spawned lazily) ->
//! answers `{accepted}` at once; sidecar events update `status` afterwards.

pub(crate) mod process;
mod rpc;
#[cfg(test)]
mod rpc_tests;
pub(crate) mod rules;

use crate::shared::voice_settings::VoiceSettings;
use gpui::App;
use process::Sidecar;
use std::time::{Duration, Instant};

/// Unload the sidecar (and its ~450 MB working set) this long after its last
/// line finished. A restart reloads the model from the OS file cache in about
/// 0.6 s, so keeping it resident buys little; the grace period only lets a
/// Speak call and a fallback line in the same moment share one load.
pub(crate) const IDLE_UNLOAD: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VoiceStatus {
    NotStarted,
    /// No `zcode-voice` executable was found.
    Missing,
    Loading,
    Downloading {
        file: String,
        percent: u32,
    },
    Ready,
    Failed {
        code: String,
        message: String,
    },
}

pub(crate) struct VoiceService {
    pub(crate) status: VoiceStatus,
    /// Voices the running sidecar reported; empty until `ready`.
    pub(crate) voices: Vec<String>,
    pub(crate) window_active: bool,
    pub(crate) ledger: rules::TurnLedger,
    sidecar: Option<Sidecar>,
    generation: u64,
    next_id: u64,
    /// Lines sent to the sidecar that haven't been spoken, dropped or failed.
    /// The sidecar is never unloaded while this is non-zero, which also
    /// covers the first-run model download.
    pending: u32,
    last_activity: Instant,
}

impl VoiceService {
    pub(crate) fn new() -> Self {
        Self {
            status: VoiceStatus::NotStarted,
            voices: Vec::new(),
            window_active: true,
            ledger: Default::default(),
            sidecar: None,
            generation: 0,
            next_id: 0,
            pending: 0,
            last_activity: Instant::now(),
        }
    }

    pub(crate) fn running(&self) -> bool {
        self.sidecar.is_some()
    }

    pub(crate) fn shutdown(&mut self) {
        self.sidecar = None;
        self.pending = 0;
        if !matches!(
            self.status,
            VoiceStatus::Failed { .. } | VoiceStatus::Missing
        ) {
            self.status = VoiceStatus::NotStarted;
        }
    }

    fn idle_expired(&self, now: Instant) -> bool {
        idle_due(self.running(), self.pending, self.last_activity, now)
    }

    /// Applies one sidecar event of the current generation.
    fn apply_event(&mut self, event: &serde_json::Value) {
        let field = |k: &str| {
            event
                .get(k)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        match event.get("event").and_then(|v| v.as_str()).unwrap_or("") {
            "loading" => self.status = VoiceStatus::Loading,
            "downloading" => {
                self.status = VoiceStatus::Downloading {
                    file: field("file"),
                    percent: event.get("percent").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                }
            }
            "ready" => {
                self.status = VoiceStatus::Ready;
                self.voices = event
                    .get("voices")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
            }
            // Per-job errors (with an id) keep the engine state; lifecycle
            // errors (no id) mean the engine itself can't speak.
            "error" if event.get("id").is_none() => {
                self.status = VoiceStatus::Failed {
                    code: field("code"),
                    message: field("message"),
                }
            }
            "spoken" | "dropped" | "error" => self.finish_job(),
            "started" => self.last_activity = Instant::now(),
            _ => {}
        }
    }

    fn finish_job(&mut self) {
        self.pending = self.pending.saturating_sub(1);
        self.last_activity = Instant::now();
    }
}

fn idle_due(running: bool, pending: u32, last_activity: Instant, now: Instant) -> bool {
    running && pending == 0 && now.saturating_duration_since(last_activity) >= IDLE_UNLOAD
}

/// The committed voice settings (defaults when unset or no owner exists).
pub(crate) fn settings(cx: &App) -> VoiceSettings {
    cx.try_global::<crate::shared::preferences::PreferenceOwner>()
        .and_then(|owner| owner.0.read(cx).snapshot.gpui_voice.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lifecycle_events_drive_status() {
        let mut service = VoiceService::new();
        service.apply_event(&json!({"event":"loading"}));
        assert_eq!(service.status, VoiceStatus::Loading);
        service.apply_event(&json!({"event":"downloading","file":"0.onnx","percent":42}));
        assert_eq!(
            service.status,
            VoiceStatus::Downloading {
                file: "0.onnx".into(),
                percent: 42
            }
        );
        service.apply_event(&json!({"event":"ready","voices":["af_sky","bm_george"]}));
        assert_eq!(service.status, VoiceStatus::Ready);
        assert_eq!(service.voices, ["af_sky", "bm_george"]);
    }

    #[test]
    fn only_lifecycle_errors_fail_the_engine() {
        let mut service = VoiceService::new();
        service.apply_event(&json!({"event":"ready","voices":[]}));
        service
            .apply_event(&json!({"event":"error","id":"v1","code":"synth_failed","message":"x"}));
        assert_eq!(service.status, VoiceStatus::Ready);
        service.apply_event(&json!({"event":"error","code":"espeak_missing","message":"no data"}));
        assert_eq!(
            service.status,
            VoiceStatus::Failed {
                code: "espeak_missing".into(),
                message: "no data".into()
            }
        );
    }

    #[test]
    fn idle_expiry_needs_a_running_sidecar() {
        let service = VoiceService::new();
        assert!(!service.idle_expired(Instant::now() + IDLE_UNLOAD * 2));
    }

    #[test]
    fn finished_lines_count_down_without_underflow() {
        let mut service = VoiceService::new();
        service.pending = 2;
        service.apply_event(&json!({"event":"spoken","id":"v1"}));
        assert_eq!(service.pending, 1);
        service
            .apply_event(&json!({"event":"error","id":"v2","code":"synth_failed","message":"x"}));
        assert_eq!(service.pending, 0);
        service.apply_event(&json!({"event":"dropped","id":"late"}));
        assert_eq!(service.pending, 0);
    }

    #[test]
    fn unloads_only_when_running_drained_and_quiet() {
        let last = Instant::now();
        let due = last + IDLE_UNLOAD;
        assert!(idle_due(true, 0, last, due));
        assert!(!idle_due(true, 0, last, due - Duration::from_secs(1)));
        // A queued, playing or still-downloading line keeps it loaded.
        assert!(!idle_due(true, 1, last, due + IDLE_UNLOAD * 10));
        assert!(!idle_due(false, 0, last, due));
    }
}
