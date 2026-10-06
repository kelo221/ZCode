use crate::app::root::RootView;
use crate::composer::held_queue::HeldDisposition;
use crate::shared::{
    i18n::label,
    theme::{BORDER, MUTED, PANEL, ui_size},
    theme_colors::color as rgb,
};
use ely_gpui_component::buttons::{Button, ButtonVariant};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};

impl RootView {
    pub(crate) fn held_queue_confirmation(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let receipt = self.state.read(cx).held_confirmation.clone()?;
        if !self.state.read(cx).held_binding_matches(&receipt, cx) {
            return None;
        }
        let mut row = div().flex().flex_wrap().gap_2().items_center();
        for (id, text, disposition) in [
            (
                "held-queue-clear",
                label("Clear queue and send", "清空队列并发送"),
                Some(HeldDisposition::Clear),
            ),
            (
                "held-queue-keep",
                label("Keep queue and send", "保留队列并发送"),
                Some(HeldDisposition::Keep),
            ),
            ("held-queue-cancel", label("Cancel", "取消"), None),
        ] {
            let receipt = receipt.clone();
            row = row.child(
                Button::new(id, text)
                    .variant(ButtonVariant::Secondary)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |s, cx| match disposition {
                            Some(disposition) => {
                                s.confirm_held_submission(receipt.clone(), disposition, cx)
                            }
                            None => s.cancel_held_confirmation(&receipt, cx),
                        });
                    })),
            );
        }
        #[cfg(test)]
        let row = crate::app::test_support::track_children(
            row,
            vec![
                "held-queue-clear".into(),
                "held-queue-keep".into(),
                "held-queue-cancel".into(),
            ],
        );
        Some(
            div()
                .w_full()
                .p_2()
                .flex()
                .flex_col()
                .gap_2()
                .bg(rgb(PANEL))
                .border_1()
                .border_color(rgb(BORDER))
                .rounded_md()
                .text_size(px(ui_size(12.)))
                .child(div().text_color(rgb(MUTED)).child(label(
                    "The queue is paused. Clear or keep its items before sending?",
                    "队列已暂停。发送前清空还是保留已有输入？",
                )))
                .child(row)
                .into_any_element(),
        )
    }
}
