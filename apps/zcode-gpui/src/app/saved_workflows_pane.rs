use crate::app::root::RootView;
use crate::shared::theme::{BORDER, CARD, DANGER, MUTED, TEXT, ui_size};
use crate::shared::theme_colors::color as rgb;
use crate::shared::{i18n::label, saved_workflows::SavedWorkflowEntry};
use ely_gpui_component::buttons::{Button, ButtonVariant};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};

impl RootView {
    pub(crate) fn saved_workflows_section(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(key) = self.state.read(cx).active_ws_key() else {
            return div()
                .child(label(
                    "Connect a workspace to browse saved workflows",
                    "连接工作区以浏览已保存工作流",
                ))
                .into_any_element();
        };
        let scope = self
            .state
            .read(cx)
            .ws(&key)
            .unwrap()
            .saved_workflow_form
            .scope
            .clone();
        self.state
            .update(cx, |s, cx| s.fetch_saved_workflows(&scope, false, cx));
        let ws = self.state.read(cx).ws(&key).unwrap();
        let query = ws.saved_workflows.get(&scope);
        let list = query.and_then(|q| q.value.clone());
        let loading = query.is_some_and(|q| q.loading);
        let error = query.and_then(|q| q.error.clone());
        let pending = self.state.read(cx).saved_workflow_launch_pending(&key)
            || self.state.read(cx).workflow_management_pending(&key);
        let selected = ws.saved_workflow_form.selected.clone();
        let form_error = ws.saved_workflow_form.error.clone();
        let mut tabs = div().flex().flex_wrap().gap_1();
        for next in ["project", "global"] {
            let owner = key.clone();
            tabs = tabs.child(
                Button::new(
                    format!("saved-scope-{next}"),
                    if next == "project" {
                        label("Project", "项目")
                    } else {
                        label("Global", "全局")
                    },
                )
                .variant(ButtonVariant::Secondary)
                .disabled(pending)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state.update(cx, |s, cx| {
                        if s.active_ws_key().as_deref() != Some(&owner)
                            || s.saved_workflow_launch_pending(&owner)
                            || s.workflow_management_pending(&owner)
                        {
                            return;
                        }
                        let form = &mut s.ws_mut(&owner).unwrap().saved_workflow_form;
                        if form.scope != next {
                            form.scope = next.into();
                            form.selected = None;
                            form.inputs.clear();
                            form.error = None;
                            form.baseline = None;
                            form.generation = uuid::Uuid::now_v7().to_string();
                        }
                        s.fetch_saved_workflows(next, false, cx);
                        cx.notify();
                    })
                })),
            );
        }
        let owner = key.clone();
        let refresh_scope = scope.clone();
        tabs = tabs.child(
            Button::new("saved-refresh", label("Refresh", "刷新"))
                .disabled(loading)
                .loading(loading)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state.update(cx, |s, cx| {
                        if s.active_ws_key().as_deref() == Some(&owner) {
                            s.fetch_saved_workflows(&refresh_scope, true, cx);
                        }
                    })
                })),
        );
        #[cfg(test)]
        let tabs = crate::app::test_support::track_children(
            tabs,
            vec![
                "saved-scope-project".into(),
                "saved-scope-global".into(),
                "saved-refresh".into(),
            ],
        );
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .text_size(px(ui_size(12.)))
                    .text_color(rgb(TEXT))
                    .child(label("Saved workflows", "已保存工作流")),
            )
            .child(tabs)
            .children(loading.then(|| div().child(label("Loading…", "加载中…"))))
            .children(
                error
                    .into_iter()
                    .chain(form_error)
                    .map(|e| div().text_color(rgb(DANGER)).child(e)),
            );
        if let Some(list) = list {
            body = body.children(list.invalid.into_iter().map(|i| {
                div()
                    .text_color(rgb(DANGER))
                    .child(format!("{}: {}", i.path, i.reason))
            }));
            if list.workflows.is_empty() {
                body = body.child(div().text_color(rgb(MUTED)).child(label(
                    "No saved workflows in this scope",
                    "此范围没有已保存工作流",
                )));
            }
            for entry in list.workflows {
                let owner = key.clone();
                let scope = entry.scope.clone();
                let name = entry.name.clone();
                let button = div().child(
                    Button::new(format!("saved-select-{name}"), name.clone())
                        .disabled(pending)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.update(cx, |s, cx| {
                                s.select_saved_workflow(&owner, &scope, &name, cx)
                            })
                        })),
                );
                #[cfg(test)]
                let button = crate::app::test_support::track_children(
                    button,
                    vec![format!("saved-select-{}", entry.name)],
                );
                body = body.child(button).child(
                    div()
                        .text_color(rgb(MUTED))
                        .child(entry.description.clone()),
                );
                if selected.as_deref() == Some(&entry.name) {
                    body = body.child(self.saved_workflow_arguments(&key, &entry, pending, cx));
                }
            }
        }
        body.into_any_element()
    }

    fn saved_workflow_arguments(
        &self,
        key: &str,
        entry: &SavedWorkflowEntry,
        pending: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let inputs = self
            .state
            .read(cx)
            .ws(key)
            .unwrap()
            .saved_workflow_form
            .inputs
            .clone();
        let mut body = div().flex().flex_col().gap_1().p_2().bg(rgb(CARD));
        if let Some(text) = &entry.when_to_use {
            body = body.child(text.clone());
        }
        for (name, arg) in &entry.args {
            if let Some(input) = inputs.get(name) {
                let field = div().child(input.clone());
                #[cfg(test)]
                let field = crate::app::test_support::track_children(
                    field,
                    vec![format!("saved-arg-{name}")],
                );
                body = body
                    .child(format!(
                        "{} ({}){}",
                        name,
                        arg.kind,
                        if arg.required == Some(true) { " *" } else { "" }
                    ))
                    .children(arg.description.clone())
                    .children(arg.default.as_ref().map(|v| format!("Default: {v}")))
                    .child(field);
            }
        }
        let (owner, scope, name) = (key.to_owned(), entry.scope.clone(), entry.name.clone());
        let launch = div().child(
            Button::new("saved-launch", label("Run in new chat", "在新会话中运行"))
                .disabled(pending)
                .loading(pending)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state.update(cx, |s, cx| {
                        s.launch_saved_workflow(&owner, &scope, &name, cx)
                    })
                })),
        );
        #[cfg(test)]
        let launch = crate::app::test_support::track_children(launch, vec!["saved-launch".into()]);
        body.child(launch)
            .child(self.workflow_definition_section(key, &entry.scope, &entry.name, cx))
            .child(self.workflow_history_section(key, &entry.scope, &entry.name, cx))
            .child(self.workflow_management_section(key, &entry.scope, &entry.name, cx))
            .into_any_element()
    }
}
