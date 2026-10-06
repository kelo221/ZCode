use crate::{
    app::root::RootView,
    shared::{
        i18n::label,
        theme::{DANGER, MUTED, ui_size},
        theme_colors::color,
    },
};
use ely_gpui_component::{buttons::Button, theme::ControlSize};
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px,
};

impl RootView {
    pub(crate) fn workflow_markdown_control(
        &self,
        id: &str,
        version: u64,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.state.read(cx);
        let origin = state.workflow_focus()?.clone();
        let query = state.active_workflow_artifacts()?;
        if query.query.loading
            || query.query.error.is_some()
            || !query.query.value.as_ref()?.iter().any(|r| {
                r.id == id
                    && r.version == version
                    && r.kind == "markdown"
                    && r.content_type.as_deref() == Some("text/markdown")
            })
        {
            return None;
        }
        let selected = query
            .content
            .as_ref()
            .is_some_and(|c| c.id == id && c.version == version);
        let id = id.to_owned();
        let target = format!("view-markdown-{id}");
        let button = Button::new(
            SharedString::from(target.clone()),
            label("View Markdown", "查看 Markdown"),
        )
        .size(ControlSize::Sm)
        .disabled(selected)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.state.update(cx, |s, cx| {
                s.view_workflow_markdown(&origin, &id, version, cx)
            });
        }));
        let wrapper = div().child(button);
        #[cfg(test)]
        let wrapper = crate::app::test_support::track_children(wrapper, vec![target]);
        Some(wrapper.into_any_element())
    }
    pub(crate) fn workflow_markdown_preview(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.state.read(cx);
        let origin = state.workflow_focus()?.clone();
        let content = state.active_workflow_artifacts()?.content.as_ref()?;
        let id = content.id.clone();
        let version = content.version;
        let retry_origin = origin.clone();
        let retry_id = id.clone();
        let selection = content.selection.clone();
        let retry_selection = selection.clone();
        let retry = Button::new("retry-markdown", label("Retry", "重试"))
            .size(ControlSize::Sm)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.state.update(cx, |s, cx| {
                    if s.markdown_selection_current(&retry_origin, &retry_selection) {
                        s.retry_workflow_markdown(&retry_origin, &retry_id, version, cx);
                    }
                });
            }));
        let close = Button::new("close-markdown", label("Close", "关闭"))
            .size(ControlSize::Sm)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.state.update(cx, |s, cx| {
                    if s.markdown_selection_current(&origin, &selection) {
                        s.close_workflow_markdown(&origin, &id, version, cx);
                    }
                });
            }));
        let buttons = div()
            .flex()
            .gap_1()
            .when(content.query.error.is_some(), |el| el.child(retry))
            .child(close);
        #[cfg(test)]
        let buttons = crate::app::test_support::track_children(
            buttons,
            if content.query.error.is_some() {
                vec!["retry-markdown".into(), "close-markdown".into()]
            } else {
                vec!["close-markdown".into()]
            },
        );
        let controls = div()
            .flex()
            .items_center()
            .justify_between()
            .child(format!("{} · v{}", content.id, version))
            .child(buttons);
        let body = if content.query.loading {
            div()
                .child(label("Loading Markdown…", "正在读取 Markdown…"))
                .into_any_element()
        } else if content.query.error.is_some() {
            div()
                .text_color(color(DANGER))
                .child(label(
                    "Markdown could not be read; Retry or Close",
                    "无法读取 Markdown；请重试或关闭",
                ))
                .into_any_element()
        } else if let Some(text) = &content.query.value {
            if text.is_empty() {
                div()
                    .child(label("This artifact is empty", "此产物为空"))
                    .into_any_element()
            } else {
                crate::shared::markdown::render_passive_markdown(text, version)
            }
        } else {
            div().into_any_element()
        };
        Some(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .text_size(px(ui_size(12.)))
                .text_color(color(MUTED))
                .child(controls)
                .child(
                    div()
                        .id("workflow-markdown-scroll")
                        .max_h(px(360.))
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(body),
                )
                .child(label(
                    "Links and images are inert in this preview",
                    "此预览中的链接和图片不执行外部操作",
                ))
                .into_any_element(),
        )
    }
}
