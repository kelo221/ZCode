use crate::app::root::RootView;
use crate::conversation::turn_meta::PlanState;
use crate::shared::theme::{ACCENT, BORDER, CARD, MUTED, TEXT, icon, phase_badge, ui_size};
use crate::shared::theme_colors::color as rgb;
use gpui::{Context, CursorStyle, Div, ParentElement, SharedString, Styled, div, prelude::*, px};

impl RootView {
    /// Header bar with conversation title, progress counter, tools & terminal toggles.
    pub(crate) fn main_header(
        &self,
        title: SharedString,
        context_tag: Option<SharedString>,
        plan: Option<&PlanState>,
        phase: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let title_col = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .min_w_0()
            .flex_1()
            .children(context_tag.map(|tag| {
                div()
                    .text_size(px(ui_size(14.)))
                    .text_color(rgb(MUTED))
                    .child(format!("{tag} ›"))
            }))
            .child(
                div()
                    .text_size(px(ui_size(14.)))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(TEXT))
                    .truncate()
                    .child(title),
            );

        div()
            .h(px(46.))
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_5()
            .child(title_col)
            .children(plan.map(|p| {
                let c = p.completed_count();
                let t = p.items.len();
                div()
                    .id("plan-header-badge")
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(ui_size(11.)))
                    .text_color(rgb(ACCENT))
                    .cursor(CursorStyle::PointingHand)
                    .child(format!("Progress {c}/{t}"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dock_open = true;
                        this.dock_tab = crate::app::dock::DockTab::Review;
                        this.plan_expanded = !this.plan_expanded;
                        cx.notify();
                    }))
            }))
            .children(phase_badge(phase))
            .child(
                div()
                    .id("dock-toggle")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(ui_size(11.)))
                    .text_color(rgb(if self.dock_open { ACCENT } else { MUTED }))
                    .cursor(CursorStyle::PointingHand)
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.dock_open = !this.dock_open;
                        if this.dock_open {
                            this.on_dock_tab(this.dock_tab, cx);
                        }
                        cx.notify();
                    }))
                    .child("Tools")
                    .child(icon(crate::shared::theme::I_CHEVRON_DOWN, 8., MUTED)),
            )
            .child(
                div()
                    .id("term-toggle")
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .bg(rgb(CARD))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .text_size(px(ui_size(11.)))
                    .text_color(rgb(if self.term_open { ACCENT } else { MUTED }))
                    .cursor(CursorStyle::PointingHand)
                    .hover(|s| s.text_color(rgb(TEXT)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.term_open = !this.term_open;
                        if this.term_open {
                            this.ensure_term(cx);
                            window.focus(&this.term.focus, cx);
                        }
                        cx.notify();
                    }))
                    .child("Terminal")
                    .child(icon(crate::shared::theme::I_CHEVRON_DOWN, 8., MUTED)),
            )
    }
}
