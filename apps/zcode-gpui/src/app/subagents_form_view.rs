use crate::app::root::RootView;
use crate::app::subagent_profiles::{
    AgentColor, AgentSummary, FormDraft, ToolsMode, model_choices,
};
use crate::shared::i18n::label;
use ely_gpui_component::forms::{Choice, Input, InputEvent, Select, Switch, TextInput};
use gpui::{
    AnyElement, AppContext, Context, Entity, Focusable, IntoElement, SharedString, Window, div,
    prelude::*,
};
use std::collections::BTreeMap;

pub(crate) struct ProfileFormView {
    pub id: String,
    pub baseline: Option<AgentSummary>,
    pub draft: FormDraft,
    pub inputs: BTreeMap<&'static str, Entity<TextInput>>,
    pub scope: String,
    pub origin: crate::app::subagent_profiles::WorkspaceContext,
    pub receipt: String,
    pub overriding: bool,
    pub confirm_delete: bool,
    pub error: Option<String>,
}

impl RootView {
    pub(crate) fn open_profile_form(
        &mut self,
        baseline: Option<AgentSummary>,
        overriding: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let profiles = &self.state.read(cx).profiles;
        if !profiles.ready() || profiles.mutation_pending || profiles.uncertain {
            return;
        }
        let mut draft = match baseline.as_ref().filter(|_| !overriding) {
            Some(agent) => match FormDraft::edit(agent) {
                Ok(d) => d,
                Err(_) => return,
            },
            None => FormDraft::new(),
        };
        if overriding {
            let Some(agent) = baseline.as_ref().filter(|a| a.override_target().is_some()) else {
                return;
            };
            draft.fields.model_selection = agent.model_selection_override.clone();
        }
        let values = [
            ("name", draft.fields.name.clone()),
            ("description", draft.fields.description.clone()),
            ("prompt", draft.fields.system_prompt.clone()),
            (
                "tools",
                draft.fields.tools.clone().unwrap_or_default().join(", "),
            ),
        ];
        let inputs: BTreeMap<_, _> = values
            .into_iter()
            .map(|(key, text)| {
                let input = cx.new(|cx| {
                    let mut input = TextInput::new(window, cx).placeholder(key);
                    if key == "prompt" {
                        input = input.multi_line(4, 8);
                    }
                    input.set_text(text, cx);
                    input
                });
                cx.subscribe(&input, |this, _, event: &InputEvent, cx| {
                    if *event == InputEvent::Changed {
                        if let Some(form) = this.subagents.form.as_mut() {
                            form.confirm_delete = false;
                        }
                        cx.notify();
                    }
                })
                .detach();
                (key, input)
            })
            .collect();
        let focus = inputs["name"].read(cx).focus_handle(cx);
        self.subagents.form = Some(ProfileFormView {
            id: uuid::Uuid::now_v7().to_string(),
            baseline,
            draft,
            inputs,
            scope: self.subagents.scope.clone(),
            origin: self
                .state
                .read(cx)
                .profile_context(&self.subagents.scope)
                .unwrap_or_default(),
            receipt: String::new(),
            overriding,
            confirm_delete: false,
            error: None,
        });
        if !overriding {
            window.focus(&focus, cx);
        }
        cx.notify();
    }

    pub(crate) fn profile_form_current(&self, id: &str, cx: &Context<Self>) -> bool {
        self.subagents.form.as_ref().is_some_and(|f| f.id == id)
            && self.state.read(cx).profiles.ready()
            && !self.state.read(cx).profiles.mutation_pending
            && !self.state.read(cx).profiles.uncertain
    }

