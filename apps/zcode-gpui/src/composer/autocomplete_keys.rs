//! Keyboard control of the composer's suggestion popup (slash commands and
//! @ mentions). Desktop parity (prompt-editor/ChatPromptActionMenu.tsx): Up/Down
//! move the highlight, Tab takes the highlighted suggestion.
//!
//! Tab is bound app-wide to ely's `FocusNext`, and gpui dispatches a matched
//! binding's action before any key-down listener, so the composer never sees a
//! Tab key event. The composer container therefore captures `FocusNext` while
//! the popup is open and lets it through otherwise.

use crate::app::root::RootView;
use ely_gpui_component::primitives::FocusNext;
use gpui::{Context, Div, InteractiveElement, KeyDownEvent, Window};

/// The highlighted row for `text`: the stored pick while the text it was made
/// for is unchanged, else the first row.
pub(crate) fn highlighted(pick: &(String, usize), text: &str, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    if pick.0 == text {
        pick.1.min(len - 1)
    } else {
        0
    }
}

/// `from` moved by `delta`, wrapping within `len` rows.
pub(crate) fn step(from: usize, delta: isize, len: usize) -> usize {
    (from as isize + delta).rem_euclid(len as isize) as usize
}

impl RootView {
    /// Whether keys typed now are meant for the popup: the composer has focus
    /// and no IME composition is in progress.
    fn popup_keys_live(&self, window: &Window, cx: &Context<Self>) -> bool {
        let composer = self.state.read(cx).composer.read(cx);
        composer.focus.is_focused(window) && composer.marked_utf16.is_none()
    }

    pub(crate) fn autocomplete_highlight(&self, len: usize, cx: &Context<Self>) -> usize {
        let state = self.state.read(cx);
        highlighted(&self.autocomplete_pick, state.composer.read(cx).text(), len)
    }

    fn move_autocomplete(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        let Some(suggestions) = self.autocomplete_suggestions(cx) else {
            return false;
        };
        let current = self.autocomplete_highlight(suggestions.len(), cx);
        let text = self.state.read(cx).composer.read(cx).text().to_string();
        self.autocomplete_pick = (text, step(current, delta, suggestions.len()));
        cx.notify();
        true
    }

    fn accept_autocomplete(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(suggestions) = self.autocomplete_suggestions(cx) else {
            return false;
        };
        let pick = self.autocomplete_highlight(suggestions.len(), cx);
        self.autocomplete_pick = Default::default();
        self.apply_autocomplete(&suggestions[pick], cx);
        true
    }

    /// Tab / Up / Down handling for the composer container.
    pub(crate) fn autocomplete_keys(&self, container: Div, cx: &mut Context<Self>) -> Div {
        container
            .capture_action(cx.listener(|this, _: &FocusNext, window, cx| {
                if this.popup_keys_live(window, cx) && this.accept_autocomplete(cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                let m = &ev.keystroke.modifiers;
                if m.shift || m.control || m.alt || m.platform || !this.popup_keys_live(window, cx)
                {
                    return;
                }
                let delta = match ev.keystroke.key.as_str() {
                    "down" => 1,
                    "up" => -1,
                    _ => return,
                };
                if this.move_autocomplete(delta, cx) {
                    cx.stop_propagation();
                }
            }))
    }
}

#[cfg(test)]
#[path = "autocomplete_keys_tests.rs"]
mod tests;
