//! Virtualized transcript: renders only the visible row window through gpui's
//! `list` element (PARITY.md M2 "Virtualization: 10k rows smooth"). Owns the
//! cached row index, follow-bottom tracking and scroll anchoring.

use crate::app::root::RootView;
use gpui::{
    AnyElement, Context, CursorStyle, InteractiveElement, IntoElement, ListAlignment, ListOffset,
    ListSizingBehavior, ListState, MouseButton, MouseDownEvent, ParentElement, ScrollWheelEvent,
    Styled, div, px,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) const OVERDRAW: f32 = 800.;

/// Create the shared list state plus the follow-bottom flag it maintains.
/// The scroll handler records whether the viewport bottom touches the last
/// row; appends only auto-scroll when it did. It must only read the event —
/// gpui fires the handler while holding the list's internal `borrow_mut`, so
/// any call back into `ListState` (e.g. `item_count`) re-borrows and panics.
/// `ListScrollEvent.count` is the total item count despite its doc comment.
pub(crate) fn new_list_state() -> (ListState, Arc<AtomicBool>) {
    let follow = Arc::new(AtomicBool::new(true));
    let state = ListState::new(0, ListAlignment::Top, px(OVERDRAW));
    let follow_flag = follow.clone();
    state.set_scroll_handler(move |ev, _window, _cx| {
        let at_bottom = ev.visible_range.end >= ev.count;
        follow_flag.store(at_bottom, Ordering::Relaxed);
    });
    (state, follow)
}

/// How the cached row index changes between two renders.
#[derive(Debug, PartialEq)]
pub(crate) enum ListEdit {
    /// Same ids: nothing to splice (streaming may still grow the last row).
    Unchanged,
    /// `n` rows appended at the tail.
    Append(usize),
    /// `n` older rows paged in at the head (history).
    Prepend(usize),
    /// Anything else: rebuild the index.
    Reset,
}

pub(crate) fn plan_list_edit(prev: &[u64], next: &[u64], session_changed: bool) -> ListEdit {
    if session_changed {
        ListEdit::Reset
    } else if prev == next {
        ListEdit::Unchanged
    } else if next.len() > prev.len() && next.starts_with(prev) {
        ListEdit::Append(next.len() - prev.len())
    } else if !prev.is_empty() && next.len() > prev.len() && next.ends_with(prev) {
        ListEdit::Prepend(next.len() - prev.len())
    } else {
        ListEdit::Reset
    }
}

/// `offset_in_item` used to pin the tail. Larger than any row, so gpui's
/// layout pass ("rendered items do not fill the visible region") walks
/// upward and bottom-aligns the last row, even one taller than the
/// viewport. `scroll_to_reveal_item` would instead snap a tall last row to
/// its top when it already starts the viewport.
const PIN_TAIL_OFFSET: f32 = 1.0e7;

pub(crate) fn tail_pin(row_count: usize) -> Option<ListOffset> {
    (row_count > 0).then(|| ListOffset {
        item_ix: row_count - 1,
        offset_in_item: px(PIN_TAIL_OFFSET),
    })
}