    pub(crate) fn render_profile_form(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let blocked = !self.state.read(cx).profiles.ready()
            || self.state.read(cx).profiles.mutation_pending
            || self.state.read(cx).profiles.uncertain;
        let inputs = self
            .subagents
            .form
            .as_ref()
            .unwrap()
            .inputs
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for input in inputs {
            if input.read(cx).is_disabled() != blocked {
                input.update(cx, |input, cx| input.set_disabled(blocked, cx));
            }
        }
        let form = self.subagents.form.as_ref().unwrap();
        let state = self.state.read(cx);
        let selection = form.draft.fields.model_selection.clone();
        let models = state.profiles.models.clone();
        let outcome = state.profiles.outcomes.get(&form.receipt).cloned();
        let mut body = div().flex().flex_col().gap_3().child(if form.overriding {
            label("Model override", "模型覆盖")
        } else {
            label("Custom Subagent", "自定义子智能体")
        });
        if form.baseline.is_some() && !form.overriding {
            body = body.child(label("Saving rewrites supported metadata. Unknown YAML fields and comments may be discarded.", "保存会重写受支持的元数据。未知 YAML 字段和注释可能丢失。"));
        }
        if let Some(error) = form.error.clone() {
            body = body.child(error);
        }
        if let Some(outcome) = outcome {
            body = body.child(match outcome {
                Ok(()) => label(
                    "Saved. Refresh may still be pending.",
                    "已保存，列表可能仍在刷新。",
                )
                .into(),
                Err(error) => error,
            });
        }
        if !form.overriding {
            for (key, text) in [
                ("name", label("Name", "名称")),
                ("description", label("Description", "描述")),
                ("prompt", label("System prompt", "系统提示词")),
            ] {
                let field = div().child(Input::new(&form.inputs[key]));
                #[cfg(test)]
                let field = crate::app::test_support::track_children(
                    field,
                    vec![format!("profile-input-{key}")],
                );
                body = body.child(div().flex().flex_col().gap_2().child(text).child(field));
            }
            let weak = cx.entity().downgrade();
            let id = form.id.clone();
            body = body.child(
                Select::new(
                    "profile-tools-mode",
                    [
                        Choice::new("all", label("All tools", "全部工具")),
                        Choice::new("custom", label("Custom tools", "自定义工具")),
                    ],
                )
                .selected(if form.draft.tools_mode == ToolsMode::All {
                    "all"
                } else {
                    "custom"
                })
                .disabled(blocked)
                .on_change(move |value, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if !this.profile_form_current(&id, cx) {
                            return;
                        }
                        if let Some(f) = this.subagents.form.as_mut() {
                            f.draft.set_tools_mode(if value.as_ref() == "all" {
                                ToolsMode::All
                            } else {
                                ToolsMode::Custom
                            });
                        }
                        cx.notify();
                    });
                }),
            );
            if form.draft.tools_mode == ToolsMode::Custom {
                body = body.child(
                    div()
                        .child(label("Comma-separated tool names", "逗号分隔的工具名"))
                        .child(Input::new(&form.inputs["tools"])),
                );
            }
            let weak = cx.entity().downgrade();
            let id = form.id.clone();
            body = body.child(
                Switch::new(
                    "profile-inject-agents",
                    form.draft.fields.inject_agents_md.unwrap_or(true),
                )
                .label("AGENTS.md")
                .disabled(blocked)
                .on_change(move |value, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        if !this.profile_form_current(&id, cx) {
                            return;
                        }
                        if let Some(f) = this.subagents.form.as_mut() {
                            f.draft.fields.inject_agents_md = Some(value);
                        }
                        cx.notify();
                    });
                }),
            );
            let colors = [
                AgentColor::Red,
                AgentColor::Blue,
                AgentColor::Green,
                AgentColor::Yellow,
                AgentColor::Purple,
                AgentColor::Orange,
                AgentColor::Pink,
                AgentColor::Cyan,
            ];
            let choices = colors.into_iter().map(|color| {
                let text = color.as_str().to_owned();
                Choice::new(text.clone(), text)
            });
            let selected = form
                .draft
                .fields
                .color
                .map(|c| {
                    serde_json::to_value(c)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned()
                })
                .unwrap_or_default();
            let weak = cx.entity().downgrade();
            let id = form.id.clone();
            body = body.child(
                Select::new("profile-color", choices)
                    .selected(selected)
                    .disabled(blocked)
                    .on_change(move |value, _, cx| {
                        let _ = weak.update(cx, |this, cx| {
                            if !this.profile_form_current(&id, cx) {
                                return;
                            }
                            if let Some(f) = this.subagents.form.as_mut() {
                                f.draft.fields.color =
                                    serde_json::from_value(serde_json::json!(value.as_ref())).ok();
                            }
                            cx.notify();
                        });
                    }),
            );
        }
        let mut choices = vec![Choice::new("", label("Default / inherit", "默认 / 继承"))];
        if let Some(models) = &models {
            for group in model_choices(models, "") {
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
            .as_ref()
            .map(|s| format!("{}/{}", s.provider_id, s.model_id))
            .unwrap_or_default();
        let unavailable = selection.as_ref().is_some_and(|s| {
            models
                .as_ref()
                .is_some_and(|v| v.validate_selection(s).is_err())
        });
        if unavailable {
            choices.push(Choice::new(
                selected.clone(),
                format!(
                    "{} · {}",
                    selection.as_ref().unwrap().model_id,
                    label("unavailable", "不可用")
                ),
            ));
            body = body.child(label("The saved selection is unavailable. Choose a published model or clear it before saving.", "已保存的模型选择不可用。保存前请选择现有模型或清除选择。"));
        }
        let id = form.id.clone();
        body = body.child(
            Select::new("profile-model", choices)
                .selected(selected)
                .disabled(blocked || models.is_none())
                .on_change(cx.listener(move |this, value: &SharedString, _, cx| {
                    this.change_profile_selection(&id, value, false, cx)
                })),
        );
        let levels = selection
            .as_ref()
            .and_then(|s| {
                models
                    .as_ref()
                    .and_then(|view| view.model(&s.provider_id, &s.model_id))
            })
            .map(|m| m.reasoning_levels.clone())
            .unwrap_or_default();
        if !levels.is_empty() {
            let thought = selection
                .as_ref()
                .and_then(|s| s.reasoning_level())
                .unwrap_or_default()
                .to_owned();
            let choices = levels.into_iter().map(|s| Choice::new(s.clone(), s));
            let id = form.id.clone();
            body = body.child(
                Select::new("profile-reasoning", choices)
                    .selected(thought)
                    .disabled(blocked)
                    .on_change(cx.listener(move |this, value: &SharedString, _, cx| {
                        this.change_profile_selection(&id, value, true, cx)
                    })),
            );
        }
        if models.is_none() {
            body =
                body.child(state.profiles.model_error.clone().unwrap_or_else(|| {
                    label("Model metadata loading…", "正在加载模型信息…").into()
                }));
        }
        body.child(
            div()
                .pt_4()
                .border_t_1()
                .border_color(gpui::rgb(crate::shared::theme::active_theme().border))
                .child(self.profile_form_footer(blocked, cx)),
        )
        .into_any_element()
    }
}
