//! Sidebar row and filtering items.

use crate::shared::theme::{HOVER, SELECTED};
use gpui::{CursorStyle, SharedString, div, prelude::*, px, rgb};

pub(crate) const ROW_GROUP: &str = "session-row";

pub(crate) fn format_relative_time(last_activity_ms: Option<i64>) -> String {
    let Some(ts) = last_activity_ms else {
        return String::new();
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let diff_secs = (now - ts).max(0) / 1000;
    if diff_secs < 60 {
        "now".to_string()
    } else if diff_secs < 3600 {
        format!("{}m", diff_secs / 60)
    } else if diff_secs < 86400 {
        format!("{}h", diff_secs / 3600)
    } else {
        format!("{}d", diff_secs / 86400)
    }
}

/// One sidebar row: rounded, hover/selected background.
pub(crate) fn nav_row(id: SharedString, selected: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .mx_2()
        .px_2()
        .h(px(32.))
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .rounded_md()
        .text_size(px(13.))
        .cursor(CursorStyle::PointingHand)
        .when(selected, |el| el.bg(rgb(SELECTED)))
        .when(!selected, |el| el.hover(|s| s.bg(rgb(HOVER))))
}

/// Filter session indices for workspace list, omitting child subagent sessions
/// and applying search filtering.
pub fn filter_session_indices(
    sessions: &[crate::conversation::model::SessionEntry],
    child_sids: &std::collections::HashSet<&str>,
    search: &str,
) -> Vec<usize> {
    (0..sessions.len())
        .filter(|&i| {
            let s = &sessions[i];
            if child_sids.contains(s.session_id.as_str()) {
                return false;
            }
            search.is_empty()
                || s.title.to_lowercase().contains(search)
                || s.preview.to_lowercase().contains(search)
        })
        .collect()
}
