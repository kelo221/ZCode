//! Review pane file row and commit-bar rendering (split from review/pane.rs
//! for the 400-line cap).

use crate::app::root::RootView;
use crate::review::controls::side_btn;
use crate::review::git::DiffSide;
use crate::shared::theme::ui_size;
use crate::shared::theme::{BORDER, CARD, DANGER, HOVER, MUTED, SUCCESS, icon};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, Context, CursorStyle, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px,
};

impl RootView {
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
        let compare = self.git.target.is_compare();
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
            .group("file-row")
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_1p5()
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
                    .text_size(px(ui_size(10.)))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(rgb(crate::shared::theme::TOOL))
                    .child(badge),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(ui_size(11.5)))
                    .truncate()
                    .child(path.to_string()),
            )
            .child(
                div()
                    .text_size(px(ui_size(10.5)))
                    .text_color(rgb(SUCCESS))
                    .child(format!("+{add}")),
            )
            .child(
                div()
                    .text_size(px(ui_size(10.5)))
                    .text_color(rgb(DANGER))
                    .child(format!("-{del}")),
            )
            .when(side == DiffSide::Unstaged && !compare, |el| {
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
                        // Hover-revealed like the desktop; opacity fades are
                        // safe mid-frame (display toggles are not).
                        .opacity(0.)
                        .group_hover("file-row", |s| s.opacity(1.))
                        .text_size(px(ui_size(10.)))
                        .text_color(rgb(SUCCESS))
                        .cursor(CursorStyle::PointingHand)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.stage_file(&p1, cx);
                        }))
                        .child("+"),
                )
                .children(if self.git.confirm_discard.as_deref() == Some(path) {
                    // Armed confirmation (destructive op, 2026-10-05 audit):
                    // second click discards; the "keep" chip disarms.
                    vec![
                        div()
                            .id(SharedString::from(format!("discard-confirm-{act_path}")))
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .bg(rgb(DANGER))
                            .opacity(1.)
                            .text_size(px(ui_size(10.)))
                            .text_color(rgb(crate::shared::theme::BG))
                            .cursor(CursorStyle::PointingHand)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.discard_file(&p2, is_untracked, cx);
                            }))
                            .child("discard?"),
                        div()
                            .id(SharedString::from(format!("discard-keep-{act_path}")))
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .bg(rgb(CARD))
                            .opacity(1.)
                            .text_size(px(ui_size(10.)))
                            .text_color(rgb(MUTED))
                            .cursor(CursorStyle::PointingHand)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.cancel_discard(cx);
                            }))
                            .child("keep"),
                    ]
                } else {
                    vec![
                        div()
                            .id(SharedString::from(format!("discard-btn-{act_path}")))
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .bg(rgb(CARD))
                            .hover(|h| h.bg(rgb(HOVER)))
                            .opacity(0.)
                            .group_hover("file-row", |s| s.opacity(1.))
                            .text_size(px(ui_size(10.)))
                            .text_color(rgb(DANGER))
                            .cursor(CursorStyle::PointingHand)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.discard_file(&p2, is_untracked, cx);
                            }))
                            .child("✕"),
                    ]
                })
            })
            .when(side == DiffSide::Staged && !compare, |el| {
                let p1 = act_path.clone();
                el.child(
                    div()
                        .id(SharedString::from(format!("unstage-btn-{act_path}")))
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .bg(rgb(CARD))
                        .hover(|h| h.bg(rgb(HOVER)))
                        .opacity(0.)
                        .group_hover("file-row", |s| s.opacity(1.))
                        .text_size(px(ui_size(10.)))
                        .text_color(rgb(MUTED))
                        .cursor(CursorStyle::PointingHand)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.unstage_file(&p1, cx);
                        }))
                        .child("−"),
                )
            })
            .child(icon(
                if expanded {
                    crate::shared::theme::I_CHEVRON_DOWN
                } else {
                    crate::shared::theme::I_CHEVRON_RIGHT
                },
                12.,
                MUTED,
            ));
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
                    .text_size(px(ui_size(11.)))
                    .text_color(rgb(MUTED))
                    .child("(no textual changes)")
                    .into_any_element(),
                None => div()
                    .px_2()
                    .pb_1()
                    .text_size(px(ui_size(11.)))
                    .text_color(rgb(MUTED))
                    .child("loading diff…")
                    .into_any_element(),
            })
            .into_any_element()
    }

    pub(crate) fn commit_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let bar = div()
            .flex()
            .flex_col()
            .gap_1p5()
            .px_3()
            .py_2p5()
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
            );
        let bar = div().child(bar);
        #[cfg(test)]
        let bar = crate::app::test_support::track_children(bar, vec!["review-commit-bar".into()]);
        bar.into_any_element()
    }
}
