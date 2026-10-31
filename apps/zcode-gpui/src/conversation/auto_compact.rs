//! Idle tracking and in-flight state of GPUI's background auto-compaction
//! (SETTINGS.md "Background auto-compaction"). The maintenance tick that
//! starts and drives it lives in `app/auto_compact.rs`.

use crate::conversation::model::ConversationState;
use crate::shared::auto_compact_settings::AutoCompactEntry;
use std::time::{Duration, Instant};

/// Below this the next turn is cheap anyway; compacting would only lose detail.
pub(crate) const TOKEN_FLOOR: f64 = 20_000.;

/// One auto-compaction in flight for a conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoCompactFlight {
    /// Waiting for the agent and the conversation subscription (which
    /// cold-resumes the session in the CLI). Each step is tried once.
    Resuming {
        spawn_requested: bool,
        subscribe_sent: bool,
    },
    /// `compact` written; waiting for its ack.
    Sent,
    /// Accepted at `since`; `seen_work` once `control.activeWorks` listed it.
    Running { seen_work: bool, since: Instant },
}

impl ConversationState {
    /// Called on every `control` update with the new phase and whether a
    /// compaction is among its active works.
    pub(crate) fn note_control(&mut self, active: bool, compact_work: bool, now: Instant) {
        if active {
            self.idle_since = None;
            match self.auto_compact {
                // A run started before the compact went out: new activity wins.
                Some(AutoCompactFlight::Resuming { .. }) => {
                    self.auto_compact = None;
                    self.auto_compacted = false;
                }
                None => self.auto_compacted = false,
                _ => {}
            }
        } else {
            self.idle_since.get_or_insert(now);
        }
        match self.auto_compact {
            Some(AutoCompactFlight::Sent | AutoCompactFlight::Running { .. }) if compact_work => {
                let since = match self.auto_compact {
                    Some(AutoCompactFlight::Running { since, .. }) => since,
                    _ => now,
                };
                self.auto_compact = Some(AutoCompactFlight::Running {
                    seen_work: true,
                    since,
                });
            }
            Some(AutoCompactFlight::Running {
                seen_work: true, ..
            }) if !compact_work => self.auto_compact = None,
            _ => {}
        }
    }

    /// The compact ack: accepted moves `Sent` on; anything else ends the flight.
    pub(crate) fn note_auto_compact_ack(&mut self, accepted: bool, now: Instant) {
        if !accepted {
            self.auto_compact = None;
        } else if self.auto_compact == Some(AutoCompactFlight::Sent) {
            self.auto_compact = Some(AutoCompactFlight::Running {
                seen_work: false,
                since: now,
            });
        }
    }

    /// Whether the maintenance tick should start an auto-compaction now.
    pub(crate) fn auto_compact_due(&self, entry: AutoCompactEntry, now: Instant) -> bool {
        let wait = Duration::from_secs(u64::from(entry.minutes) * 60);
        entry.enabled
            && !self.auto_compacted
            && self.auto_compact.is_none()
            && self
                .idle_since
                .is_some_and(|t| now.saturating_duration_since(t) >= wait)
            && self
                .context_window
                .as_ref()
                .is_some_and(|w| w.used >= TOKEN_FLOOR)
            && !crate::app::conversation_release::has_live_work(self)
    }

    /// Mark the compaction started; it won't fire again until a new run.
    pub(crate) fn begin_auto_compact(&mut self) {
        self.auto_compacted = true;
        self.auto_compact = Some(AutoCompactFlight::Resuming {
            spawn_requested: false,
            subscribe_sent: false,
        });
    }
}

#[cfg(test)]
#[path = "auto_compact_tests.rs"]
mod tests;
