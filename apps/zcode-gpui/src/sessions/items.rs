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

// --- Progressive session-list loading ---------------------------------------
//
// Every workspace renders its whole session list each frame, which stutters
// with hundreds of threads. The sidebar therefore shows the 3 latest rows per
// workspace and lets the user load more twice: 3 → 20 → all (the desktop
// virtualizes its list instead, so it has no such cap).

/// Rows shown before the first "Load more" click.
pub const SESSION_PREVIEW_ROWS: usize = 3;
/// Rows shown after the first "Load more" click; the second click shows all.
pub const SESSION_EXTENDED_ROWS: usize = 25;
/// Limit steps: 0 = preview, 1 = extended, ≥2 = everything.
pub const SESSION_LIMIT_ALL: u8 = 2;

/// How many of `total` filtered rows to render at `step`. While searching,
/// all matches are shown — the query is explicit intent and result sets are
/// small.
pub(crate) fn visible_session_count(step: u8, total: usize, searching: bool) -> usize {
    if searching || step >= SESSION_LIMIT_ALL {
        return total;
    }
    let cap = if step == 0 {
        SESSION_PREVIEW_ROWS
    } else {
        SESSION_EXTENDED_ROWS
    };
    cap.min(total)
}

/// The step after one "Load more" click.
pub(crate) fn next_limit_step(step: u8) -> u8 {
    (step + 1).min(SESSION_LIMIT_ALL)
}

/// Label of the load-more row: the second (final) click says "all".
pub(crate) fn load_more_label(step: u8, hidden: usize) -> String {
    if next_limit_step(step) >= SESSION_LIMIT_ALL {
        format!("Load all ({hidden} more)")
    } else {
        format!("Load more ({hidden} more)")
    }
}
