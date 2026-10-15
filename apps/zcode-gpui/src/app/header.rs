use crate::app::layout::HEADER_HEIGHT;
use crate::app::root::RootView;
use crate::conversation::turn_meta::PlanState;
use crate::shared::theme::{BORDER, MUTED, TAG, TEXT, phase_badge, ui_size};
use crate::shared::theme_colors::{color as rgb, tinted};
use ely_gpui_component::buttons::{ButtonVariant, IconButton};
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ControlSize;
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

        let toggle = |id: &'static str, glyph: IconName, on: bool, tip: &'static str| {
            IconButton::new(id, glyph)
                .size(ControlSize::Md)
                .variant(if on {
                    ButtonVariant::Subtle
                } else {
                    ButtonVariant::Ghost
                })
                .tooltip(tip)
        };
        div()
            .h(px(HEADER_HEIGHT))
            .flex_none()
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .pl_4()
            .pr_2()
            .border_b_1()
            .border_color(tinted(BORDER, 0.5))
            .child(title_col)
            .children(plan.map(|p| {
                let c = p.completed_count();
                let t = p.items.len();
                div()
                    .id("plan-header-badge")
                    .px_2()
                    .py_0p5()
                    .rounded_full()
                    .bg(tinted(TAG, 0.5))
                    .text_size(px(ui_size(12.)))
                    .text_color(rgb(MUTED))
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
            .when(self.desktop_tools(cx), |el| {
                el.child(
                    toggle("dock-toggle", IconName::PanelRight, self.dock_open, "Tools").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.dock_open = !this.dock_open;
                            if this.dock_open {
                                this.on_dock_tab(this.dock_tab, cx);
                            }
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    toggle(
                        "term-toggle",
                        IconName::Terminal,
                        self.term_open,
                        "Terminal",
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.term_open = !this.term_open;
                        if this.term_open {
                            this.ensure_term(cx);
                            window.focus(&this.term.focus, cx);
                        }
                        cx.notify();
                    })),
                )
            })
    }
}
