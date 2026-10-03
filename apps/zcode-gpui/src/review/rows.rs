//! Review pane row/header/commit-bar rendering (split from review/pane.rs
//! for the 400-line cap).

use crate::app::root::RootView;
use crate::review::git::DiffSide;
use crate::review::pane::side_btn;
use crate::shared::theme::{ACCENT, BORDER, CARD, DANGER, HOVER, MUTED, SUCCESS, icon};
use gpui::{
    AnyElement, Context, CursorStyle, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px, rgb,
};

impl RootView {
    /// Branch chip, totals, side filter, stage-all and refresh controls.
    pub(crate) fn review_header(
        &self,
        branch: &str,
        files_n: usize,
        add: u32,
        del: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let side = self.git.side;
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .p_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id("branch-chip")
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .bg(rgb(CARD))
                            .hover(|h| h.bg(rgb(HOVER)))
                            .cursor(CursorStyle::PointingHand)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_size(px(11.))
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
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(format!("{files_n} files")),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(SUCCESS))
                            .child(format!("+{add}")),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(DANGER))
                            .child(format!("-{del}")),
                    )
                    .child(div().flex_1())
                    .child(side_btn(
                        "stage-all",
                        "Stage all",
                        cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.stage_all(cx);
                        }),
                    ))
                    .child(side_btn(
                        "git-refresh",
                        "Refresh",
                        cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.refresh_git(cx);
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .children([DiffSide::Unstaged, DiffSide::Staged].map(|s| {
                        let selected = s == side;
                        div()
                            .id(SharedString::from(format!("side-{}", s.label())))
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .text_size(px(11.))
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
                    })),
            )
            .into_any_element()
    }

    pub(crate) fn review_row(
        &self,
        path: &str,
        side: DiffSide,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (badge, add, del) = self
            .git
            .status
            .files
            .iter()
            .find(|f| f.path == path)
            .map(|f| {
                if side == DiffSide::Staged {
                    (f.badge(true), f.staged_add, f.staged_del)
                } else {
                    (f.badge(false), f.unstaged_add, f.unstaged_del)
                }
            })
            .unwrap_or(("--".to_string(), 0, 0));
        let expanded = self.git.expanded.contains(path);
        let row_path = path.to_string();
        let act_path = path.to_string();
        let is_untracked = self
            .git
            .status
            .files
            .iter()
            .find(|f| f.path == path)
            .is_some_and(crate::review::git::GitFile::untracked);

        let row = div()
            .id(SharedString::from(format!("file-{path}")))
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_sm()
            .cursor(CursorStyle::PointingHand)
            .hover(|h| h.bg(rgb(HOVER)))
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                let p = row_path.clone();
                this.toggle_file(&p, cx);
            }))
            .child(
                div()
                    .w(px(18.))
                    .text_size(px(10.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(crate::shared::theme::TOOL))
                    .child(badge),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(11.5))
                    .truncate()
                    .child(path.to_string()),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(SUCCESS))
                    .child(format!("+{add}")),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(DANGER))
                    .child(format!("-{del}")),
            )
            .when(side == DiffSide::Unstaged, |el| {
                let p1 = act_path.clone();
                let p2 = act_path.clone();
                el.child(
                    div()
                        .id(SharedString::from(format!("stage-btn-{act_path}")))
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .bg(rgb(CARD))
                        .hover(|h| h.bg(rgb(HOVER)))
                        .text_size(px(10.))
                        .text_color(rgb(SUCCESS))
                        .cursor(CursorStyle::PointingHand)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.stage_file(&p1, cx);
                        }))
                        .child("+"),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("discard-btn-{act_path}")))
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .bg(rgb(CARD))
                        .hover(|h| h.bg(rgb(HOVER)))
                        .text_size(px(10.))
                        .text_color(rgb(DANGER))
                        .cursor(CursorStyle::PointingHand)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.discard_file(&p2, is_untracked, cx);
                        }))
                        .child("✕"),
                )
            })
            .when(side == DiffSide::Staged, |el| {
                let p1 = act_path.clone();
                el.child(
                    div()
                        .id(SharedString::from(format!("unstage-btn-{act_path}")))
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .bg(rgb(CARD))
                        .hover(|h| h.bg(rgb(HOVER)))
                        .text_size(px(10.))
                        .text_color(rgb(MUTED))
                        .cursor(CursorStyle::PointingHand)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.unstage_file(&p1, cx);
                        }))
                        .child("−"),
                )
            })
            .child(icon(if expanded { '▾' } else { '▸' }, 9., MUTED));
        if !expanded {
            return row.into_any_element();
        }
        let diff = self.git.diff_for(side, path).cloned();
        div()
            .flex()
            .flex_col()
            .child(row)
            .child(match diff {
                Some(text) if !text.is_empty() => crate::shared::diff_view::render_diff(&text),
                Some(_) => div()
                    .px_2()
                    .pb_1()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child("(no textual changes)")
                    .into_any_element(),
                None => div()
                    .px_2()
                    .pb_1()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child("loading diff…")
                    .into_any_element(),
            })
            .into_any_element()
    }

    pub(crate) fn commit_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .p_2()
            .border_t_1()
            .border_color(rgb(BORDER))
            .child(self.commit_input.clone())
            .child(
                div()
                    .flex()
                    .gap_1p5()
                    .child(side_btn(
                        "commit",
                        "Commit",
                        cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.do_commit(cx);
                        }),
                    ))
                    .child(side_btn(
                        "push",
                        "Push",
                        cx.listener(|this, _, _, cx| {
                            cx.stop_propagation();
                            this.push(cx);
                        }),
                    )),
            )
            .into_any_element()
    }
}
