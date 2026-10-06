use crate::app::root::RootView;
use crate::shared::{
    i18n::label,
    theme::{CARD, DANGER, MUTED, ui_size},
    theme_colors::color as rgb,
};
use ely_gpui_component::buttons::Button;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px,
};

impl RootView {
    pub(crate) fn workflow_definition_section(
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
            .and_then(|ws| ws.workflow_definitions.get(&format!("{scope}\0{name}")));
        let definition = query.and_then(|q| q.value.clone());
        let error = query.and_then(|q| q.error.clone());
        let loading = query.is_some_and(|q| q.loading);
        let attempted = query.is_some_and(|q| q.attempted);
        let (owner, scope, name) = (key.to_owned(), scope.to_owned(), name.to_owned());
        let button = div().child(
            Button::new(
                "saved-definition",
                if attempted {
                    label("Refresh definition", "刷新定义")
                } else {
                    label("Show definition", "显示定义")
                },
            )
            .disabled(loading)
            .loading(loading)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.inspect_workflow_definition(&owner, &scope, &name, true, cx)
                })
            })),
        );
        #[cfg(test)]
        let button =
            crate::app::test_support::track_children(button, vec!["saved-definition".into()]);
        let mut body = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(button)
            .children(error.map(|e| div().text_color(rgb(DANGER)).child(e)));
        if let Some(definition) = definition {
            body = body
                .child(
                    div()
                        .text_color(rgb(MUTED))
                        .child(definition.meta.description),
                )
                .children(definition.meta.when_to_use)
                .children(definition.meta.args.into_iter().map(|(name, arg)| {
                    div().text_color(rgb(MUTED)).child(format!(
                        "{name}: {}{}",
                        arg.kind,
                        if arg.required == Some(true) { " *" } else { "" }
                    ))
                }))
                .child(div().text_color(rgb(MUTED)).child(definition.path))
                .child(
                    div()
                        .id("saved-definition-script")
                        .max_h(px(240.))
                        .overflow_y_scroll()
                        .p_2()
                        .bg(rgb(CARD))
                        .font_family("monospace")
                        .text_size(px(12.))
                        .child(definition.script),
                );
        } else if loading {
            body = body.child(
                div()
                    .text_size(px(ui_size(11.)))
                    .child(label("Loading definition…", "加载定义中…")),
            );
        }
        body.into_any_element()
    }
}
