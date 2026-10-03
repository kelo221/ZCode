//! Visual inspector for workflow runs, subagent actors roster, and node execution progress.
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/workflow-runs.ts and PARITY.md M7.

use crate::app::root::RootView;
use crate::conversation::workflows::{WorkflowRunActor, WorkflowRunNode, WorkflowRunState};
use crate::shared::theme::{
    ACCENT, BORDER, CARD, DANGER, HOVER, MUTED, PANEL, SUCCESS, TEXT, TOOL,
};
use gpui::{
    AnyElement, Context, CursorStyle, ElementId, InteractiveElement, IntoElement, ParentElement,
    Styled, div, prelude::*, px, rgb,
};

impl RootView {
    pub(crate) fn workflows_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let active_sid = self.state.read(cx).active.clone();
        let runs = active_sid
            .as_ref()
            .and_then(|sid| self.state.read(cx).conversations.get(sid))
            .map(|c| c.workflow_runs.runs.clone())
            .unwrap_or_default();

        div()
            .flex_1()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .min_h_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(format!("Workflows & Subagents ({})", runs.len())),
                    ),
            )
            .child(if runs.is_empty() {
                div()
                    .id("workflows-empty")
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .child("No workflow runs in this conversation.")
            } else {
                div()
                    .id("workflows-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .children(
                        runs.into_iter()
                            .enumerate()
                            .map(|(idx, run)| self.render_workflow_run_card(idx, run, cx)),
                    )
            })
            .into_any_element()
    }

    fn render_workflow_run_card(
        &self,
        idx: usize,
        run: WorkflowRunState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_running = run.status == "running";
        let is_completed = run.status == "completed";
        let is_err = run.status == "errored";
        let is_stopped = run.status == "stopped";

        let status_color = if is_completed {
            SUCCESS
        } else if is_err {
            DANGER
        } else if is_running {
            ACCENT
        } else {
            MUTED
        };

        let run_id = run.run_id.clone();
        let can_resume = run.resumable.unwrap_or(false) || is_stopped;

        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_2p5()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded_md()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .child(format!("Run #{}", idx + 1)),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .px_1p5()
                                    .rounded_sm()
                                    .bg(rgb(PANEL))
                                    .text_color(rgb(status_color))
                                    .child(run.status.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .when(can_resume, |el| {
                                let rid = run_id.clone();
                                el.child(
                                    div()
                                        .id(ElementId::NamedInteger("resume-wf".into(), idx as u64))
                                        .px_2()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_size(px(10.5))
                                        .bg(rgb(PANEL))
                                        .text_color(rgb(ACCENT))
                                        .cursor(CursorStyle::PointingHand)
                                        .hover(|h| h.bg(rgb(HOVER)))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            let r = rid.clone();
                                            this.state.update(cx, |state, cx| {
                                                state.resume_workflow_run(&r, None, cx);
                                            });
                                        }))
                                        .child("Resume"),
                                )
                            })
                            .when(is_running, |el| {
                                el.child(
                                    div()
                                        .id(ElementId::NamedInteger("stop-wf".into(), idx as u64))
                                        .px_2()
                                        .py_0p5()
                                        .rounded_sm()
                                        .text_size(px(10.5))
                                        .bg(rgb(PANEL))
                                        .text_color(rgb(DANGER))
                                        .cursor(CursorStyle::PointingHand)
                                        .hover(|h| h.bg(rgb(HOVER)))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.state.update(cx, |state, cx| {
                                                state.stop(cx);
                                            });
                                        }))
                                        .child("Stop"),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .text_size(px(10.5))
                    .text_color(rgb(MUTED))
                    .child(format!("Tokens: {}", run.usage.spent_tokens))
                    .child(format!("Nodes: {}", run.usage.nodes_used))
                    .when_some(run.subagent_model.as_deref(), |el, model| {
                        el.child(format!("Model: {model}"))
                    }),
            )
            .when(!run.actors.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .pt_1()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .child(
                            div()
                                .text_size(px(11.))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(MUTED))
                                .child("Subagents (Actors):"),
                        )
                        .children(run.actors.iter().map(|a| self.render_actor_row(a))),
                )
            })
            .when(!run.nodes.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .pt_1()
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .child(
                            div()
                                .text_size(px(11.))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(MUTED))
                                .child("Execution Nodes:"),
                        )
                        .children(
                            run.nodes
                                .iter()
                                .rev()
                                .take(6)
                                .map(|n| self.render_node_row(n)),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_actor_row(&self, actor: &WorkflowRunActor) -> AnyElement {
        let is_running = actor.status == "running";
        let badge_color = if is_running { ACCENT } else { MUTED };
        let name = actor.name.as_deref().unwrap_or("subagent");

        div()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(rgb(PANEL))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(11.))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(TEXT))
                            .child(name.to_string()),
                    )
                    .when_some(actor.phase_name.as_deref(), |el, p| {
                        el.child(
                            div()
                                .text_size(px(10.))
                                .text_color(rgb(MUTED))
                                .child(format!("({p})")),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(badge_color))
                    .child(actor.status.clone()),
            )
            .into_any_element()
    }

    fn render_node_row(&self, node: &WorkflowRunNode) -> AnyElement {
        let is_settled = node.phase == "settled";
        let is_executing = node.phase == "executing";
        let phase_color = if is_settled {
            SUCCESS
        } else if is_executing {
            ACCENT
        } else {
            MUTED
        };

        div()
            .flex()
            .flex_col()
            .gap_0p5()
            .px_2()
            .py_1()
            .rounded_sm()
            .bg(rgb(PANEL))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(10.5))
                                    .text_color(rgb(TOOL))
                                    .child(node.kind.clone().unwrap_or_else(|| "step".into())),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(phase_color))
                                    .child(node.phase.clone()),
                            ),
                    )
                    .when_some(node.outcome.as_deref(), |el, out| {
                        el.child(
                            div()
                                .text_size(px(10.))
                                .text_color(if out == "ok" {
                                    rgb(SUCCESS)
                                } else {
                                    rgb(DANGER)
                                })
                                .child(out.to_string()),
                        )
                    }),
            )
            .when_some(node.instructions_head.as_deref(), |el, head| {
                el.child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(TEXT))
                        .child(head.to_string()),
                )
            })
            .when_some(node.last_tool.as_ref(), |el, tool| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .text_size(px(9.5))
                        .text_color(rgb(MUTED))
                        .child(format!("tool: {}", tool.name))
                        .when_some(tool.target.as_deref(), |el, t| el.child(format!("→ {t}"))),
                )
            })
            .into_any_element()
    }
}
