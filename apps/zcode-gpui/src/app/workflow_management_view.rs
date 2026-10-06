use crate::app::root::RootView;
use crate::backend::workflow_management::WorkflowMutation;
use crate::shared::{
    i18n::label,
    theme::{DANGER, MUTED, ui_size},
    theme_colors::color as rgb,
};
use ely_gpui_component::buttons::{Button, ButtonVariant};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};

impl RootView {
    pub(crate) fn workflow_management_section(
        &self,
        key: &str,
        scope: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state.read(cx);
        let Some(ws) = state.ws(key) else {
            return div().into_any_element();
        };
        let pending =
            state.workflow_management_pending(key) || state.saved_workflow_launch_pending(key);
        let writable = ws.started && ws.inbound.is_some() && !state.is_read_only_view();
        let inspected = ws
            .workflow_definitions
            .get(&format!("{scope}\0{name}"))
            .is_some_and(|q| q.value.is_some() && !q.loading && q.error.is_none());
        let form = ws
            .workflow_management
            .as_ref()
            .filter(|f| f.baseline.scope == scope && f.baseline.name == name);
        let mut body = div().flex().flex_col().gap_1().text_size(px(ui_size(12.)));
        let mut targets: Vec<String> = vec![];
        if let Some(form) = form {
            for (id, title, input) in [
                (
                    "workflow-meta-description",
                    label("Description", "描述"),
                    form.description.clone(),
                ),
                (
                    "workflow-meta-when",
                    label("When to use (optional)", "适用场景（可选）"),
                    form.when_to_use.clone(),
                ),
                (
                    "workflow-meta-args",
                    label("Argument declarations (JSON)", "参数声明（JSON）"),
                    form.args.clone(),
                ),
            ] {
                let field = div().child(input);
                #[cfg(test)]
                let field = crate::app::test_support::track_children(field, vec![id.into()]);
                let _ = id;
                body = body.child(title).child(field);
            }
            let can_save = writable && !pending && !form.needs_reload && form.dirty(cx);
            let feedback = form.error.clone().or_else(|| form.metadata(cx).err());
            body = body.children(feedback.map(|e| div().text_color(rgb(DANGER)).child(e)));
            if form.needs_reload {
                body = body.child(div().text_color(rgb(MUTED)).child(label(
                    "Reload definition before another write. Your edits are retained.",
                    "再次写入前请重新加载定义。编辑内容会保留。",
                )));
            }
            let mut actions = div().flex().flex_wrap().gap_1();
            let owner = key.to_owned();
            let token = form.token.clone();
            actions = actions.child(
                Button::new("workflow-meta-save", label("Save metadata", "保存元数据"))
                    .disabled(!can_save)
                    .loading(pending)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |s, cx| {
                            if s.workflow_management_matches(&owner, &token) {
                                s.submit_workflow_management(&owner, WorkflowMutation::Metadata, cx)
                            }
                        })
                    })),
            );
            targets.push("workflow-meta-save".into());
            let (owner, scope, name) = (key.to_owned(), scope.to_owned(), name.to_owned());
            actions = actions.child(
                Button::new(
                    "workflow-meta-reload",
                    label("Reload definition", "重新加载定义"),
                )
                .variant(ButtonVariant::Secondary)
                .disabled(
                    pending
                        || !writable
                        || ws
                            .workflow_definitions
                            .get(&format!("{}\0{}", form.baseline.scope, form.baseline.name))
                            .is_some_and(|q| q.loading),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state.update(cx, |s, cx| {
                        s.reload_workflow_management(&owner, &scope, &name, cx)
                    })
                })),
            );
            targets.push("workflow-meta-reload".into());
            for (id, title, action) in [
                (
                    "workflow-delete",
                    label("Delete", "删除"),
                    WorkflowMutation::Delete,
                ),
                (
                    "workflow-move",
                    label("Move to project", "移动到项目"),
                    WorkflowMutation::Move,
                ),
            ] {
                if action == WorkflowMutation::Move
                    && (form.baseline.scope != "global"
                        || ws.purpose != crate::backend::workspace::WorkspacePurpose::Project)
                {
                    continue;
                }
                let owner = key.to_owned();
                let token = form.token.clone();
                targets.push(id.into());
                actions = actions.child(
                    Button::new(id, title)
                        .variant(ButtonVariant::Secondary)
                        .disabled(pending || !writable || form.needs_reload)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.update(cx, |s, cx| {
                                if s.workflow_management_matches(&owner, &token) {
                                    s.request_workflow_confirmation(&owner, action.clone(), cx)
                                }
                            })
                        })),
                );
            }
            #[cfg(test)]
            let actions = crate::app::test_support::track_children(actions, targets.clone());
            body = body.child(actions);
            if let Some(action) = &form.confirmation {
                let question = if *action == WorkflowMutation::Delete {
                    format!("Delete {}/{}?", form.baseline.scope, form.baseline.name)
                } else {
                    format!(
                        "Move global/{} to project {}? Existing files will not be overwritten.",
                        form.baseline.name,
                        ws.path.display()
                    )
                };
                body = body.child(question);
                let owner = key.to_owned();
                let token = form.token.clone();
                let action = action.clone();
                let confirm = div().flex().flex_wrap().gap_1().child(
                    Button::new("workflow-confirm", label("Confirm", "确认"))
                        .disabled(pending || !writable)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.update(cx, |s, cx| {
                                if s.workflow_management_matches(&owner, &token) {
                                    s.submit_workflow_management(&owner, action.clone(), cx)
                                }
                            })
                        })),
                );
                let owner = key.to_owned();
                let token = form.token.clone();
                let confirm = confirm.child(
                    Button::new("workflow-cancel", label("Cancel", "取消"))
                        .variant(ButtonVariant::Secondary)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.state.update(cx, |s, cx| {
                                if s.workflow_management_matches(&owner, &token) {
                                    s.cancel_workflow_confirmation(&owner, cx)
                                }
                            })
                        })),
                );
                #[cfg(test)]
                let confirm = crate::app::test_support::track_children(
                    confirm,
                    vec!["workflow-confirm".into(), "workflow-cancel".into()],
                );
                body = body.child(confirm);
            }
        } else {
            let (owner, scope, name) = (key.to_owned(), scope.to_owned(), name.to_owned());
            let button = div().child(
                Button::new("workflow-manage", label("Manage definition", "管理定义"))
                    .variant(ButtonVariant::Secondary)
                    .disabled(!inspected || pending || !writable)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |s, cx| {
                            s.open_workflow_management(&owner, &scope, &name, cx)
                        })
                    })),
            );
            #[cfg(test)]
            let button =
                crate::app::test_support::track_children(button, vec!["workflow-manage".into()]);
            body = body.child(button);
        }
        body.into_any_element()
    }
}
