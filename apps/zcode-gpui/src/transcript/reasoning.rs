use crate::app::root::RootView;
use crate::shared::theme::{BORDER, REASONING, ui_size};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, Context, ElementId, IntoElement, ParentElement, Styled, div, prelude::*, px,
};

impl RootView {
    pub(crate) fn render_reasoning(
        &self,
        row_id: u64,
        text: &str,
        duration_ms: Option<u64>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.expanded_reasonings.contains(&row_id);
        let duration = duration_ms
            .map(|ms| format!(" ({})", crate::conversation::turn_meta::format_duration(ms)))
            .unwrap_or_default();
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .my_1()
            .child(
                div()
                    .id(ElementId::NamedInteger("toggle-thought".into(), row_id))
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .text_size(px(ui_size(11.5)))
                    .text_color(rgb(REASONING))
                    .child(if expanded { "▼" } else { "▶" })
                    .child(format!("💭 Thought{duration}"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if !this.expanded_reasonings.remove(&row_id) {
                            this.expanded_reasonings.insert(row_id);
                        }
                        cx.notify();
                    })),
            )
            .when(expanded, |el| {
                el.child(
                    div()
                        .pl_3()
                        .border_l_1()
                        .border_color(rgb(BORDER))
                        .text_size(px(ui_size(12.)))
                        .text_color(rgb(REASONING))
                        .opacity(0.85)
                        .child(text.to_owned()),
                )
            })
            .into_any_element()
    }
}
