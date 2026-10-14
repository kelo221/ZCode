//! Thinking-level indicator from Desktop `ThoughtLevelCycleControl.tsx`: a
//! brain icon with a vertical bar filled (success colour) in proportion to
//! the level among the model's thinking levels. Clicking it cycles to the
//! next level; the label beside it opens the full menu.

use crate::app::root::RootView;
use crate::app::store::AppState;
use crate::composer::config_cmds::EffectiveConfig;
use crate::shared::app_assets::BRAIN_ICON;
use crate::shared::theme::{MUTED, SUCCESS, TEXT};
use crate::shared::theme_colors::color as rgb;
use gpui::{AnyElement, Context, div, prelude::*, px, relative};

const NO_THOUGHT: [&str; 8] = [
    "disabled", "false", "no", "none", "off", "nothink", "no-think", "no_think",
];

fn is_no_thought(level: &str) -> bool {
    NO_THOUGHT.contains(&level.trim().to_lowercase().as_str())
}

/// Fill fraction: position of `current` among the non-off levels; zero for
/// off, the default (empty) and unknown values.
pub fn thought_progress(current: &str, levels: &[String]) -> f32 {
    let thinking: Vec<&String> = levels.iter().filter(|l| !is_no_thought(l)).collect();
    if thinking.is_empty() || is_no_thought(current) {
        return 0.;
    }
    thinking
        .iter()
        .position(|l| *l == current)
        .map_or(0., |i| (i + 1) as f32 / thinking.len() as f32)
}

/// Next level in configured order, wrapping; an unknown current starts at
/// the first level.
pub fn next_thought(current: &str, levels: &[String]) -> Option<String> {
    let next = match levels.iter().position(|l| l == current) {
        Some(i) => (i + 1) % levels.len(),
        None => 0,
    };
    levels.get(next).cloned()
}

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

impl RootView {
    pub(crate) fn thought_indicator(
        &self,
        effective: &EffectiveConfig,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fill = thought_progress(&effective.thought, &effective.thought_levels);
        let next = next_thought(&effective.thought, &effective.thought_levels);
        let (provider, model) = (effective.provider.clone(), effective.model.clone());
        div()
            .id("thought-cycle")
            .flex()
            .items_center()
            .gap_1()
            .h(px(28.))
            .px_1()
            .rounded_md()
            .cursor_pointer()
            .text_color(rgb(MUTED))
            .hover(|s| s.text_color(rgb(TEXT)))
            .child(
                gpui::svg()
                    .path(BRAIN_ICON)
                    .size(px(16.))
                    .text_color(rgb(MUTED)),
            )
            .child(
                div()
                    .relative()
                    .w(px(4.))
                    .h(px(16.))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(crate::shared::theme_colors::tinted(TEXT, 0.1))
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .left_0()
                            .w_full()
                            .rounded_full()
                            .bg(rgb(SUCCESS))
                            .h(relative(fill))
                            .when(fill > 0., |el| el.min_h(px(4.))),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                if let Some(thought) = next.clone() {
                    let (provider, model) = (provider.clone(), model.clone());
                    this.state.update(cx, |s, cx| {
                        apply_thought(s, &provider, &model, &thought, cx)
                    });
                }
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn progress_counts_only_thinking_levels() {
        let l = levels(&["off", "low", "medium", "high"]);
        assert_eq!(thought_progress("off", &l), 0.);
        assert_eq!(thought_progress("", &l), 0.);
        assert_eq!(thought_progress("low", &l), 1. / 3.);
        assert_eq!(thought_progress("high", &l), 1.);
        assert_eq!(thought_progress("bogus", &l), 0.);
    }

    #[test]
    fn cycling_wraps_and_starts_unknown_at_first() {
        let l = levels(&["low", "high"]);
        assert_eq!(next_thought("low", &l).as_deref(), Some("high"));
        assert_eq!(next_thought("high", &l).as_deref(), Some("low"));
        assert_eq!(next_thought("", &l).as_deref(), Some("low"));
        assert_eq!(next_thought("", &[]), None);
    }
}
