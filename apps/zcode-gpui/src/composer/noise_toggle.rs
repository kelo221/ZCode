//! Composer toggle for the noise-free transcript (transcript/noise_free.rs).

use crate::app::root::RootView;
use crate::shared::i18n::label;
use crate::shared::preferences::{PreferenceChange, Preferences};
use ely_gpui_component::buttons::{ButtonVariant, IconButton};
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ControlSize;
use gpui::{AnyElement, Context, IntoElement, ParentElement, div};

impl RootView {
    pub(crate) fn noise_toggle(&self, cx: &mut Context<Self>) -> AnyElement {
        let on = crate::transcript::noise_free::enabled(cx);
        let button = IconButton::new("composer-noise-free", IconName::Filter)
            .size(ControlSize::Sm)
            .variant(if on {
                ButtonVariant::Subtle
            } else {
                ButtonVariant::Ghost
            })
            .tooltip(if on {
                label(
                    "Noise-free: tool calls are folded. Click to show everything",
                    "简洁模式：工具调用已折叠。点击显示全部",
                )
            } else {
                label(
                    "Showing everything. Click to fold tool calls",
                    "正在显示全部。点击折叠工具调用",
                )
            })
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                Preferences::enqueue(PreferenceChange::NoiseFree(!on), cx);
            });
        let element = div().child(button);
        #[cfg(test)]
        let element =
            crate::app::test_support::track_children(element, vec!["composer-noise-free".into()]);
        element.into_any_element()
    }
}
