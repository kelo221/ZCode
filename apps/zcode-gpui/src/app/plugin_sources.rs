//! Personal Source drafts/cards and Restorable Builtin rows.

use crate::app::{plugin_controls::PluginMutation, root::RootView};
use crate::shared::plugins::{AvailablePluginSummary, PluginMarketplaceSummary};
use crate::shared::theme::{BORDER, CARD, DANGER, MUTED, TEXT, ui_size};
use crate::shared::theme_colors::color as rgb;
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};

impl RootView {
    pub(crate) fn render_add_source(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let input = self.state.update(cx, |s, cx| s.source_input(cx))?;
        let workspace = self.state.read(cx).active_ws_key()?;
        let draft = &self.state.read(cx).ws(&workspace)?.plugin_source_draft;
        let picking = draft.picking;
        let error = draft.error.clone();
        let picker = div().child(
            ely_gpui_component::buttons::Button::new(
                "pick-plugin-source",
                crate::shared::i18n::label("Choose local directory", "选择本地目录"),
            )
            .disabled(picking)
            .loading(picking)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.pick_plugin_source(&workspace, cx);
            })),
        );
        #[cfg(test)]
        let picker =
            crate::app::test_support::track_children(picker, vec!["pick-plugin-source".into()]);
        let field = div().child(input);
        #[cfg(test)]
        let field =
            crate::app::test_support::track_children(field, vec!["plugin-source-input".into()]);
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field)
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .child(self.plugin_mutation_button(
                            "add-plugin-source".into(),
                            crate::shared::i18n::label("Add source", "添加来源").into(),
                            PluginMutation::Add,
                            cx,
                        ))
                        .child(picker),
                )
                .children(error.map(|error| div().text_color(rgb(DANGER)).child(error)))
                .into_any_element(),
        )
    }

    pub(crate) fn render_marketplace_card(
        &self,
        m: &PluginMarketplaceSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .p_2p5()
            .rounded_md()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(ui_size(12.)))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(m.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(ui_size(10.)))
                            .text_color(rgb(MUTED))
                            .child(format!(
                                "{} plugins • last updated: {}",
                                m.plugin_count,
                                m.last_updated.as_deref().unwrap_or("unknown")
                            )),
                    )
                    .children(m.refresh_failure.clone().map(|error| {
                        div()
                            .text_size(px(ui_size(10.)))
                            .text_color(rgb(DANGER))
                            .child(error)
                    })),
            )
            .child(self.plugin_mutation_button(
                format!("remove-{}", m.id),
                crate::shared::i18n::label("Remove", "移除").into(),
                PluginMutation::Remove(m.id.clone()),
                cx,
            ))
            .into_any_element()
    }

    pub(crate) fn render_restorable_builtin_row(
        &self,
        p: &AvailablePluginSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .p_2()
            .rounded_md()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(ui_size(11.)))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT))
                            .child(p.display_label().to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(ui_size(10.)))
                            .text_color(rgb(MUTED))
                            .child(p.description.clone().unwrap_or_default()),
                    ),
            )
            .child(self.plugin_mutation_button(
                format!("restore-{}", p.id),
                crate::shared::i18n::label("Restore", "恢复").into(),
                PluginMutation::Restore(p.id.clone()),
                cx,
            ))
            .into_any_element()
    }
}
