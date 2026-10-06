use crate::app::root::RootView;
use crate::shared::{
    i18n::label,
    theme::{DANGER, MUTED},
    theme_colors::color as rgb,
};
use ely_gpui_component::buttons::Button;
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div};

impl RootView {
    pub(crate) fn workflow_history_section(
        &self,
        key: &str,
        scope: &str,
        name: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let query = self
            .state
            .read(cx)
            .ws(key)
            .and_then(|ws| ws.workflow_histories.get(&format!("{scope}\0{name}")));
        let history = query.and_then(|q| q.value.clone());
        let error = query.and_then(|q| q.error.clone());
        let loading = query.is_some_and(|q| q.loading);
        let attempted = query.is_some_and(|q| q.attempted);
        let (owner, query_scope, query_name) = (key.to_owned(), scope.to_owned(), name.to_owned());
        let button = div().child(
            Button::new(
                "saved-history",
                if attempted {
                    label("Refresh history", "刷新历史")
                } else {
                    label("Show history", "显示历史")
                },
            )
            .disabled(loading)
            .loading(loading)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.fetch_workflow_history(&owner, &query_scope, &query_name, true, cx)
                })
            })),
        );
        #[cfg(test)]
        let button = crate::app::test_support::track_children(button, vec!["saved-history".into()]);
        let mut body = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(button)
            .children(error.map(|e| div().text_color(rgb(DANGER)).child(e)));
        if let Some(history) = history {
            if history.runs.is_empty() {
                body = body.child(label("No run history returned", "未返回运行历史"));
            }
            if history.truncated == Some(true) {
                body = body.child(label(
                    "Only the latest 50 runs are shown",
                    "仅显示最近 50 次运行",
                ));
            }
            for run in history.runs {
                let mut row = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_color(rgb(MUTED))
                    .child(format!(
                        "{} · {} · {} tokens",
                        run.run_id, run.status, run.spent_tokens
                    ))
                    .child(format!(
                        "{}: {} / {}",
                        label("Created / updated", "创建 / 更新"),
                        run.created_at,
                        run.updated_at
                    ))
                    .children(run.stop_reason)
                    .children(run.cwd)
                    .children(run.args.map(|a| {
                        format!("Args: {}", serde_json::to_string(&a).unwrap_or_default())
                    }))
                    .children(run.artifacts.unwrap_or_default().into_iter().map(|a| {
                        div().child(format!(
                            "{}: {} v{} {} {}",
                            a.kind,
                            a.title.unwrap_or(a.id),
                            a.version,
                            a.content_type.unwrap_or_default(),
                            label("(summary)", "（摘要）")
                        ))
                    }));
                let can_open = self
                    .state
                    .read(cx)
                    .workflow_history_parent(key, scope, name, &run.run_id)
                    .is_some()
                    && !self.state.read(cx).is_read_only_view();
                if can_open {
                    let (owner, scope, name, id) = (
                        key.to_owned(),
                        scope.to_owned(),
                        name.to_owned(),
                        run.run_id.clone(),
                    );
                    let open = div().child(
                        Button::new(format!("history-chat-{id}"), label("Open chat", "打开会话"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.state.update(cx, |s, cx| {
                                    s.open_workflow_history_chat(&owner, &scope, &name, &id, cx)
                                })
                            })),
                    );
                    #[cfg(test)]
                    let open = crate::app::test_support::track_children(
                        open,
                        vec![format!("history-chat-{}", run.run_id)],
                    );
                    row = row.child(open);
                }
                body = body.child(row);
            }
        }
        body.into_any_element()
    }
}
