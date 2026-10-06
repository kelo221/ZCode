use crate::app::root::RootView;
use crate::app::store::AppState;
use crate::backend::workspace::{CommandCtx, Pending};
use crate::shared::i18n::label;
use ely_gpui_component::buttons::Button;
use gpui::{AnyElement, Context, ParentElement, Styled, div, prelude::*};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GoalState {
    pub objective: String,
    pub status: String,
    pub iteration: u64,
}
impl GoalState {
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        Some(Self {
            objective: value.get("objective")?.as_str()?.into(),
            status: value.get("status")?.as_str()?.into(),
            iteration: value.get("iteration")?.as_u64()?,
        })
    }
}

#[derive(Default, Clone)]
pub(crate) struct GoalAvailability {
    pub pause: bool,
    pub resume: bool,
}
impl GoalAvailability {
    pub(crate) fn from_value(value: &Value) -> Self {
        Self {
            pause: value.pointer("/pauseGoal/allowed").and_then(Value::as_bool) == Some(true),
            resume: value
                .pointer("/resumeGoal/allowed")
                .and_then(Value::as_bool)
                == Some(true),
        }
    }
}

impl AppState {
    pub(crate) fn goal_action_pending(&self, workspace: &str, sid: &str) -> bool {
        self.ws(workspace).is_some_and(|ws| ws.pending.values().any(|pending| matches!(pending, Pending::Command(ctx) if ctx.sid == sid && matches!(ctx.ctype.as_str(), "pauseGoal" | "resumeGoal"))))
    }

    pub(crate) fn change_goal(
        &mut self,
        workspace: &str,
        sid: &str,
        resume: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let allowed = self.conversations.get(sid).is_some_and(|conv| {
            conv.revision_known
                && conv.goal.is_some()
                && if resume {
                    conv.goal_availability.resume
                } else {
                    conv.goal_availability.pause
                }
        });
        if self.active_ws_key().as_deref() != Some(workspace)
            || self.active.as_deref() != Some(sid)
            || self.is_read_only_view()
            || !allowed
            || self.goal_action_pending(workspace, sid)
        {
            self.push_error(
                label("Goal action is currently unavailable", "目标操作当前不可用").into(),
            );
            return false;
        }
        let ok = self.send_session_command(
            workspace,
            CommandCtx::new(
                sid,
                if resume { "resumeGoal" } else { "pauseGoal" },
                json!({}),
            )
            .cas(),
        );
        cx.notify();
        ok
    }
}

impl RootView {
    pub(crate) fn goal_section(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.state.read(cx);
        let (workspace, sid) = state.active_ws_key().zip(state.active.clone())?;
        let conv = state.conversations.get(&sid)?;
        let goal = conv.goal.as_ref()?;
        let pending = state.goal_action_pending(&workspace, &sid);
        let writable = conv.revision_known && !state.is_read_only_view() && !pending;
        let controls = [
            (false, conv.goal_availability.pause, label("Pause", "暂停")),
            (true, conv.goal_availability.resume, label("Resume", "继续")),
        ]
        .into_iter()
        .filter(|(_, allowed, _)| *allowed)
        .map(|(resume, _, name)| {
            let (workspace, sid) = (workspace.clone(), sid.clone());
            let element = div().child(
                Button::new(if resume { "goal-resume" } else { "goal-pause" }, name)
                    .size(ely_gpui_component::theme::ControlSize::Sm)
                    .disabled(!writable)
                    .loading(pending)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |state, cx| {
                            state.change_goal(&workspace, &sid, resume, cx);
                        });
                    })),
            );
            #[cfg(test)]
            let element = crate::app::test_support::track_children(
                element,
                vec![if resume {
                    "goal-resume".into()
                } else {
                    "goal-pause".into()
                }],
            );
            element
        });
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .text_size(gpui::px(crate::shared::theme::font_size_sm()))
                .child(goal.objective.clone())
                .child(format!(
                    "{} · {} {}",
                    goal.status,
                    label("Iteration", "轮次"),
                    goal.iteration
                ))
                .child(div().flex().gap_1().children(controls))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
#[path = "goal_tests.rs"]
mod tests;
