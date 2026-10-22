//! Review dock layout: progress and agents on top, then the git summary
//! (branch chip, file count, +/- totals) that opens the file list, the side
//! filter, stage-all and the commit bar. Split from review/pane.rs (git I/O).

use crate::app::root::RootView;
use crate::review::controls::side_btn;
use crate::review::git::DiffSide;
use crate::shared::theme::ui_size;
use crate::shared::theme::{ACCENT, BORDER, CARD, DANGER, HOVER, MUTED, SUCCESS, TEXT};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, Context, CursorStyle, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px, relative,
};

impl RootView {
    /// Git summary (branch chip, file count, +/-), which opens the file list;
    /// once open, a second row carries the side filter and stage-all.
    pub(crate) fn review_header(
        &self,
        branch: &str,
        files_n: usize,
        add: u32,
        del: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let side = self.git.side;
        let compare = self.git.target.is_compare();
        let open = self.git.list_visible();
        let summary = div()
            .id("git-summary")
            .flex()
            .items_center()
            .gap_2()
            .px_1()
            .py_0p5()
            .rounded_sm()
            .cursor(CursorStyle::PointingHand)
            .hover(|h| h.bg(rgb(HOVER)))
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                if this.git.target.is_compare() {
                    return;
                }
                this.git.list_open = !this.git.list_open;
                cx.notify();
            }))
            .child(
                div()
                    .text_size(px(ui_size(11.)))
                    .text_color(rgb(MUTED))
                    .child(if open { "▾" } else { "▸" }),
            )
            .child(
                div()
                    .id("branch-chip")
                    .min_w_0()
                    .truncate()
                    .px_1p5()
                    .py_0p5()
                    .rounded_sm()
                    .bg(rgb(CARD))
                    .hover(|h| h.bg(rgb(HOVER)))
                    .cursor(CursorStyle::PointingHand)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_size(px(ui_size(11.)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.cycle_branch(cx);
                    }))
                    .child(if branch.is_empty() {
                        "no branch".to_string()
                    } else {
                        branch.to_string()
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(ui_size(11.)))
                    .text_color(rgb(MUTED))
                    .child(format!(
                        "{files_n} file{}",
                        if files_n == 1 { "" } else { "s" }
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(ui_size(11.)))
                    .text_color(rgb(SUCCESS))
                    .child(format!("+{add}")),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(ui_size(11.)))
                    .text_color(rgb(DANGER))
                    .child(format!("-{del}")),
            )
            .child(div().flex_1())
            .child(side_btn(
                "git-refresh",
                "Refresh",
                cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.refresh_git(cx);
                }),
            ));
        #[cfg(test)]
        let summary = crate::app::test_support::track_children(
            div().child(summary),
            vec!["git-summary".into()],
        );
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .p_2()
            .border_t_1()
            .border_color(rgb(BORDER))
            .when(open, |el| el.border_b_1())
            .child(summary)
            .when(open, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .when(!compare, |el| {
                            el.children([DiffSide::Unstaged, DiffSide::Staged].map(|s| {
                                let selected = s == side;
                                div()
                                    .id(SharedString::from(format!("side-{}", s.label())))
                                    .px_3()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_size(px(ui_size(11.)))
                                    .cursor(CursorStyle::PointingHand)
                                    .when(selected, |el| el.bg(rgb(CARD)).text_color(rgb(ACCENT)))
                                    .when(!selected, |el| {
                                        el.text_color(rgb(MUTED)).hover(|h| h.bg(rgb(HOVER)))
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.git.side = s;
                                        // Expanded rows need this side's diff too.
                                        this.load_missing_diffs(cx);
                                        cx.notify();
                                    }))
                                    .child(s.label())
                            }))
                        })
                        .child(div().flex_1())
                        .child(self.review_primary_button(cx)),
                )
            })
            .into_any_element()
    }

    pub(crate) fn review_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        // Plan progress capsule (desktop parity: a section in the side panel,
        // not an overlay on the transcript).
        let plan = self
            .state
            .read(cx)
            .active_conversation()
            .and_then(|c| c.plan.clone());
        let branch = self.git.status.branch.clone();
        let (files_n, add, del) = self.git.totals();
        let side = self.git.side;
        let rows: Vec<String> = self
            .git
            .files_for_side()
            .iter()
            .map(|f| f.path.clone())
            .collect();
        let list_open = self.git.list_visible();
        let commit_bar = (list_open && !self.git.target.is_compare()).then(|| self.commit_bar(cx));

        // Agents and progress own the dock; the git file list only takes the
        // lower part once the user opens it from the summary row.
        let activity = div()
            .id("review-activity")
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_y_scroll()
            .when(list_open, |el| el.flex_none().max_h(relative(0.5)))
            .when(!list_open, |el| el.flex_1())
            .children(plan.as_ref().map(|p| {
                crate::conversation::turn_meta::render_plan_checklist(p, self.plan_expanded, cx)
            }))
            .children(self.goal_section(cx))
            .child(self.render_agents_section(cx))
            .child(self.render_background_activity(cx))
            .children(self.render_agent_branches(cx));

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .text_color(rgb(TEXT))
            .child(activity)
            .child(self.review_header(&branch, files_n, add, del, cx))
            .children(self.notice_banner())
            .when(list_open, |el| {
                el.child(
                    div()
                        .id("review-list")
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .children(
                            rows.into_iter()
                                .map(|path| self.review_row(&path, side, cx)),
                        ),
                )
            })
            .children(self.git.busy.then(|| {
                div()
                    .px_2()
                    .py_0p5()
                    .text_size(px(ui_size(11.)))
                    .text_color(rgb(MUTED))
                    .child("git running…")
            }))
            .children(commit_bar)
            .into_any_element()
    }
}
