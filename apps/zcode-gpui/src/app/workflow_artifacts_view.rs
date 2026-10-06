use crate::{
    app::root::RootView,
    shared::{
        i18n::label,
        theme::{BORDER, DANGER, MUTED, TEXT, ui_size},
        theme_colors::color,
    },
};
use ely_gpui_component::{buttons::Button, theme::ControlSize};
use gpui::{
    AnyElement, Context, IntoElement, ParentElement, Styled, div, prelude::FluentBuilder, px,
};

impl RootView {
    pub(crate) fn workflow_artifacts_section(
        &self,
        run: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.state.read(cx);
        let origin = state.workflow_focus()?.clone();
        if origin.run != run {
            return None;
        }
        let query = state.active_workflow_artifacts().map(|q| &q.query);
        let loading = query.is_some_and(|q| q.loading);
        let error = query.and_then(|q| q.error.clone());
        let rows = query.and_then(|q| q.value.clone());
        let message = if loading {
            Some(label("Loading artifacts…", "正在读取产物…"))
        } else if error.as_deref() == Some(crate::shared::workflow_artifacts::ARTIFACT_UNAVAILABLE)
        {
            Some(label(
                "Artifact inspection is unavailable on this backend",
                "此后端不支持产物检查",
            ))
        } else if error.is_some() {
            Some(label(
                "Artifacts could not be read; Refresh to retry",
                "无法读取产物；请刷新重试",
            ))
        } else if rows.as_ref().is_some_and(Vec::is_empty) {
            Some(label(
                "No artifacts were returned for this run",
                "此运行未返回产物",
            ))
        } else if rows.is_none() {
            Some(label(
                "Open a connected run to inspect artifacts",
                "连接后打开运行以检查产物",
            ))
        } else {
            None
        };
        let refresh = Button::new("workflow-artifacts-refresh", label("Refresh", "刷新"))
            .size(ControlSize::Sm)
            .disabled(loading)
            .loading(loading)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.state
                    .update(cx, |s, cx| s.refresh_workflow_artifacts(&origin, cx));
            }));
        let refresh = div().child(refresh);
        #[cfg(test)]
        let refresh = crate::app::test_support::track_children(
            refresh,
            vec!["workflow-artifacts-refresh".into()],
        );
        let mut sorted = rows.into_iter().flatten().collect::<Vec<_>>();
        sorted.sort_by_key(|row| !row.primary);
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .pt_2()
                .border_t_1()
                .border_color(color(BORDER))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(ui_size(14.)))
                                .text_color(color(TEXT))
                                .child(label("Artifacts", "产物")),
                        )
                        .child(refresh),
                )
                .when_some(message, |el, message| {
                    el.child(
                        div()
                            .text_size(px(ui_size(12.)))
                            .text_color(color(if error.is_some() { DANGER } else { MUTED }))
                            .child(message),
                    )
                })
                .children(sorted.into_iter().map(|row| {
                    div()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .py_1()
                        .child(
                            div()
                                .text_size(px(ui_size(14.)))
                                .text_color(color(TEXT))
                                .child(row.title.clone().unwrap_or_else(|| row.id.clone())),
                        )
                        .child(
                            div()
                                .text_size(px(ui_size(12.)))
                                .text_color(color(MUTED))
                                .child(format!(
                                    "{} · {} · v{}{}{}",
                                    row.kind,
                                    row.id,
                                    row.version,
                                    if row.primary {
                                        label(" · Deliverable", " · 交付物")
                                    } else {
                                        ""
                                    },
                                    row.content_type
                                        .as_ref()
                                        .map(|t| format!(" · {t}"))
                                        .unwrap_or_default()
                                )),
                        )
                        .children(self.workflow_markdown_control(&row.id, row.version, cx))
                        .when(row.item_count > 0, |el| {
                            el.child(
                                div()
                                    .text_size(px(ui_size(12.)))
                                    .text_color(color(MUTED))
                                    .child(format!(
                                        "{}: {}",
                                        label("Items", "条目"),
                                        row.item_count
                                    )),
                            )
                        })
                }))
                .children(self.workflow_markdown_preview(cx))
                .child(
                    div()
                        .text_size(px(ui_size(12.)))
                        .text_color(color(MUTED))
                        .child(label(
                            "Other artifact formats remain metadata-only",
                            "其他产物格式暂仅显示元数据",
                        )),
                )
                .into_any_element(),
        )
    }
}