impl RootView {
    /// Reconcile the cached row index with the conversation model. Called
    /// every render before the element tree is built.
    pub(crate) fn sync_list(&mut self, _row_count: usize, first_row_id: u64, cx: &Context<Self>) {
        let (active_sid, last_id, count) = {
            let state = self.state.read(cx);
            let conv = state.active_conversation();
            (
                state.active.clone(),
                conv.and_then(|c| c.rows.keys().next_back().copied())
                    .unwrap_or(0),
                conv.map_or(0, |c| c.rows.len()),
            )
        };
        // Row ids restart at 1 per conversation, so a session switch invalidates
        // the whole cached index even when counts/prefixes coincide.
        let session_changed = active_sid != self.last_session;
        let follow = self.follow_bottom.load(Ordering::Relaxed);
        if !session_changed
            && count == self.last_row_count
            && (first_row_id, last_id) == self.last_bounds
        {
            // Same rows, but a streaming row may have grown: keep the tail
            // pinned while following.
            if follow && let Some(pin) = tail_pin(count) {
                self.list_state.scroll_to(pin);
            }
            return;
        }

        let ids: Vec<u64> = self
            .state
            .read(cx)
            .active_conversation()
            .map(|c| c.rows.keys().copied().collect())
            .unwrap_or_default();
        // The index is the single source of the list's item count.
        let row_count = ids.len();
        let prev_count = self.row_index.len();
        // `reset` discards the logical scroll top, so capture it first.
        let prev_top = self.list_state.logical_scroll_top();
        let edit = plan_list_edit(&self.row_index, &ids, session_changed);
        match edit {
            ListEdit::Unchanged => {}
            ListEdit::Append(n) => self.list_state.splice(prev_count..prev_count, n),
            // Splicing at the head keeps measured heights and shifts the
            // logical scroll top by `n`, so the visible row stays in place
            // without the flash a full `reset` (all rows unmeasured) causes.
            ListEdit::Prepend(n) => self.list_state.splice(0..0, n),
            ListEdit::Reset => self.list_state.reset(row_count),
        }

        self.row_index = ids;
        self.last_row_count = row_count;
        self.last_first_row_id = first_row_id;
        self.last_bounds = (first_row_id, last_id);
        self.last_session = active_sid;
        if session_changed {
            // A new conversation opens pinned to its tail; an in-flight
            // autoscroll belongs to the old list.
            self.follow_bottom.store(true, Ordering::Relaxed);
            self.middle_scroll = None;
        }
        if row_count == 0 {
            return;
        }

        // These only mutate list state (consumed by the next layout), so they
        // are safe to call here during render.
        if session_changed || prev_count == 0 || follow {
            if let Some(pin) = tail_pin(row_count) {
                self.list_state.scroll_to(pin);
            }
        } else if edit == ListEdit::Reset {
            // Replacement/truncation while reading history: stay on the same
            // row (clamped) instead of snapping to the top after `reset`.
            let clamped = prev_top.item_ix >= row_count;
            self.list_state.scroll_to(ListOffset {
                item_ix: prev_top.item_ix.min(row_count - 1),
                offset_in_item: if clamped {
                    px(0.)
                } else {
                    prev_top.offset_in_item
                },
            });
        }
    }

    /// The virtualized row window, centered on the reading column.
    pub(crate) fn transcript_list(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let state = self.list_state.clone();
        let is_middle_scrolling = self.middle_scroll.is_some();
        div()
            .id("transcript-scroll-area")
            .size_full()
            .flex_1()
            .min_h_0()
            .cursor(if is_middle_scrolling {
                CursorStyle::ResizeUpDown
            } else {
                CursorStyle::Arrow
            })
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    this.handle_middle_down(ev, window, cx);
                }),
            )
            // Move/release/cancel for autoscroll are bound on the root view so
            // they still fire once the pointer leaves the transcript.
            .on_scroll_wheel(cx.listener(|this, ev: &ScrollWheelEvent, _window, _cx| {
                this.sync_follow_after_wheel(ev.delta.pixel_delta(px(20.)).y);
            }))
            .child(
                gpui::list(state, move |ix, _window, app| {
                    entity.update(app, |view, cx| view.render_virtual_row(ix, cx))
                })
                .with_sizing_behavior(ListSizingBehavior::Auto)
                .size_full(),
            )
            .into_any_element()
    }

    /// Single row lookup + render for the virtual list's render callback.
    pub(crate) fn render_virtual_row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(&row_id) = self.row_index.get(ix) else {
            return div().into_any_element();
        };
        let state = self.state.read(cx);
        let Some(row) = state
            .active_conversation()
            .and_then(|c| c.rows.get(&row_id))
            .cloned()
        else {
            return div().into_any_element();
        };
        let row_actions = !state.is_read_only_view()
            && state
                .active_conversation()
                .is_some_and(|c| c.log_epoch.is_some() && c.subscribed);
        let content = self
            .transcript_row(&row, row_actions, cx)
            .unwrap_or_else(|| div().into_any_element());
        div()
            .w_full()
            .max_w(px(crate::app::root::CONTENT_WIDTH))
            .px_6()
            .py_2()
            .child(content)
            .into_any_element()
    }
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
