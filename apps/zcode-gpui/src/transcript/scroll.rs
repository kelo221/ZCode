//! Continuous middle-mouse-button (wheel button) autoscroll support.
//! Uses native VSYNC-synchronized frame animation (cx.on_next_frame)
//! and bounds-clamped scrolling without synthetic DOM overlays or thread sleeps.

use crate::app::root::RootView;
use gpui::{
    Context, KeyDownEvent, Keystroke, ListOffset, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Window, px,
};
use std::sync::atomic::Ordering;

/// Deadzone radius (in pixels) around the anchor where scrolling is neutral (0 speed).
pub(crate) const AUTOSCROLL_DEADZONE: f32 = 12.0;

/// Compute pixels-per-frame scroll delta from vertical offset `dy = cursor_y - anchor_y`.
/// Returns positive for downward scroll (cursor below anchor),
/// negative for upward scroll (cursor above anchor).
pub(crate) fn autoscroll_speed(dy: f32) -> f32 {
    let abs_dy = dy.abs();
    if abs_dy <= AUTOSCROLL_DEADZONE {
        return 0.0;
    }
    let sign = dy.signum();
    let dist = abs_dy - AUTOSCROLL_DEADZONE;
    // Gentle at small offsets for comfortable reading; progressively faster with distance.
    sign * (dist * 0.08 + (dist * 0.005).powi(2)).min(28.0)
}

/// Clamps a proposed scroll delta against the current offset and bounds [0, max].
/// Returns `px(0.)` when the bounds are already reached, or the clamped delta.
pub(crate) fn clamp_scroll_distance(current: Pixels, max: Pixels, distance: Pixels) -> Pixels {
    // NaN fails both comparisons below and `f32::max` would then discard it,
    // snapping the viewport to the top.
    if distance == px(0.) || !f32::from(distance).is_finite() {
        return px(0.);
    }
    if distance > px(0.) {
        // Scrolling downward
        if max <= px(0.) || current >= max {
            return px(0.);
        }
        (max - current).min(distance).max(px(0.))
    } else {
        // Scrolling upward
        if current <= px(0.) {
            return px(0.);
        }
        (-current).max(distance).min(px(0.))
    }
}

/// Slack (px) under which the viewport counts as pinned to the bottom.
pub(crate) const BOTTOM_SLACK: f32 = 2.0;

/// Whether the transcript should keep following appends after a programmatic
/// scroll of `allowed` from `current` within `[0, max]`. Any upward movement
/// detaches; downward movement re-attaches only once the bottom is reached.
pub(crate) fn follows_after_scroll(current: Pixels, max: Pixels, allowed: Pixels) -> Option<bool> {
    if allowed < px(0.) {
        Some(false)
    } else if allowed > px(0.) {
        Some(current + allowed >= max - px(BOTTOM_SLACK))
    } else {
        None
    }
}

/// Keyboard transcript scrolling (Desktop `timelineKeyboardScrollIntent`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TranscriptKey {
    PageUp,
    PageDown,
    Top,
    Bottom,
}

/// PageUp/PageDown bare; Home/End bare or with Ctrl. Editable controls see
/// the key first, so the composer keeps Home/End for its caret.
pub(crate) fn transcript_key(keystroke: &Keystroke) -> Option<TranscriptKey> {
    let m = &keystroke.modifiers;
    if m.alt || m.shift || m.platform || m.function {
        return None;
    }
    match (keystroke.key.as_str(), m.control) {
        ("pageup", false) => Some(TranscriptKey::PageUp),
        ("pagedown", false) => Some(TranscriptKey::PageDown),
        ("home", _) => Some(TranscriptKey::Top),
        ("end", _) => Some(TranscriptKey::Bottom),
        _ => None,
    }
}

/// Overlap kept between consecutive pages so the reader keeps their place.
pub(crate) const PAGE_OVERLAP: f32 = 40.0;

/// One page of the transcript viewport, never less than half of it.
pub(crate) fn page_step(viewport: Pixels) -> Pixels {
    (viewport - px(PAGE_OVERLAP)).max(viewport * 0.5)
}

/// State tracking for middle-mouse-button autoscrolling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MiddleScrollState {
    /// Window coordinates where middle click started.
    pub anchor: Point<Pixels>,
    /// Last sampled mouse coordinates.
    pub last_pos: Point<Pixels>,
    /// Whether the middle mouse button is currently held down.
    pub button_held: bool,
}

