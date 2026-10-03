//! Review pane: unstaged/staged filters, per-file +/− stats with expandable
//! diffs, stage/commit/push. All git I/O runs on the background executor.

use crate::app::root::RootView;
use crate::review::git::{self, DiffSide};
use crate::shared::theme::{BORDER, CARD, DANGER, HOVER, MUTED, SUCCESS, TEXT};
use gpui::{
    AnyElement, Context, CursorStyle, IntoElement, ParentElement, Stateful, Styled, div,
    prelude::*, px, rgb,
};

/// What to do with a finished `run_git` result.
#[derive(Clone)]
pub(crate) enum GitAction {
    ThenRefresh(&'static str),
}

impl RootView {
    /// Full status sweep: porcelain status + both numstat maps in one job.
    pub(crate) fn refresh_git(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.active_workspace_path(cx) else {
            return;
        };
        self.git.generation += 1;
        let generation = self.git.generation;
        self.git.requested = Some(path.clone());
        self.git.busy = true;
        cx.notify();
        let root = path.clone();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    (
                        git::run_git(&path, &["status", "--porcelain=v1", "-b"]),
                        git::run_git(&path, &["diff", "--numstat"]),
                        git::run_git(&path, &["diff", "--cached", "--numstat"]),
                        git::run_git(&path, &["branch", "--list", "--format=%(refname:short)"]),
                    )
                })
                .await;
            this.update(cx, |v, cx| {
                if v.git.generation != generation {
                    return;
                }
                let (status, unstaged, staged, branches) = res;
                v.git.busy = false;
                match status {
                    Ok(out) => {
                        let mut st = git::parse_status(&out);
                        st.branches = branches
                            .unwrap_or_default()
                            .lines()
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        git::apply_numstat(
                            &mut st,
                            staged.map_or(Default::default(), |s| git::parse_numstat(&s)),
                            unstaged.map_or(Default::default(), |s| git::parse_numstat(&s)),
                        );
                        v.git.apply_status(root, st);
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

    /// Refresh when the active workspace differs from the one the pane last
    /// asked about (workspace switch while the dock is open).
    pub(crate) fn sync_git_workspace(&mut self, cx: &mut Context<Self>) {
        let active = self.active_workspace_path(cx);
        if active.is_some() && active != self.git.requested {
            self.refresh_git(cx);
        }
    }

    /// Load diffs for expanded rows that have none cached for the current
    /// side (after a refresh invalidated the cache or a side switch).
    pub(crate) fn load_missing_diffs(&mut self, cx: &mut Context<Self>) {
        for path in self.git.missing_diffs() {
            self.load_diff(&path, cx);
        }
    }

    /// Run one git command, then apply the mapped follow-up (diff cache or
    /// refresh + notice).
    pub(crate) fn run_git_action(
        &mut self,
        args: Vec<String>,
        action: GitAction,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.active_workspace_path(cx) else {
            return;
        };
        self.git.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    git::run_git(&path, &args.iter().map(String::as_str).collect::<Vec<_>>())
                })
                .await;
            this.update(cx, |v, cx| {
                v.git.busy = false;
                match action {
                    GitAction::ThenRefresh(label) => {
                        v.git.notice = Some(match res {
                            Ok(_) => (true, format!("{label} succeeded")),
                            Err(e) => (false, format!("{label} failed: {e}")),
                        });
                        v.refresh_git(cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Expand/collapse a file row; loads the diff text on first expand.
    pub(crate) fn toggle_file(&mut self, path: &str, cx: &mut Context<Self>) {
        if !self.git.expanded.remove(path) {
            self.git.expanded.insert(path.to_string());
            if self.git.diff_for(self.git.side, path).is_none() {
                self.load_diff(path, cx);
            }
        }
        cx.notify();
    }

    /// Fetch one file's diff for the current side into the cache.
    fn load_diff(&mut self, path: &str, cx: &mut Context<Self>) {
        let side = self.git.side;
        let Some(ws) = self.git.root.clone() else {
            return;
        };
        if !self.git.mark_pending(side, path) {
            return;
        }
        let generation = self.git.generation;
        let untracked = self
            .git
            .status
            .files
            .iter()
            .find(|f| f.path == path)
            .is_some_and(git::GitFile::untracked);
        let file = path.to_string();
        let diff_file = file.clone();
        cx.spawn(async move |this, cx| {
            let text = cx
                .background_spawn(async move {
                    if untracked {
                        // Untracked files have no index diff: show an
                        // all-added view from the file contents.
                        Ok(git::untracked_as_diff(&ws, &file))
                    } else {
                        let mut args = vec!["diff"];
                        if side == DiffSide::Staged {
                            args.push("--cached");
                        }
                        args.extend(["--", file.as_str()]);
                        git::run_git(&ws, &args)
                    }
                })
                .await;
            this.update(cx, |v, cx| {
                // A refresh since the request invalidated the cache.
                if v.git.generation != generation {
                    return;
                }
                let text = text.unwrap_or_else(|e| format!("(git diff failed: {e})"));
                v.git.store_diff(side, &diff_file, text);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Commit staged changes using the single-line composer's content.
    pub(crate) fn do_commit(&mut self, cx: &mut Context<Self>) {
        let msg = self.commit_input.read(cx).text().trim().to_string();
        if msg.is_empty() {
            self.git.notice = Some((false, "Commit message is empty".into()));
            cx.notify();
            return;
        }
        self.commit_input.update(cx, |c, _ccx| c.take_text());
        self.run_git_action(
            vec!["commit".into(), "-m".into(), msg],
            GitAction::ThenRefresh("Commit"),
            cx,
        );
    }

    pub(crate) fn stage_all(&mut self, cx: &mut Context<Self>) {
        self.run_git_action(
            vec!["add".into(), "-A".into()],
            GitAction::ThenRefresh("Stage all"),
            cx,
        );
    }

    pub(crate) fn stage_file(&mut self, path: &str, cx: &mut Context<Self>) {
        self.run_git_action(
            vec!["add".into(), "--".into(), path.to_string()],
            GitAction::ThenRefresh("Stage file"),
            cx,
        );
    }

    pub(crate) fn unstage_file(&mut self, path: &str, cx: &mut Context<Self>) {
        self.run_git_action(
            vec![
                "restore".into(),
                "--staged".into(),
                "--".into(),
                path.to_string(),
            ],
            GitAction::ThenRefresh("Unstage file"),
            cx,
        );
    }

    pub(crate) fn discard_file(&mut self, path: &str, untracked: bool, cx: &mut Context<Self>) {
        let args = if untracked {
            vec!["clean".into(), "-f".into(), "--".into(), path.to_string()]
        } else {
            vec!["restore".into(), "--".into(), path.to_string()]
        };
        self.run_git_action(args, GitAction::ThenRefresh("Discard file"), cx);
    }

    pub(crate) fn checkout_branch(&mut self, branch: &str, cx: &mut Context<Self>) {
        self.run_git_action(
            vec!["checkout".into(), branch.to_string()],
            GitAction::ThenRefresh("Switch branch"),
            cx,
        );
    }

    pub(crate) fn cycle_branch(&mut self, cx: &mut Context<Self>) {
        let branches = &self.git.status.branches;
        if branches.len() <= 1 {
            return;
        }
        let current = &self.git.status.branch;
        if let Some(pos) = branches.iter().position(|b| b == current) {
            let next_pos = (pos + 1) % branches.len();
            let next_branch = branches[next_pos].clone();
            self.checkout_branch(&next_branch, cx);
        } else if let Some(first) = branches.first() {
            let first = first.clone();
            self.checkout_branch(&first, cx);
        }
    }

    pub(crate) fn push(&mut self, cx: &mut Context<Self>) {
        self.run_git_action(vec!["push".into()], GitAction::ThenRefresh("Push"), cx);
    }

    fn notice_banner(&self) -> Option<AnyElement> {
        let (ok, msg) = self.git.notice.as_ref()?;
        Some(
            div()
                .px_2()
                .py_1()
                .text_size(px(11.))
                .text_color(rgb(if *ok { SUCCESS } else { DANGER }))
                .child(msg.clone())
                .into_any_element(),
        )
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

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .text_color(rgb(TEXT))
            .children(plan.as_ref().map(|p| {
                crate::conversation::turn_meta::render_plan_checklist(p, self.plan_expanded, cx)
            }))
            .child(self.render_agents_section(cx))
            .child(self.review_header(&branch, files_n, add, del, cx))
            .children(self.notice_banner())
            .child(
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
            .children(self.git.busy.then(|| {
                div()
                    .px_2()
                    .py_0p5()
                    .text_size(px(11.))
                    .text_color(rgb(MUTED))
                    .child("git running…")
            }))
            .child(self.commit_bar(cx))
            .into_any_element()
    }
}

pub(crate) fn side_btn(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .px_2()
        .py_0p5()
        .rounded_sm()
        .bg(rgb(CARD))
        .border_1()
        .border_color(rgb(BORDER))
        .text_size(px(11.))
        .text_color(rgb(TEXT))
        .cursor(CursorStyle::PointingHand)
        .hover(|h| h.bg(rgb(HOVER)))
        .child(label)
        .on_click(on_click)
}
