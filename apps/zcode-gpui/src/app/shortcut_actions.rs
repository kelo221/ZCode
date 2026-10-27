//! Window shortcuts beyond the basic set in `navigation.rs`: toggle sidebar,
//! zoom, close the current context and previous/next conversation.

use crate::app::root::RootView;
use crate::app::store::AppState;
use crate::backend::workspace::WorkspacePurpose;
use crate::sessions::items::{filter_session_indices, latest_activity, sort_by_latest_activity};
use crate::shared::preferences::{PreferenceChange, Preferences};
use crate::shared::theme::{
    DEFAULT_UI_FONT_SIZE, MAX_UI_FONT_SIZE, MIN_UI_FONT_SIZE, font_size_base,
};
use gpui::{Context, Window};

/// Interface font size after a zoom step (`None` resets).
pub(crate) fn zoom_target(base: f32, step: Option<f32>) -> f32 {
    match step {
        Some(step) => (base + step).clamp(MIN_UI_FONT_SIZE, MAX_UI_FONT_SIZE),
        None => DEFAULT_UI_FONT_SIZE,
    }
}

/// One sidebar section: (is the Tasks section, latest activity, conversations).
type Section = (bool, Option<i64>, Vec<(String, String)>);

/// Every listed conversation as (workspace key, session id), in sidebar
/// order: the Tasks section, then projects by latest activity.
pub(crate) fn conversation_order(state: &AppState) -> Vec<(String, String)> {
    let child_sids = state.all_child_session_ids();
    let mut groups: Vec<Section> = state
        .workspaces
        .iter()
        .map(|w| {
            let indices = filter_session_indices(&w.sessions, &child_sids, "");
            let rows = indices
                .iter()
                .map(|&i| (w.key.clone(), w.sessions[i].session_id.clone()))
                .collect();
            let tasks = w.purpose == WorkspacePurpose::Conversation;
            (tasks, latest_activity(&w.sessions, &indices), rows)
        })
        .collect();
    let (mut tasks, mut projects): (Vec<_>, Vec<_>) = groups.drain(..).partition(|g| g.0);
    sort_by_latest_activity(&mut projects, |g| g.1);
    tasks.append(&mut projects);
    tasks.into_iter().flat_map(|g| g.2).collect()
}

/// Index of the conversation next to `active`, wrapping at the ends.
pub(crate) fn adjacent(
    order: &[(String, String)],
    active: Option<&str>,
    forward: bool,
) -> Option<usize> {
    if order.is_empty() {
        return None;
    }
    let len = order.len();
    let Some(at) = active.and_then(|sid| order.iter().position(|(_, s)| s == sid)) else {
        return Some(if forward { 0 } else { len - 1 });
    };
    Some(if forward {
        (at + 1) % len
    } else {
        (at + len - 1) % len
    })
}

impl RootView {
    /// The phone layout always shows its sidebar page; hiding is desktop-only.
    pub(crate) fn sidebar_hidden(&self, compact: bool) -> bool {
        !compact && self.folders.sidebar_hidden
    }

    pub(crate) fn toggle_sidebar(&mut self) {
        self.folders.sidebar_hidden = !self.folders.sidebar_hidden;
    }

    pub(crate) fn zoom(&mut self, step: Option<f32>, cx: &mut Context<Self>) {
        let base = font_size_base();
        let next = zoom_target(base, step);
        if next != base {
            Preferences::enqueue(PreferenceChange::FontSize(next), cx);
        }
    }

    /// Desktop `getCloseActiveContextSidePaneTab`: closes the open side pane.
    pub(crate) fn close_active_context(&mut self) -> bool {
        std::mem::take(&mut self.dock_open)
    }

    pub(crate) fn select_adjacent_conversation(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.read(cx);
        let order = conversation_order(state);
        let Some(ix) = adjacent(&order, state.active.as_deref(), forward) else {
            return;
        };
        let (ws, sid) = order[ix].clone();
        self.close_settings(window, cx);
        self.show_chat_page();
        self.state
            .update(cx, |s, cx| s.select_session(&ws, &sid, cx));
    }
}

#[cfg(test)]
#[path = "shortcut_actions_tests.rs"]
mod tests;
