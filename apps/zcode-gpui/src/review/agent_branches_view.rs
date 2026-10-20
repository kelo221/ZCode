//! Review pane "Agent branches" list and the read-only comparison mode
//! (PARITY.md "Agent branches (read-only)"). Git I/O runs on the
//! background executor; generations drop results for superseded requests.

use crate::app::root::RootView;
use crate::review::agent_branches::{self, AgentBranch, ReviewTarget};
use crate::review::controls::side_btn;
use crate::shared::theme::ui_size;
use crate::shared::theme::{ACCENT, CARD, DANGER, HOVER, MUTED, SUCCESS, TEXT};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, Context, CursorStyle, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px,
};
use std::path::PathBuf;

impl RootView {
    /// Re-list the agent branches of the repository containing `cwd`.
    pub(crate) fn refresh_agent_branches(&mut self, cwd: PathBuf, cx: &mut Context<Self>) {
        self.git.branches_generation += 1;
        let generation = self.git.branches_generation;
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move { agent_branches::load_agent_branches(&cwd) })
                .await;
            this.update(cx, |v, cx| {
                if v.git.branches_generation != generation {
                    return;
                }
                // Not a repository, or git failed: the section just stays hidden.
                v.git.agent_branches = res.unwrap_or_default();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn open_agent_branch(&mut self, branch: AgentBranch, cx: &mut Context<Self>) {
        if self.git.set_target(ReviewTarget::AgentBranch(branch)) {
            self.refresh_git(cx);
        }
    }

    pub(crate) fn close_agent_branch(&mut self, cx: &mut Context<Self>) {
        if self.git.set_target(ReviewTarget::WorkingTree) {
            self.refresh_git(cx);
        }
    }

    /// Re-read the pinned comparison's file list (the SHAs never move).
    pub(crate) fn refresh_compare(&mut self, branch: AgentBranch, cx: &mut Context<Self>) {
        self.git.generation += 1;
        let generation = self.git.generation;
        self.git.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let top = branch.repo_top.clone();
            let res = cx
                .background_spawn(async move { agent_branches::load_compare(&branch) })
                .await;
            this.update(cx, |v, cx| {
                if v.git.generation != generation {
                    return;
                }
                v.git.busy = false;
                match res {
                    Ok(status) => {
                        v.git.apply_status(top, status);
                        v.load_missing_diffs(cx);
                    }
                    Err(e) => v.git.notice = Some((false, e)),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Header action: "Stage all" on the working tree; in a read-only
    /// comparison, "Working tree" returns to it instead.
    pub(crate) fn review_primary_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let (id, button) = if self.git.target.is_compare() {
            let close = cx.listener(|this: &mut Self, _: &gpui::ClickEvent, _, cx| {
                cx.stop_propagation();
                this.close_agent_branch(cx);
            });
            (
                "compare-close",
                side_btn("compare-close", "Working tree", close),
            )
        } else {
            let stage = cx.listener(|this: &mut Self, _: &gpui::ClickEvent, _, cx| {
                cx.stop_propagation();
                this.stage_all(cx);
            });
            ("stage-all", side_btn("stage-all", "Stage all", stage))
        };
        let wrapper = div().child(button);
        #[cfg(test)]
        let wrapper = crate::app::test_support::track_children(wrapper, vec![id.into()]);
        #[cfg(not(test))]
        let _ = id;
        wrapper.into_any_element()
    }

    pub(crate) fn render_agent_branches(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.git.agent_branches.is_empty() {
            return None;
        }
        let selected = match &self.git.target {
            ReviewTarget::AgentBranch(b) => Some(b.branch.clone()),
            ReviewTarget::WorkingTree => None,
        };
        let rows = self.git.agent_branches.iter().map(|branch| {
            let is_selected = selected.as_deref() == Some(branch.branch.as_str());
            let open = branch.clone();
            let row = div()
                .id(SharedString::from(format!(
                    "agent-branch-{}",
                    branch.branch
                )))
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .rounded_sm()
                .cursor(CursorStyle::PointingHand)
                .when(is_selected, |el| el.bg(rgb(CARD)))
                .hover(|h| h.bg(rgb(HOVER)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.open_agent_branch(open.clone(), cx);
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(ui_size(11.5)))
                        .text_color(rgb(if is_selected { ACCENT } else { TEXT }))
                        .child(branch.branch.clone()),
                )
                .child(
                    div()
                        .text_size(px(ui_size(10.5)))
                        .text_color(rgb(MUTED))
                        .child(format!("{} files", branch.files)),
                )
                .child(
                    div()
                        .text_size(px(ui_size(10.5)))
                        .text_color(rgb(SUCCESS))
                        .child(format!("+{}", branch.add)),
                )
                .child(
                    div()
                        .text_size(px(ui_size(10.5)))
                        .text_color(rgb(DANGER))
                        .child(format!("−{}", branch.del)),
                );
            let row = div().child(row);
            #[cfg(test)]
            let row = crate::app::test_support::track_children(
                row,
                vec![format!("agent-branch-{}", branch.branch)],
            );
            row
        });
        Some(
            div()
                .id("agent-branches")
                .flex()
                .flex_col()
                .py_1()
                .child(
                    div()
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .px_3()
                        .text_size(px(ui_size(12.)))
                        .text_color(rgb(TEXT))
                        .child("Agent branches"),
                )
                .children(rows)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[path = "agent_branches_view_tests.rs"]
mod tests;
