//! Reasoning row (Desktop `ai-elements/reasoning.tsx`): a Brain icon with
//! "Thought · N seconds"; the chevron shows on hover and stays while open. The
//! expanded text is plain, capped in height, and indented behind a guide line.
//! In noise-free mode thinking starts open, uncapped, as a shaded italic block
//! (PARITY.md "Noise-free transcript").

use crate::app::root::RootView;
use crate::shared::app_assets::BRAIN_ICON;
use crate::shared::i18n::{t, t_fmt};
use crate::shared::selectable::SelectableText;
use crate::shared::theme::{
    BORDER, I_CHEVRON_DOWN, I_CHEVRON_RIGHT, MUTED, PANEL, SUBTLEST, icon, ui_size,
};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, Context, ElementId, IntoElement, ParentElement, Styled, div, prelude::*, px,
};

const REASONING_GROUP: &str = "reasoning-trigger";

fn duration_label(duration_ms: Option<u64>) -> String {
    match duration_ms {
        Some(ms) if ms >= 1000 => t_fmt(
            "chat.reasoning.durationSeconds",
            &[("seconds", &ms.div_ceil(1000).to_string())],
        ),
        _ => t("chat.reasoning.durationFewSeconds").to_string(),
    }
}

/// 实测（model-io 调试日志）：Claude 类模型多数思考块只带签名、正文为空，
/// 常与下一个带正文的思考块相邻，导致出现两行 "Thought"。正文为空的行不渲染；
/// 流式增量写入正文后该行会自然出现。
pub(crate) fn has_reasoning_text(text: &str) -> bool {
    !text.trim().is_empty()
}

impl RootView {
    pub(crate) fn render_reasoning(
        &self,
        row_id: u64,
        text: &str,
        duration_ms: Option<u64>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let noise_free = crate::transcript::noise_free::enabled(cx);
        let expanded = noise_free != self.toggled_reasonings.contains(&row_id);
        let muted = rgb(SUBTLEST);
        let body = text.to_owned();
        let row = div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id(ElementId::NamedInteger("toggle-thought".into(), row_id))
                    .group(REASONING_GROUP)
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .text_size(px(ui_size(14.)))
                    .text_color(muted)
                    .child(
                        gpui::svg()
                            .path(BRAIN_ICON)
                            .size(px(16.))
                            .flex_none()
                            .text_color(muted),
                    )
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(t("chat.reasoning.thought").to_string()),
                    )
                    .child("·")
                    .child(duration_label(duration_ms))
                    .child(
                        div()
                            .when(!expanded, |el| {
                                el.opacity(0.)
                                    .group_hover(REASONING_GROUP, |s| s.opacity(1.))
                            })
                            .child(icon(
                                if expanded {
                                    I_CHEVRON_DOWN
                                } else {
                                    I_CHEVRON_RIGHT
                                },
                                16.,
                                SUBTLEST,
                            )),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if !this.toggled_reasonings.remove(&row_id) {
                            this.toggled_reasonings.insert(row_id);
                        }
                        cx.notify();
                    })),
            )
            .when(expanded && noise_free, |el| {
                el.child(
                    div().pt_2().child(
                        div()
                            .id(ElementId::NamedInteger("thought-body".into(), row_id))
                            .w_full()
                            .min_w_0()
                            .px_3()
                            .py_2()
                            .rounded_md()
                            .bg(rgb(PANEL))
                            .italic()
                            .text_size(px(ui_size(13.5)))
                            .text_color(rgb(MUTED))
                            .child(SelectableText::new(
                                ElementId::NamedInteger("thought-text".into(), row_id),
                                body.clone(),
                            )),
                    ),
                )
            })
            .when(expanded && !noise_free, |el| {
                el.child(
                    div().pt_3().child(
                        div()
                            .id(ElementId::NamedInteger("thought-body".into(), row_id))
                            .w_full()
                            .min_w_0()
                            .max_h(px(240.))
                            .overflow_y_scroll()
                            .ml_2()
                            .pl(px(14.))
                            .border_l_1()
                            .border_color(rgb(BORDER))
                            .text_size(px(ui_size(14.)))
                            .text_color(muted)
                            .child(SelectableText::new(
                                ElementId::NamedInteger("thought-text".into(), row_id),
                                body,
                            )),
                    ),
                )
            });
        #[cfg(test)]
        let row = crate::app::test_support::track_children(
            row,
            vec![
                format!("thought-toggle-{row_id}"),
                format!("thought-body-{row_id}"),
            ],
        );
        row.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{duration_label, has_reasoning_text};

    #[test]
    fn signature_only_reasoning_is_hidden() {
        assert!(!has_reasoning_text(""));
        assert!(!has_reasoning_text("  \n"));
        assert!(has_reasoning_text("Checking the ATM path."));
    }

    #[test]
    fn duration_rounds_up_to_whole_seconds() {
        assert_eq!(duration_label(None), "a few seconds");
        assert_eq!(duration_label(Some(400)), "a few seconds");
        assert_eq!(duration_label(Some(3200)), "4 seconds");
    }
}
