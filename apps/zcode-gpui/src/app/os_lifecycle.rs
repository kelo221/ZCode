//! OS lifecycle hooks for RootView (window bounds persistence, keep-awake, notifications).

use crate::shared::os::keep_awake::KeepAwakeGuard;
use crate::shared::os::notifications::notify_task_completed;
use crate::shared::window_state::{WindowState, save_window_state};
use gpui::{Bounds, Pixels, Window};

#[derive(Default)]
pub struct OsLifecycleState {
    pub keep_awake: Option<KeepAwakeGuard>,
    pub last_saved_bounds: Option<(Bounds<Pixels>, bool)>,
}

/// Persist window geometry when the bounds or maximized state change.
pub fn sync_window_state(window: &Window, last_saved: &mut Option<(Bounds<Pixels>, bool)>) {
    let current_bounds = window.bounds();
    let is_maximized = window.is_maximized();
    let current = (current_bounds, is_maximized);

    if last_saved.as_ref() != Some(&current) {
        *last_saved = Some(current);
        let ws = WindowState::from_bounds(current_bounds, is_maximized);
        let _ = save_window_state(&ws);
    }
}

/// Keep the system awake while an agent turn is actively executing.
pub fn sync_keep_awake(guard: &mut Option<KeepAwakeGuard>, is_running: bool) {
    if is_running && guard.is_none() {
        *guard = Some(KeepAwakeGuard::acquire());
    } else if !is_running && guard.is_some() {
        *guard = None;
    }
}

/// Notify user on turn completion when the application window is in background.
pub fn check_turn_completion_notification(
    last_phase: &str,
    current_phase: &str,
    title: &str,
    is_window_focused: bool,
) {
    if last_phase != current_phase && current_phase == "completedSuccess" {
        notify_task_completed(title, "Task completed successfully", is_window_focused);
    }
}
