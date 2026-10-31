//! Applies a thinking level picked from the composer's thinking menu.

use crate::app::store::AppState;
use gpui::Context;

/// Apply a thinking level to whichever config owner is active (restored
/// draft override, draft UI choice, or the live session).
pub(crate) fn apply_thought(
    s: &mut AppState,
    provider: &str,
    model: &str,
    thought: &str,
    cx: &mut Context<AppState>,
) {
    // /plan 草稿已有 override，但没有 live session；先更新同一 owner，避免思考级别点击被吞掉。
    if s.set_restored_model(provider, model, thought) {
        cx.notify();
        return;
    }
    if !s.effective_config().has_session {
        s.ui_thought = Some(thought.to_string());
        cx.notify();
        return;
    }
    if provider.is_empty() || model.is_empty() {
        s.switch_thought(thought, cx);
    } else {
        s.switch_model(provider, model, thought, cx);
    }
}
