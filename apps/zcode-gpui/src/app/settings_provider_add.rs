//! Model settings: "Add provider" pane (desktop `AddProviderDialog`) — pick a
//! preset template or name a custom OpenAI/Anthropic-compatible provider.

use crate::app::root::RootView;
use crate::app::settings_provider_form::Field;
use crate::provider_settings::ProviderView;
use crate::shared::i18n::label;
use crate::shared::theme::{active_theme, font_size_sm, ui_size};
use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::forms::Input;
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ControlSize;
use gpui::{AnyElement, Context, IntoElement, SharedString, div, prelude::*, px};
use serde_json::json;

impl RootView {
    pub(crate) fn render_provider_add(
        &mut self,
        view: &ProviderView,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = active_theme();
        let heading = |text: &str| {
            div()
                .text_size(px(ui_size(14.)))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(text.to_owned())
        };
        let mut page = div().flex().flex_col().gap_4().child(
            div()
                .text_size(px(ui_size(16.)))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme.foreground)
                .child(label("Add provider", "添加提供商")),
        );

        if !view.templates.is_empty() {
            let mut grid = div().flex().flex_wrap().gap_2();
            let mut targets = Vec::new();
            for template in &view.templates {
                let id = template.id.clone();
                let target = format!("provider-template-{}", template.id);
                grid = grid.child(
                    Button::new(
                        SharedString::from(target.clone()),
                        label(&template.name_en, &template.name_zh).to_owned(),
                    )
                    .variant(ButtonVariant::Outline)
                    .size(ControlSize::Md)
                    .disabled(busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let operation = json!({ "type": "createProvider", "templateId": id });
                        this.mutate_provider(operation, cx);
                    })),
                );
                targets.push(target);
            }
            #[cfg(test)]
            let grid = crate::app::test_support::track_children(grid, targets);
            #[cfg(not(test))]
            let _ = targets;
            page = page
                .child(heading(label("From a preset", "使用预置模板")))
                .child(grid);
        }

        let Some(form) = self.settings.provider_form.as_ref() else {
            return page.into_any_element();
        };
        let custom = div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().w(px(280.)).child(Input::new(&form.new_provider)))
            .child(
                Button::new("provider-create", label("Create", "创建"))
                    .icon(IconName::Plus)
                    .size(ControlSize::Md)
                    .variant(ButtonVariant::Primary)
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.commit_provider_field(Field::NewProvider, cx)
                    })),
            )
            .child(
                Button::new("provider-add-cancel", label("Cancel", "取消"))
                    .size(ControlSize::Md)
                    .variant(ButtonVariant::Ghost)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.update(cx, |s, cx| {
                            s.provider_settings.adding = false;
                            cx.notify();
                        })
                    })),
            );
        #[cfg(test)]
        let custom = crate::app::test_support::track_children(
            custom,
            vec![
                "provider-new-name".into(),
                "provider-create".into(),
                "provider-add-cancel".into(),
            ],
        );
        page.child(heading(label("Custom provider", "自定义提供商")))
            .child(
                div()
                    .text_size(px(font_size_sm()))
                    .text_color(theme.subtle)
                    .child(label(
                        "Any OpenAI- or Anthropic-compatible endpoint. Set its URL, format and key after creating it.",
                        "任意兼容 OpenAI 或 Anthropic 的接口。创建后再设置地址、格式和密钥。",
                    )),
            )
            .child(custom)
            .into_any_element()
    }
}
