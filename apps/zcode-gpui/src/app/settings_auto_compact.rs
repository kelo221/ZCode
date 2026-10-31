//! Settings › Models: the per-model background auto-compaction control (a
//! Timer switch and a minutes field) shown in each provider model row. Same
//! `gpuiAutoCompact` entry as the composer toggle (SETTINGS.md).

use crate::app::root::RootView;
use crate::provider_settings::Provider;
use crate::shared::auto_compact_settings::{self, AutoCompactEntry};
use crate::shared::i18n::label;
use crate::shared::theme::{active_theme, font_size_xs};
use ely_gpui_component::forms::{Input, InputEvent, Switch, TextInput};
use ely_gpui_component::primitives::{Icon, IconName, Tooltip};
use ely_gpui_component::theme::ControlSize;
use gpui::{AnyElement, AppContext, Context, Entity, Focusable, Window, div, prelude::*, px};
use std::collections::HashMap;

/// Minutes drafts by `provider/model`, created once per model shown.
#[derive(Default)]
pub(crate) struct MinutesInputs(HashMap<String, Entity<TextInput>>);

/// The entry a minutes draft commits, or `None` when it is empty or unchanged.
pub(crate) fn minutes_update(current: AutoCompactEntry, draft: &str) -> Option<AutoCompactEntry> {
    let minutes: u32 = draft.trim().parse().ok()?;
    let next = AutoCompactEntry { minutes, ..current }.clamped();
    (next != current).then_some(next)
}

impl RootView {
    /// Creates missing inputs for the provider's models and re-seeds the
    /// unfocused ones from the committed setting.
    pub(crate) fn sync_auto_compact_inputs(
        &mut self,
        provider: Option<&Provider>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(provider) = provider else { return };
        let keys: Vec<String> = provider
            .models
            .iter()
            .filter_map(|m| auto_compact_settings::model_key(&provider.id, &m.id))
            .collect();
        for key in keys {
            let existing = self
                .settings
                .provider_form
                .as_ref()
                .and_then(|f| f.auto_compact.0.get(&key).cloned());
            let input = match existing {
                Some(input) => input,
                None => {
                    let input = make_minutes_input(key.clone(), window, cx);
                    let Some(form) = self.settings.provider_form.as_mut() else {
                        return;
                    };
                    form.auto_compact.0.insert(key.clone(), input.clone());
                    input
                }
            };
            if input.read(cx).focus_handle(cx).is_focused(window) {
                continue;
            }
            let committed = auto_compact_settings::entry(&key, cx).minutes.to_string();
            if input.read(cx).text() != committed {
                input.update(cx, |input, cx| input.set_text(committed, cx));
            }
        }
    }

    fn commit_auto_compact_minutes(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(input) = self
            .settings
            .provider_form
            .as_ref()
            .and_then(|f| f.auto_compact.0.get(key).cloned())
        else {
            return;
        };
        let current = auto_compact_settings::entry(key, cx);
        let draft = input.read(cx).text().to_owned();
        match minutes_update(current, &draft) {
            Some(next) => {
                input.update(cx, |input, cx| input.set_text(next.minutes.to_string(), cx));
                auto_compact_settings::save(key.to_owned(), next, cx);
            }
            // Empty or unchanged input shows the committed value again.
            None => input.update(cx, |input, cx| {
                input.set_text(current.minutes.to_string(), cx)
            }),
        }
        cx.notify();
    }

    /// Timer switch plus minutes field for one model row.
    pub(crate) fn auto_compact_model_control(
        &self,
        provider_id: &str,
        model_id: &str,
        slug: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let key = auto_compact_settings::model_key(provider_id, model_id)?;
        let entry = auto_compact_settings::entry(&key, cx);
        let theme = active_theme();
        let input = self
            .settings
            .provider_form
            .as_ref()
            .and_then(|f| f.auto_compact.0.get(&key).cloned());
        let control = div()
            .id(format!("provider-model-auto-compact-{slug}"))
            .flex()
            .items_center()
            .gap_1()
            .tooltip(Tooltip::text(label(
                "Compact this model's conversations after they sit idle this many minutes",
                "该模型的会话空闲达到此分钟数后自动压缩",
            )))
            .child(
                div()
                    .text_color(if entry.enabled {
                        theme.foreground
                    } else {
                        theme.subtlest
                    })
                    .child(Icon::new(IconName::Timer)),
            )
            .child(
                Switch::new(
                    format!("provider-model-auto-compact-on-{slug}"),
                    entry.enabled,
                )
                .on_change(move |enabled, _, cx| {
                    auto_compact_settings::save(
                        key.clone(),
                        AutoCompactEntry { enabled, ..entry },
                        cx,
                    );
                }),
            )
            .children(input.map(|input| {
                div()
                    .w(px(56.))
                    .child(Input::new(&input).size(ControlSize::Sm))
            }))
            .child(
                div()
                    .text_size(px(font_size_xs()))
                    .text_color(theme.subtle)
                    .child(label("min", "分钟")),
            );
        #[cfg(test)]
        let control = crate::app::test_support::track_children(
            div().child(control),
            vec![format!("provider-model-auto-compact-{slug}")],
        );
        Some(control.into_any_element())
    }
}

fn make_minutes_input(
    key: String,
    window: &mut Window,
    cx: &mut Context<RootView>,
) -> Entity<TextInput> {
    let input = cx.new(|cx| {
        TextInput::new(window, cx)
            .filter(|c| c.is_ascii_digit())
            .max_len(4)
    });
    cx.subscribe(
        &input,
        move |this: &mut RootView, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Submit | InputEvent::Blur) {
                this.commit_auto_compact_minutes(&key, cx);
            }
        },
    )
    .detach();
    input
}

#[cfg(test)]
#[path = "settings_auto_compact_tests.rs"]
mod tests;
