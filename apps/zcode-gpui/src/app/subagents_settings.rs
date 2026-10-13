use crate::app::root::RootView;
use crate::app::subagent_profiles::{SettingsScope, group_plugin_rows, group_rows};
use crate::shared::i18n::{label, t};
use crate::shared::theme::{active_theme, ui_size};
use ely_gpui_component::buttons::{Button, ButtonVariant, IconButton};
use ely_gpui_component::forms::{Choice, Input, InputEvent, Select, TextInput};
use ely_gpui_component::primitives::{Icon, IconName};
use ely_gpui_component::theme::{ControlSize, IconSize};
use gpui::{
    AnyElement, AppContext, Context, Entity, IntoElement, SharedString, Window, div, prelude::*, px,
};

pub(crate) struct SubagentsView {
    pub scope: String,
    pub search: Option<Entity<TextInput>>,
    pub form: Option<crate::app::subagents_form_view::ProfileFormView>,
    pub action_receipt: Option<String>,
    pub activation_confirm: bool,
    pub delete_target: Option<crate::app::subagent_profiles::AgentSummary>,
}
impl Default for SubagentsView {
    fn default() -> Self {
        Self {
            scope: "user".into(),
            search: None,
            form: None,
            action_receipt: None,
            activation_confirm: false,
            delete_target: None,
        }
    }
}

