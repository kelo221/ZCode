use crate::app::root::RootView;
use crate::app::subagent_profiles::{EditableScope, OverrideDraft, OverridePayload};
use gpui::{Context, SharedString};

impl RootView {
    pub(crate) fn change_profile_selection(
        &mut self,
        id: &str,
        value: &SharedString,
        reasoning: bool,
        cx: &mut Context<Self>,
    ) {
        if !self.profile_form_current(id, cx) {
            return;
        }
        let Some(view) = self.state.read(cx).profiles.models.clone() else {
            return;
        };
        let Some(form) = self.subagents.form.as_mut() else {
            return;
        };
        let result = if form.overriding {
            let Some(agent) = &form.baseline else {
                return;
            };
            let Ok(mut draft) = OverrideDraft::from_agent(agent) else {
                return;
            };
            draft.selection = form.draft.fields.model_selection.clone();
            let result = if reasoning {
                draft.set_reasoning(&view, Some(value))
            } else if value.is_empty() {
                draft.clear();
                Ok(())
            } else if let Some((provider, model)) = value.split_once('/') {
                draft.select_model(&view, provider, model)
            } else {
                return;
            };
            form.draft.fields.model_selection = draft.selection;
            result
        } else if reasoning {
            form.draft.set_reasoning(&view, Some(value))
        } else if value.is_empty() {
            form.draft.clear_model();
            Ok(())
        } else if let Some((provider, model)) = value.split_once('/') {
            form.draft.select_model(&view, provider, model)
        } else {
            return;
        };
        form.error = result.err().map(|error| error.to_string());
        form.confirm_delete = false;
        cx.notify();
    }

    pub(crate) fn save_profile_form(&mut self, id: &str, deleting: bool, cx: &mut Context<Self>) {
        if !self.profile_form_current(id, cx) {
            return;
        }
        let Some(form) = self.subagents.form.as_mut() else {
            return;
        };
        let state = self.state.read(cx);
        let Some(current) = state.profile_context(&form.scope) else {
            return;
        };
        if form.scope != "user" && current != form.origin {
            form.error = Some("Originating workspace changed; reopen the form".into());
            cx.notify();
            return;
        }
        let context = form.origin.clone();
        form.error = None;
        if !deleting && let Some(selection) = &form.draft.fields.model_selection {
            let result = state
                .profiles
                .models
                .as_ref()
                .ok_or("Model metadata is unavailable".to_owned())
                .and_then(|view| {
                    view.validate_selection(selection)
                        .map_err(|e| e.to_string())
                });
            if let Err(error) = result {
                form.error = Some(error);
                cx.notify();
                return;
            }
        }
        let result = if deleting {
            form.baseline
                .as_ref()
                .and_then(|agent| agent.delete_params())
                .map(|params| ("deleteAgent", serde_json::to_value(params).unwrap()))
                .ok_or("Profile cannot be deleted".to_owned())
        } else if form.overriding {
            let Some(agent) = &form.baseline else {
                return;
            };
            OverrideDraft::from_agent(agent)
                .map_err(|e| e.to_string())
                .map(|mut draft| {
                    draft.selection = form.draft.fields.model_selection.clone();
                    match draft.payload() {
                        OverridePayload::BuiltIn(params) => (
                            "setBuiltInModelOverride",
                            serde_json::to_value(params).unwrap(),
                        ),
                        OverridePayload::Plugin(params) => (
                            "setPluginAgentModelOverride",
                            serde_json::to_value(params).unwrap(),
                        ),
                    }
                })
        } else {
            form.draft.fields.name = form.inputs["name"].read(cx).text().to_string();
            form.draft.fields.description = form.inputs["description"].read(cx).text().to_string();
            form.draft.fields.system_prompt = form.inputs["prompt"].read(cx).text().to_string();
            if form.draft.tools_mode == crate::app::subagent_profiles::ToolsMode::Custom {
                form.draft.fields.tools = Some(
                    form.inputs["tools"]
                        .read(cx)
                        .text()
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .collect(),
                );
            }
            if let Some(agent) = &form.baseline {
                form.draft
                    .update_params(agent, context)
                    .map(|p| ("updateAgent", serde_json::to_value(p).unwrap()))
            } else {
                let scope = if form.scope == "user" {
                    EditableScope::User
                } else {
                    EditableScope::Workspace
                };
                form.draft
                    .create_params(scope, context)
                    .map(|p| ("createAgent", serde_json::to_value(p).unwrap()))
            }
            .map_err(|e| e.to_string())
        };
        let (method, params) = match result {
            Ok(request) => request,
            Err(error) => {
                form.error = Some(error);
                cx.notify();
                return;
            }
        };
        form.receipt = uuid::Uuid::now_v7().to_string();
        let receipt = form.receipt.clone();
        let scope = form.scope.clone();
        let baseline = form.baseline.clone();
        self.state.update(cx, |s, cx| {
            s.submit_profile_mutation(receipt, scope, method, params, baseline, cx)
        });
        cx.notify();
    }
}
