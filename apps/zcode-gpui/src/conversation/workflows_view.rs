//! Visual inspector for workflow runs, subagent actors roster, and node execution progress.
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/workflow-runs.ts and PARITY.md M7.

use crate::app::root::RootView;
use crate::conversation::workflows::{WorkflowRunActor, WorkflowRunNode, WorkflowRunState};
use crate::shared::theme::ui_size;
use crate::shared::theme::{ACCENT, BORDER, CARD, DANGER, MUTED, PANEL, SUCCESS, TEXT, TOOL};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, Styled, div, prelude::*,
    px,
};

impl RootView {
    pub(crate) fn workflows_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.state
            .update(cx, |s, cx| s.ensure_workflow_artifacts(false, cx));
        let saved = self.saved_workflows_section(cx);
        let active_sid = self.state.read(cx).active.clone();
        let focused = self
            .state
            .read(cx)
            .workflow_focus()
            .map(|f| (f.run.clone(), f.tool.clone()));
        let mut runs = active_sid
            .as_ref()
            .and_then(|sid| self.state.read(cx).conversations.get(sid))
            .map(|c| c.workflow_runs.runs.clone())
            .unwrap_or_default();
        if let Some((run, tool)) = focused {
            runs.retain(|r| r.run_id == run && r.tool_call_id.as_deref() == Some(&tool));
        }

        div()
            .flex_1()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .min_h_0()
            .id("workflows-pane-scroll")
            .overflow_y_scroll()
            .child(saved)
            .children(self.workflow_all_runs_control(cx))
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
                            .text_size(px(ui_size(12.)))
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
                    .text_size(px(ui_size(12.)))
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
        let state = self.state.read(cx);
        let owner = state.active_ws_key().zip(state.active.clone());
        let pending = owner
            .as_ref()
            .is_some_and(|(workspace, sid)| state.workflow_resume_pending(workspace, sid, &run_id));
        let can_resume = run.resumable == Some(true)
            && run.status == "stopped"
            && run.superseded_by.is_none()
            && !state.is_read_only_view();

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
                    .flex_col()
                    .items_start()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(ui_size(12.)))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .child(format!("Run #{}", idx + 1)),
                            )
                            .child(
                                div()
                                    .text_size(px(ui_size(10.)))
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
                            .flex_wrap()
                            .gap_1()
                            .when(can_resume, |el| {
                                let rid = run_id.clone();
                                let owner = owner.clone();
                                el.child(
                                    ely_gpui_component::buttons::Button::new(
                                        gpui::SharedString::from(format!("resume-wf-{idx}")),
                                        crate::shared::i18n::label("Resume", "继续"),
                                    )
                                    .size(ely_gpui_component::theme::ControlSize::Sm)
                                    .disabled(pending)
                                    .loading(pending)
                                    .on_click(cx.listener(
                                        move |this, _, _, cx| {
                                            if let Some((workspace, sid)) = &owner {
                                                this.state.update(cx, |state, cx| {
                                                    state.resume_workflow_run_for(
                                                        workspace, sid, &rid, cx,
                                                    )
                                                });
                                            }
                                        },
                                    )),
                                )
                            })
                            .children(self.workflow_open_run_control(
                                &run_id,
                                run.tool_call_id.as_deref(),
                                cx,
                            ))
                            .children(self.workflow_navigation_control(&run_id, cx))
                            .children(self.workflow_settings_control(&run_id, cx))
                            .when(is_running, |el| {
                                // 工作流取消必须指向 runId；foreground stop 可能误停当前模型回合。
                                el.children(self.activity_cancel_button(
                                    format!("stop-wf-{idx}"),
                                    run_id.clone(),
                                    cx,
                                ))
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_3()
                    .text_size(px(ui_size(10.5)))
                    .text_color(rgb(MUTED))
                    .child(format!("Tokens: {}", run.usage.spent_tokens))
                    .child(format!("Nodes: {}", run.usage.nodes_used))
                    .when_some(run.subagent_model.as_deref(), |el, model| {
                        el.child(format!("Model: {model}"))
                    }),
            )
            .children(self.workflow_settings_form(&run_id, cx))
            .children(self.workflow_artifacts_section(&run_id, cx))
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
                                .text_size(px(ui_size(11.)))
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
                                .text_size(px(ui_size(11.)))
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
                            .text_size(px(ui_size(11.)))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(TEXT))
                            .child(name.to_string()),
                    )
                    .when_some(actor.phase_name.as_deref(), |el, p| {
                        el.child(
                            div()
                                .text_size(px(ui_size(10.)))
                                .text_color(rgb(MUTED))
                                .child(format!("({p})")),
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(ui_size(10.)))
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
                                    .text_size(px(ui_size(10.5)))
                                    .text_color(rgb(TOOL))
                                    .child(node.kind.clone().unwrap_or_else(|| "step".into())),
                            )
                            .child(
                                div()
                                    .text_size(px(ui_size(10.)))
                                    .text_color(rgb(phase_color))
                                    .child(node.phase.clone()),
                            ),
                    )
                    .when_some(node.outcome.as_deref(), |el, out| {
                        el.child(
                            div()
                                .text_size(px(ui_size(10.)))
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
                        .text_size(px(ui_size(10.)))
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
                        .text_size(px(ui_size(9.5)))
                        .text_color(rgb(MUTED))
                        .child(format!("tool: {}", tool.name))
                        .when_some(tool.target.as_deref(), |el, t| el.child(format!("→ {t}"))),
                )
            })
            .into_any_element()
    }
}
