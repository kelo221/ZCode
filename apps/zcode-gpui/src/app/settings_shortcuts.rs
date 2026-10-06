use crate::app::root::RootView;
use crate::shared::preferences::{PreferenceChange, Preferences};
use crate::shared::settings::AppSettings;
use crate::shared::shortcut_runtime::{SUPPORTED, bindings};
use ely_gpui_component::buttons::Button;
use ely_gpui_component::forms::HotkeyInput;
use gpui::{AnyElement, Context, IntoElement, Keystroke, div, prelude::*};

fn binding_stroke(binding: &str) -> Option<Keystroke> {
    let binding = binding
        .replace(
            "CmdOrCtrl",
            if cfg!(target_os = "macos") {
                "cmd"
            } else {
                "ctrl"
            },
        )
        .replace("Ctrl", "ctrl")
        .replace("Cmd", "cmd")
        .replace("Shift", "shift")
        .replace("Alt", "alt")
        .replace('+', "-");
    Keystroke::parse(&binding).ok()
}

fn stroke_binding(stroke: &Keystroke) -> String {
    let mut parts = Vec::new();
    if stroke.modifiers.control {
        parts.push("Ctrl".to_owned());
    }
    if stroke.modifiers.platform {
        parts.push("Cmd".to_owned());
    }
    if stroke.modifiers.alt {
        parts.push("Alt".to_owned());
    }
    if stroke.modifiers.shift {
        parts.push("Shift".to_owned());
    }
    parts.push(stroke.key.clone());
    parts.join("+")
}

impl RootView {
    pub(crate) fn render_shortcut_settings(
        &self,
        settings: &AppSettings,
        saving: bool,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div().flex().flex_col().gap_4();
        for command in SUPPORTED {
            let id = command.as_str().to_owned();
            let reset_id = id.clone();
            let values = bindings(*command, settings.shortcut_bindings.as_ref());
            let value = values.first().and_then(|s| binding_stroke(s));
            let row = div()
                .flex()
                .items_center()
                .gap_4()
                .child(
                    div()
                        .flex_1()
                        .child(crate::shared::shortcut_runtime::label(*command)),
                )
                .child(HotkeyInput::new(format!("shortcut-{id}"), value).on_change(
                    move |stroke, _, cx| {
                        let bindings = stroke.as_ref().map(stroke_binding).into_iter().collect();
                        Preferences::enqueue(
                            PreferenceChange::Shortcut(id.clone(), Some(bindings)),
                            cx,
                        );
                    },
                ))
                .child(
                    Button::new(
                        format!("reset-{reset_id}"),
                        crate::shared::i18n::label("Reset", "重置"),
                    )
                    .disabled(saving)
                    .on_click(move |_, _, cx| {
                        Preferences::enqueue(PreferenceChange::Shortcut(reset_id.clone(), None), cx)
                    }),
                );
            #[cfg(test)]
            let row = crate::app::test_support::track_children(
                row,
                vec![
                    format!("shortcut-label-{command:?}"),
                    format!("shortcut-{}", command.as_str()),
                    format!("reset-{}", command.as_str()),
                ],
            );
            list = list.child(row);
        }
        list.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcut_conversion_roundtrips() {
        let stroke = binding_stroke("Ctrl+Shift+y").unwrap();
        assert_eq!(stroke_binding(&stroke), "Ctrl+Shift+y");
    }
}