impl RootView {
    /// Middle mouse button down: start continuous autoscroll or cancel existing mode.
    pub(crate) fn handle_middle_down(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ev.button != MouseButton::Middle {
            return;
        }
        // Second middle click cancels active autoscroll.
        if self.middle_scroll.is_some() {
            self.cancel_middle_scroll(cx);
            return;
        }
        self.middle_scroll = Some(MiddleScrollState {
            anchor: ev.position,
            last_pos: ev.position,
            button_held: true,
        });
        cx.notify();
        self.schedule_autoscroll_frame(window, cx);
    }

    /// Schedule a VSYNC-synchronized animation frame for autoscroll. At most
    /// one callback is ever queued: a cancel + restart within one frame would
    /// otherwise leave the stale callback rescheduling itself alongside the
    /// new one, compounding speed and never stopping.
    pub(crate) fn schedule_autoscroll_frame(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.autoscroll_frame_pending {
            return;
        }
        self.autoscroll_frame_pending = true;
        cx.on_next_frame(window, |this, window, cx| {
            this.autoscroll_frame_pending = false;
            this.tick_autoscroll(window, cx);
        });
    }

    /// Autoscroll step executed every display frame.
    pub(crate) fn tick_autoscroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ms) = self.middle_scroll.as_ref() else {
            return;
        };
        if self.row_index.is_empty() {
            self.cancel_middle_scroll(cx);
            return;
        }
        let dy = f32::from(ms.last_pos.y - ms.anchor.y);
        let speed = autoscroll_speed(dy);
        if speed != 0.0 {
            self.scroll_transcript_clamped(px(speed));
            cx.notify();
        }
        if self.middle_scroll.is_some() {
            self.schedule_autoscroll_frame(window, cx);
        }
    }

    /// Mouse move: update current mouse position for the continuous autoscroll loop.
    pub(crate) fn handle_middle_move(&mut self, ev: &MouseMoveEvent, _cx: &mut Context<Self>) {
        if let Some(ms) = self.middle_scroll.as_mut() {
            ms.last_pos = ev.position;
        }
    }

    /// Middle mouse button up: if dragged away from anchor, release ends scroll;
    /// if clicked in place, continues in toggle-autoscroll mode until next click.
    pub(crate) fn handle_middle_up(&mut self, ev: &MouseUpEvent, cx: &mut Context<Self>) {
        if ev.button != MouseButton::Middle {
            return;
        }
        let Some(ms) = self.middle_scroll.as_mut() else {
            return;
        };
        ms.button_held = false;
        let dist = (ev.position.y - ms.anchor.y).abs();
        if dist >= px(AUTOSCROLL_DEADZONE) {
            // Drag-and-release gesture: user held button, moved away, and released.
            self.cancel_middle_scroll(cx);
        }
    }

    /// Cancel active middle mouse scrolling and restore normal cursor.
    pub(crate) fn cancel_middle_scroll(&mut self, cx: &mut Context<Self>) {
        if self.middle_scroll.take().is_some() {
            cx.notify();
        }
    }

    /// Scroll the transcript list by `distance`, strictly clamped to [0, max_offset]
    /// to avoid overshooting past the content and triggering layout spring-back.
    pub(crate) fn scroll_transcript_clamped(&mut self, distance: Pixels) {
        let current = -self.list_state.scroll_px_offset_for_scrollbar().y;
        let max = self.list_state.max_offset_for_scrollbar().y;
        let allowed = clamp_scroll_distance(current, max, distance);
        if allowed != px(0.) {
            self.list_state.scroll_by(allowed);
        }
        // `ListState::scroll_by` bypasses the list's scroll handler, so the
        // follow flag must be maintained here or streaming appends would yank
        // the viewport back down mid-autoscroll.
        if let Some(follow) = follows_after_scroll(current, max, allowed) {
            self.follow_bottom
                .store(follow, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Wheel/touch scroll over the transcript, after the list applied it.
    /// The only writer of the follow flag for wheel input; events without
    /// vertical motion leave it alone. `delta_y > 0` is gpui's "scroll up".
    pub(crate) fn sync_follow_after_wheel(&mut self, delta_y: Pixels) {
        let follow = if delta_y > px(0.) {
            false
        } else if delta_y < px(0.) {
            let current = -self.list_state.scroll_px_offset_for_scrollbar().y;
            let max = self.list_state.max_offset_for_scrollbar().y;
            current >= max - px(BOTTOM_SLACK)
        } else {
            return;
        };
        self.follow_bottom
            .store(follow, std::sync::atomic::Ordering::Relaxed);
    }

    /// Clicking the transcript takes focus from the composer, as clicking
    /// Desktop's timeline blurs its textarea; Home/End then scroll the
    /// transcript instead of moving the caret. Inner targets that focus
    /// themselves (selectable text, card inputs) run first and are kept.
    pub(crate) fn focus_transcript_on_click(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.read(cx).phone_client.enabled
            || self.transcript_focus.contains_focused(window, cx)
        {
            return;
        }
        window.focus(&self.transcript_focus, cx);
    }

    /// Whether a key that reached the root belongs to the transcript: focus
    /// is in it, in the composer (which only lets PageUp/PageDown through),
    /// on the neutral handle, or nowhere. Dock, terminal and settings
    /// controls keep their keys.
    fn transcript_owns_keys(&self, window: &Window, cx: &Context<Self>) -> bool {
        if self.settings.open || self.row_index.is_empty() {
            return false;
        }
        let compact = crate::app::mobile_shell::is_compact(f32::from(window.viewport_size().width));
        if compact && self.mobile_page != crate::app::mobile_shell::MobilePage::Chat {
            return false;
        }
        window.focused(cx).is_none()
            || self.transcript_focus.contains_focused(window, cx)
            || self.settings.focus.is_focused(window)
            || self
                .state
                .read(cx)
                .composer
                .read(cx)
                .focus
                .contains_focused(window, cx)
    }

    /// PageUp/PageDown/Home/End over the transcript. Returns whether the key was used.
    pub(crate) fn handle_transcript_scroll_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(key) = transcript_key(&event.keystroke) else {
            return false;
        };
        if !self.transcript_owns_keys(window, cx) {
            return false;
        }
        self.cancel_middle_scroll(cx);
        self.scroll_transcript_by_key(key);
        cx.notify();
        true
    }

    /// Keyboard paging works in rows, not `scroll_px_offset_for_scrollbar`:
    /// that offset only sums measured rows, so above the overdraw window it
    /// under-reports and a pixel clamp would cut pages short.
    pub(crate) fn scroll_transcript_by_key(&mut self, key: TranscriptKey) {
        let top = self.list_state.logical_scroll_top();
        // Content that fits the viewport stays at row 0 and keeps following.
        let at_top = top.item_ix == 0 && top.offset_in_item <= px(0.);
        match key {
            TranscriptKey::PageUp | TranscriptKey::Top if at_top => {}
            TranscriptKey::PageUp => {
                let viewport = self.list_state.viewport_bounds().size.height;
                if viewport > px(0.) {
                    self.list_state.scroll_by(-page_step(viewport));
                    self.follow_bottom.store(false, Ordering::Relaxed);
                }
            }
            TranscriptKey::Top => {
                self.list_state.scroll_to(ListOffset {
                    item_ix: 0,
                    offset_in_item: px(0.),
                });
                self.follow_bottom.store(false, Ordering::Relaxed);
            }
            TranscriptKey::PageDown => {
                let viewport = self.list_state.viewport_bounds();
                if viewport.size.height <= px(0.) {
                    return;
                }
                self.list_state.scroll_by(page_step(viewport.size.height));
                let last = self.row_index.len() - 1;
                let next = self.list_state.logical_scroll_top();
                // Past the last row, or its bottom inside the viewport: back at the tail.
                let at_tail = next.item_ix > last
                    || self
                        .list_state
                        .bounds_for_item(last)
                        .is_some_and(|b| b.bottom() <= viewport.bottom() + px(BOTTOM_SLACK));
                if at_tail {
                    self.scroll_transcript_to_tail();
                } else {
                    self.follow_bottom.store(false, Ordering::Relaxed);
                }
            }
            TranscriptKey::Bottom => self.scroll_transcript_to_tail(),
        }
    }

    pub(crate) fn scroll_transcript_to_tail(&mut self) {
        if let Some(pin) = crate::transcript::list::tail_pin(self.row_index.len()) {
            self.list_state.scroll_to(pin);
        }
        self.follow_bottom.store(true, Ordering::Relaxed);
    }
}
