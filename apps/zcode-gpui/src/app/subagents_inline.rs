use crate::app::{
    root::RootView,
    subagent_profiles::{AgentSummary, OverrideDraft, OverridePayload, model_choices},
};
use crate::shared::i18n::label;
use ely_gpui_component::forms::{Choice, Select};
use gpui::{AnyElement, Context, IntoElement, SharedString, div, prelude::*, px};

impl RootView {
    pub(super) fn profile_inline_controls(
        &self,
        agent: &AgentSummary,
        blocked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let models = self.state.read(cx).profiles.models.as_ref();
        let selection = agent.model_selection_override.as_ref();
        let mut choices = vec![Choice::new("", label("Inherit model", "继承模型"))];
        if let Some(view) = models {
            for group in model_choices(view, "") {
                for choice in group.choices {
                    choices.push(Choice::new(
                        format!(
                            "{}/{}",
                            choice.selection.provider_id, choice.selection.model_id
                        ),
                        format!("{} / {}", group.label, choice.selection.model_id),
                    ));
                }
            }
        }
        let selected = selection
            .map(|s| format!("{}/{}", s.provider_id, s.model_id))
            .unwrap_or_default();
        let unavailable =
            selection.is_some_and(|s| models.is_none_or(|v| v.validate_selection(s).is_err()));
        if unavailable {
            choices.push(Choice::new(
                selected.clone(),
                format!(
                    "{} · {}",
                    selection.unwrap().model_id,
                    label("unavailable", "不可用")
                ),
            ));
        }
        let target = agent.clone();
        let scope = self.subagents.scope.clone();
        let mut controls = div().flex().flex_wrap().gap_2().min_w_0().child(
            div().w(px(210.)).child(
                Select::new(format!("profile-model-{}", agent.id), choices)
                    .selected(selected)
                    .disabled(blocked || models.is_none())
                    .on_change(cx.listener(move |this, value: &SharedString, _, cx| {
                        this.submit_inline_selection(&scope, &target, value, false, cx)
                    })),
            ),
        );
        let levels = selection
            .and_then(|s| models.and_then(|v| v.model(&s.provider_id, &s.model_id)))
            .map(|m| m.reasoning_levels.clone())
            .unwrap_or_default();
        if !levels.is_empty() {
            let target = agent.clone();
            let scope = self.subagents.scope.clone();
            controls = controls.child(
                div().w(px(112.)).child(
                    Select::new(
                        format!("profile-reasoning-{}", agent.id),
                        levels.into_iter().map(|l| Choice::new(l.clone(), l)),
                    )
                    .selected(
                        selection
                            .and_then(|s| s.reasoning_level())
                            .unwrap_or_default()
                            .to_owned(),
                    )
                    .disabled(blocked || unavailable)
                    .on_change(cx.listener(
                        move |this, value: &SharedString, _, cx| {
                            this.submit_inline_selection(&scope, &target, value, true, cx)
                        },
                    )),
                ),
            );
        }
        controls.into_any_element()
    }

    pub(crate) fn submit_inline_selection(
        &mut self,
        scope: &str,
        agent: &AgentSummary,
        value: &str,
        reasoning: bool,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.read(cx);
        if self.subagents.scope != scope
            || !state.profiles.ready()
            || state.profiles.mutation_pending
            || state.profiles.uncertain
        {
            return;
        }
        let Some(view) = state.profiles.models.as_ref() else {
            return;
        };
        let Ok(mut draft) = OverrideDraft::from_agent(agent) else {
            return;
        };
        let result = if reasoning {
            draft.set_reasoning(view, Some(value))
        } else if value.is_empty() {
            draft.clear();
            Ok(())
        } else if let Some((provider, model)) = value.split_once('/') {
            draft.select_model(view, provider, model)
        } else {
            return;
        };
        if result.is_err() || draft.selection == agent.model_selection_override {
            return;
        }
        let (method, params) = match draft.payload() {
            OverridePayload::BuiltIn(p) => {
                ("setBuiltInModelOverride", serde_json::to_value(p).unwrap())
            }
            OverridePayload::Plugin(p) => (
                "setPluginAgentModelOverride",
                serde_json::to_value(p).unwrap(),
            ),
        };
        let receipt = uuid::Uuid::now_v7().to_string();
        self.subagents.action_receipt = Some(receipt.clone());
        self.state.update(cx, |s, cx| {
            s.submit_profile_mutation(
                receipt,
                scope.into(),
                method,
                params,
                Some(agent.clone()),
                cx,
            )
        });
        cx.notify();
    }
}
