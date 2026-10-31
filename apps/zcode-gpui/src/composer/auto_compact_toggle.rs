//! Composer toggle for background auto-compaction of the selected model
//! (SETTINGS.md "Background auto-compaction"; app/auto_compact.rs).

use crate::app::root::RootView;
use crate::shared::auto_compact_settings::{self, AutoCompactEntry};
use crate::shared::i18n::label;
use ely_gpui_component::buttons::{ButtonVariant, IconButton};
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ControlSize;
use gpui::{AnyElement, Context, IntoElement, ParentElement, div};

impl RootView {
    pub(crate) fn auto_compact_toggle(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let choice = self.state.read(cx).current_model_choice();
        let key = auto_compact_settings::model_key(&choice.provider, &choice.model)?;
        let entry = auto_compact_settings::entry(&key, cx);
        let on = entry.enabled;
        let minutes = entry.minutes;
        let state = if on {
            label("on", "开")
        } else {
            label("off", "关")
        };
        let tooltip = format!(
            "{} {minutes} {}: {state}",
            label("Auto-compact after", "空闲"),
            label("min idle", "分钟后自动压缩"),
        );
        let button = IconButton::new("composer-auto-compact", IconName::Timer)
            .size(ControlSize::Sm)
            .variant(if on {
                ButtonVariant::Subtle
            } else {
                ButtonVariant::Ghost
            })
            .tooltip(tooltip)
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                auto_compact_settings::save(
                    key.clone(),
                    AutoCompactEntry {
                        enabled: !on,
                        minutes,
                    },
                    cx,
                );
            });
        let element = div().child(button);
        #[cfg(test)]
        let element =
            crate::app::test_support::track_children(element, vec!["composer-auto-compact".into()]);
        Some(element.into_any_element())
    }
}

#[cfg(test)]
#[path = "auto_compact_toggle_tests.rs"]
mod tests;
