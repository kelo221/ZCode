//! Tests for `conversation/auto_compact.rs`: idle tracking, eligibility and
//! the in-flight steps.

use super::*;
use crate::conversation::context_usage::ContextWindow;
use serde_json::json;

const ON: AutoCompactEntry = AutoCompactEntry {
    enabled: true,
    minutes: 50,
};

fn window(used: f64) -> Option<ContextWindow> {
    Some(ContextWindow {
        used,
        max: 200_000.,
        cache_hit_rate: None,
        segments: Vec::new(),
    })
}

/// Idle since `t0` with a big context.
fn idle(t0: Instant) -> ConversationState {
    let mut c = ConversationState {
        context_window: window(80_000.),
        ..Default::default()
    };
    c.note_control(false, false, t0);
    c
}

fn later(t0: Instant, minutes: u64) -> Instant {
    t0 + Duration::from_secs(minutes * 60)
}

#[test]
fn leaving_the_active_phase_starts_the_idle_clock_once() {
    let t0 = Instant::now();
    let mut c = ConversationState::default();
    c.note_control(true, false, t0);
    assert_eq!(c.idle_since, None);
    c.note_control(false, false, t0);
    assert_eq!(c.idle_since, Some(t0));
    // Later idle updates keep the first idle instant.
    c.note_control(false, false, later(t0, 1));
    assert_eq!(c.idle_since, Some(t0));
}

#[test]
fn a_new_run_clears_idle_and_the_compacted_flag() {
    let t0 = Instant::now();
    let mut c = idle(t0);
    c.auto_compacted = true;
    c.note_control(true, false, later(t0, 1));
    assert_eq!(c.idle_since, None);
    assert!(!c.auto_compacted);
}

#[test]
fn control_frames_through_apply_control_track_the_phase() {
    let mut c = ConversationState::default();
    c.apply_control(&json!({ "phase": "running" }));
    assert_eq!(c.idle_since, None);
    c.apply_control(&json!({ "phase": "idle" }));
    assert!(c.idle_since.is_some());
}

#[test]
fn due_after_the_configured_minutes_only() {
    let t0 = Instant::now();
    let c = idle(t0);
    assert!(!c.auto_compact_due(ON, later(t0, 49)));
    assert!(c.auto_compact_due(ON, later(t0, 50)));
}

#[test]
fn not_due_when_disabled() {
    let t0 = Instant::now();
    let off = AutoCompactEntry {
        enabled: false,
        ..ON
    };
    assert!(!idle(t0).auto_compact_due(off, later(t0, 500)));
}

#[test]
fn not_due_below_the_token_floor_or_without_usage() {
    let t0 = Instant::now();
    let mut c = idle(t0);
    c.context_window = window(TOKEN_FLOOR - 1.);
    assert!(!c.auto_compact_due(ON, later(t0, 60)));
    c.context_window = None;
    assert!(!c.auto_compact_due(ON, later(t0, 60)));
}

#[test]
fn not_due_twice_in_one_idle_stretch() {
    let t0 = Instant::now();
    let mut c = idle(t0);
    c.begin_auto_compact();
    c.auto_compact = None; // finished
    assert!(!c.auto_compact_due(ON, later(t0, 120)));
    // New activity re-arms it.
    c.note_control(true, false, later(t0, 121));
    c.note_control(false, false, later(t0, 122));
    assert!(c.auto_compact_due(ON, later(t0, 172)));
}

#[test]
fn not_due_while_running_or_with_live_work() {
    let t0 = Instant::now();
    let mut c = idle(t0);
    c.phase = "running".into();
    c.note_control(true, false, later(t0, 60));
    assert!(!c.auto_compact_due(ON, later(t0, 60)));

    let mut c = idle(t0);
    c.queue = crate::conversation::queue::QueueState::from_value(&json!({
        "items": [{ "queueItemId": "q1", "text": "next" }]
    }));
    assert!(c.queue.as_ref().is_some_and(|q| !q.items.is_empty()));
    assert!(!c.auto_compact_due(ON, later(t0, 60)));
}

#[test]
fn the_compaction_itself_is_not_new_activity() {
    let t0 = Instant::now();
    let mut c = idle(t0);
    c.begin_auto_compact();
    c.auto_compact = Some(AutoCompactFlight::Sent);
    // The CLI may report the work before the ack arrives.
    c.note_control(false, true, later(t0, 50));
    assert!(matches!(
        c.auto_compact,
        Some(AutoCompactFlight::Running {
            seen_work: true,
            ..
        })
    ));
    c.note_auto_compact_ack(true, later(t0, 50));
    assert!(matches!(
        c.auto_compact,
        Some(AutoCompactFlight::Running {
            seen_work: true,
            ..
        })
    ));
    c.note_control(false, false, later(t0, 51));
    assert_eq!(c.auto_compact, None);
    assert!(c.auto_compacted);
    assert!(!c.auto_compact_due(ON, later(t0, 200)));
}

#[test]
fn ack_moves_sent_to_running_and_rejection_ends_the_flight() {
    let t0 = Instant::now();
    let mut c = idle(t0);
    c.begin_auto_compact();
    c.auto_compact = Some(AutoCompactFlight::Sent);
    c.note_auto_compact_ack(true, t0);
    assert_eq!(
        c.auto_compact,
        Some(AutoCompactFlight::Running {
            seen_work: false,
            since: t0
        })
    );
    c.auto_compact = Some(AutoCompactFlight::Sent);
    c.note_auto_compact_ack(false, t0);
    assert_eq!(c.auto_compact, None);
    assert!(c.auto_compacted, "no retry until new activity");
}

#[test]
fn a_run_starting_while_resuming_cancels_the_compaction() {
    let t0 = Instant::now();
    let mut c = idle(t0);
    c.begin_auto_compact();
    c.note_control(true, false, later(t0, 51));
    assert_eq!(c.auto_compact, None);
    assert!(!c.auto_compacted);
}
