//! Card and row renderers for the plugin store.
//!
//! Separated from plugin_pane.rs to maintain the strict <= 400 lines limit;
//! marketplace and restorable-builtin rows live in plugin_sources.rs.

use crate::app::root::RootView;
use crate::shared::plugins::{AvailablePluginSummary, InstalledPluginSummary, available_card_id};
use crate::shared::theme::ui_size;
use crate::shared::theme::{ACCENT, BORDER, CARD, MUTED, SUCCESS, TEXT, WARNING};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement, IntoElement, ParentElement, Styled, div,
    prelude::*, px,
};

impl RootView {
    pub(crate) fn render_available_card(
        &self,
        p: &AvailablePluginSummary,
        is_featured: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = p.name.clone();
        let market = p.marketplace.clone();
        let installed = p.installed;
        let example_prompts = p
            .listing
            .as_ref()
            .and_then(|l| l.example_prompts.clone())
            .unwrap_or_default();
        let prompt_owner = self.state.read(cx).active_ws_key();
        let prompt_plugin = p.id.clone();
        let prompt_pending = prompt_owner
            .as_ref()
            .is_some_and(|key| self.state.read(cx).plugin_prompt_pending(key));
        let requires_paid = p
            .listing
            .as_ref()
            .and_then(|l| l.requires_paid_plan)
            .unwrap_or(false);

        div()
            .id(ElementId::Name(
                available_card_id(&p.id, &market, is_featured).into(),
            ))
            .flex()
            .flex_col()
            .gap_1p5()
            .p_2p5()
            .rounded_md()
            .bg(rgb(CARD))
            .border_1()
            .border_color(if is_featured {
                rgb(ACCENT)
            } else {
                rgb(BORDER)
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_size(px(ui_size(12.)))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .child(p.display_label().to_string()),
                            )
                            .when(requires_paid, |el| {
                                el.child(
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(rgb(0x382c14))
                                        .text_size(px(ui_size(9.5)))
                                        .text_color(rgb(WARNING))
                                        .child("Paid Plan"),
                                )
                            }),
                    )
                    .child(if installed {
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .bg(rgb(0x133827))
                            .text_size(px(ui_size(10.)))
                            .text_color(rgb(SUCCESS))
                            .child("Installed")
                            .into_any_element()
                    } else {
                        self.plugin_mutation_button(
                            format!("install/{}/{}/{}", is_featured, market, p.id),
                            crate::shared::i18n::label("Install", "安装").into(),
                            crate::app::plugin_controls::PluginMutation::Install(
                                name.clone(),
                                market.clone(),
                            ),
                            cx,
                        )
                    }),
            )
            .child(
                div()
                    .text_size(px(ui_size(10.5)))
                    .text_color(rgb(MUTED))
                    .child(p.description.clone().unwrap_or_default()),
            )
            .child(self.plugin_detail_button(p, cx))
            .when(!example_prompts.is_empty(), |el| {
                el.child(
                    div().flex().flex_wrap().gap_1().mt_1().children(
                        example_prompts
                            .into_iter()
                            .take(2)
                            .enumerate()
                            .map(|(i, prompt)| {
                                let pr = prompt.clone();
                                let owner = prompt_owner.clone();
                                let plugin = prompt_plugin.clone();
                                let control = ely_gpui_component::buttons::Button::new(
                                    format!("plugin-prompt-{}-{i}", p.id),
                                    prompt.clone(),
                                )
                                .variant(ely_gpui_component::buttons::ButtonVariant::Secondary)
                                .size(ely_gpui_component::theme::ControlSize::Sm)
                                .disabled(
                                    prompt_pending
                                        || prompt_owner.as_ref().is_none_or(|key| {
                                            !self.state.read(cx).plugin_operation_available(key)
                                        }),
                                )
                                .loading(prompt_pending)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(owner) = &owner {
                                        this.state.update(cx, |s, cx| {
                                            s.use_plugin_prompt(owner, &plugin, &pr, cx)
                                        });
                                    }
                                }));
                                let control = div().child(control);
                                #[cfg(test)]
                                let control = crate::app::test_support::track_children(
                                    control,
                                    vec![format!("plugin-prompt-{}-{i}", p.id)],
                                );
                                control
                            }),
                    ),
                )
            })
            .into_any_element()
    }

    pub(crate) fn render_installed_row(
        &self,
        p: &InstalledPluginSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p_id = p.id.clone();
        let p_market = p.marketplace.clone();
        let enabled = p.enabled;
        let has_update = p.update_status.as_deref() == Some("update-available");
        let latest_ver = p.latest_version.clone();

        div()
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
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
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_size(px(ui_size(12.)))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .child(p.display_label().to_string()),
                            )
                            .child(
                                div()
                                    .text_size(px(ui_size(10.)))
                                    .text_color(rgb(MUTED))
                                    .child(format!("v{}", p.version.as_deref().unwrap_or("0.1.0"))),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(ui_size(10.)))
                            .text_color(rgb(MUTED))
                            .child(format!("source: {}", p.marketplace)),
                    ),
            )
            .child(self.plugin_config_button(&p.id, cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .when(has_update, |el| {
                        el.child(self.plugin_mutation_button(
                            format!("update-{}", p_id),
                            format!(
                                "{} ({})",
                                crate::shared::i18n::label("Update", "更新"),
                                latest_ver.as_deref().unwrap_or("new")
                            ),
                            crate::app::plugin_controls::PluginMutation::Update(
                                p_id.clone(),
                                p_market.clone(),
                            ),
                            cx,
                        ))
                    })
                    .child(
                        self.plugin_mutation_button(
                            format!("toggle-{}", p_id),
                            if enabled {
                                crate::shared::i18n::label("Enabled", "已启用")
                            } else {
                                crate::shared::i18n::label("Disabled", "已禁用")
                            }
                            .into(),
                            crate::app::plugin_controls::PluginMutation::Enable(
                                p_id.clone(),
                                !enabled,
                            ),
                            cx,
                        ),
                    )
                    .child(self.plugin_mutation_button(
                        format!("uninstall-{}", p_id),
                        crate::shared::i18n::label("Uninstall", "卸载").into(),
                        crate::app::plugin_controls::PluginMutation::Uninstall(
                            p_id.clone(),
                            p_market.clone(),
                        ),
                        cx,
                    )),
            )
            .into_any_element()
    }
}