impl RootView {
    pub(crate) fn render_subagents_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.state.read(cx).profiles.ready() {
            return self.profile_activation(cx);
        }
        if self.subagents.form.as_ref().is_some_and(|f| {
            self.state
                .read(cx)
                .profiles
                .outcomes
                .get(&f.receipt)
                .is_some_and(Result::is_ok)
        }) {
            self.subagents.action_receipt = self.subagents.form.as_ref().map(|f| f.receipt.clone());
            self.subagents.form = None;
            window.focus(&self.settings.focus, cx);
        }
        if self.subagents.form.is_some() {
            let form = self.render_profile_form(window, cx);
            return if self.state.read(cx).profiles.uncertain {
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(self.profile_review(cx))
                    .child(form)
                    .into_any_element()
            } else {
                form
            };
        }
        let scope = self.subagents.scope.clone();
        self.state
            .update(cx, |s, cx| s.read_profiles(&scope, false, cx));
        if self.subagents.search.is_none() {
            let input = cx.new(|cx| {
                TextInput::new(window, cx).placeholder(t("settings.subagents.searchPlaceholder"))
            });
            cx.subscribe(&input, |_, _, event: &InputEvent, cx| {
                if *event == InputEvent::Changed {
                    cx.notify();
                }
            })
            .detach();
            self.subagents.search = Some(input);
        }
        let search = self.subagents.search.as_ref().unwrap().clone();
        let query = search.read(cx).text().to_string();
        let state = self.state.read(cx);
        let pending = state.profiles.mutation_pending;
        let uncertain = state.profiles.uncertain;
        let mut choices = vec![Choice::new("user", t("settings.plugin.scope.user"))];
        for ws in &state.workspaces {
            if state.profile_workspace(&ws.key).is_some() {
                choices.push(Choice::new(ws.key.clone(), ws.display.clone()));
            }
        }
        let selected_scope = if scope == "user" {
            SettingsScope::User
        } else {
            SettingsScope::Workspace(state.profile_context(&scope).unwrap_or_default())
        };
        let cache = state.profiles.queries.get(&scope);
        let rows = cache
            .and_then(|q| q.snapshot.as_ref())
            .map(|s| s.visible_rows(&selected_scope, &query))
            .unwrap_or_default();
        let error = cache
            .and_then(|q| q.error.clone())
            .or_else(|| state.profiles.connection_error.clone());
        let loading = cache.is_some_and(|q| q.loading);
        let can_create = cache
            .and_then(|q| q.snapshot.as_ref())
            .is_some_and(|s| scope != "user" || s.capability.user_scope_available);
        let outcome = self
            .subagents
            .action_receipt
            .as_ref()
            .and_then(|r| state.profiles.outcomes.get(r))
            .cloned();
        let local = state.profiles.isolated.is_none();
        let compact = f32::from(window.viewport_size().width) < 1000.;
        let theme = active_theme();
        let toolbar = div()
            .flex()
            .items_center()
            .gap_3()
            .flex_wrap()
            .child(
                div().w(px(160.)).child(
                    Select::new("subagents-scope", choices)
                        .selected(scope.clone())
                        .disabled(pending || uncertain)
                        .on_change(cx.listener(|this, value: &SharedString, _, cx| {
                            if this.state.read(cx).profiles.mutation_pending
                                || this.state.read(cx).profiles.uncertain
                                || this.state.read(cx).profile_workspace(value).is_none()
                            {
                                return;
                            }
                            this.subagents.scope = value.to_string();
                            this.subagents.action_receipt = None;
                            this.subagents.delete_target = None;
                            cx.notify();
                        })),
                ),
            )
            .child(div().w(px(1.)).h(px(16.)).bg(theme.border))
            // Desktop count label: "Subagents" plus a subtle count.
            .child(
                div()
                    .h(px(28.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_1()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .child(t("settings.subagents.title").to_owned())
                    .child(
                        div()
                            .text_size(px(ui_size(12.)))
                            .text_color(theme.subtle)
                            .child(rows.len().to_string()),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .w(px(if compact { 220. } else { 256. }))
                    .min_w_0()
                    .child(
                        Input::new(&search).size(ControlSize::Lg).prefix(
                            Icon::new(IconName::Search)
                                .size(IconSize::Sm)
                                .color(theme.subtle),
                        ),
                    ),
            );
        let mut body = div().flex().flex_col().gap_8().min_w_0().child(toolbar);
        if local {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().text_color(theme.subtle).child(label(
                        "Local manager · application preference saves paused",
                        "本地管理 · 应用偏好保存已暂停",
                    )))
                    .child(
                        Button::new("subagents-close", label("Close manager", "关闭管理"))
                            .size(ControlSize::Sm)
                            .variant(ButtonVariant::Ghost)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.update(cx, |s, cx| s.close_profiles(cx))
                            })),
                    ),
            );
        }
        if uncertain {
            body = body.child(self.profile_review(cx));
        }
        if let Some(error) = error {
            body = body.child(div().text_color(theme.destructive).child(error));
        }
        if let Some(outcome) = outcome {
            body = body.child(
                div().text_color(theme.subtle).child(match outcome {
                    Ok(()) => label(
                        "Saved; inventory refresh may still be pending",
                        "已保存；列表可能仍在刷新",
                    )
                    .to_owned(),
                    Err(e) => e,
                }),
            );
        }
        if loading {
            body = body.child(
                div()
                    .text_color(theme.subtle)
                    .child(label("Loading agents…", "正在加载智能体…")),
            );
        }
        if self.subagents.delete_target.is_some() {
            body = body.child(self.profile_delete_confirmation(cx));
        }
        let groups = group_rows(&rows);
        let owner = scope.clone();
        let controls = div()
            .flex()
            .items_center()
            .gap_2()
            .child(
                IconButton::new("subagents-refresh", IconName::RefreshCw)
                    .size(ControlSize::Md)
                    .variant(ButtonVariant::Outline)
                    .tooltip(label("Refresh", "刷新"))
                    .disabled(loading || pending)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state
                            .update(cx, |s, cx| s.read_profiles(&owner, true, cx))
                    })),
            )
            .child(
                Button::new("subagents-new", t("settings.create.action"))
                    .icon(IconName::Plus)
                    .size(ControlSize::Md)
                    .variant(ButtonVariant::Primary)
                    .disabled(pending || uncertain || !can_create)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_profile_form(None, false, window, cx)
                    })),
            );
        #[cfg(test)]
        let controls = crate::app::test_support::track_children(
            controls,
            vec!["subagents-refresh".into(), "subagents-new".into()],
        );
        let installed_header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(crate::app::subagents_rows::group_header(
                t("settings.subagents.group.user"),
                groups.user.len(),
            ))
            .child(controls);
        body = body.child(
            div()
                .flex()
                .flex_col()
                .gap_4()
                .child(installed_header)
                .child(self.profile_rows(groups.user, pending || uncertain, compact, cx)),
        );
        for (_, rows) in group_plugin_rows(&groups.plugin) {
            let name = rows
                .first()
                .and_then(|r| r.agent.plugin_name.as_deref())
                .map(crate::app::subagents_rows::canonical_plugin_name)
                .unwrap_or_else(|| label("Plugin", "插件").to_owned());
            body = body.child(self.profile_group(&name, rows, pending || uncertain, compact, cx));
        }
        body = body.child(self.profile_group(
            t("settings.subagents.group.builtIn"),
            groups.built_in,
            pending || uncertain,
            compact,
            cx,
        ));
        if rows.is_empty() && !loading {
            body = body.child(
                div()
                    .text_color(theme.subtle)
                    .child(label("No matching agents", "没有匹配的智能体")),
            );
        }
        body.into_any_element()
    }
}
