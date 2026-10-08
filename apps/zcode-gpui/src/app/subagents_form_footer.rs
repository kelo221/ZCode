use crate::app::root::RootView;
use crate::shared::i18n::label;
use ely_gpui_component::buttons::Button;
use gpui::{AnyElement, Context, IntoElement, div, prelude::*};

impl RootView {
    pub(super) fn profile_form_footer(&self, blocked: bool, cx: &mut Context<Self>) -> AnyElement {
        let form = self.subagents.form.as_ref().unwrap();
        let id = form.id.clone();
        let mut controls = div().flex().gap_2().child(
            Button::new("profile-form-back", label("Cancel", "取消")).on_click(cx.listener(
                |this, _, window, cx| {
                    this.subagents.form = None;
                    window.focus(&this.settings.focus, cx);
                    cx.notify();
                },
            )),
        );
        controls = controls.child(
            Button::new("profile-form-save", label("Save", "保存"))
                .primary()
                .disabled(blocked)
                .on_click(
                    cx.listener(move |this, _, _, cx| this.save_profile_form(&id, false, cx)),
                ),
        );
        if !form.overriding && form.baseline.is_some() {
            let id = form.id.clone();
            controls = controls.child(
                Button::new(
                    "profile-form-delete",
                    if form.confirm_delete {
                        label("Confirm delete", "确认删除")
                    } else {
                        label("Delete", "删除")
                    },
                )
                .disabled(blocked)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.profile_form_current(&id, cx) {
                        return;
                    }
                    if this
                        .subagents
                        .form
                        .as_ref()
                        .is_some_and(|f| f.confirm_delete)
                    {
                        this.save_profile_form(&id, true, cx);
                    } else if let Some(form) = this.subagents.form.as_mut() {
                        form.confirm_delete = true;
                        cx.notify();
                    }
                })),
            );
        }
        #[cfg(test)]
        let controls = crate::app::test_support::track_children(
            controls,
            if !form.overriding && form.baseline.is_some() {
                vec![
                    "profile-form-back".into(),
                    "profile-form-save".into(),
                    "profile-form-delete".into(),
                ]
            } else {
                vec!["profile-form-back".into(), "profile-form-save".into()]
            },
        );
        controls.into_any_element()
    }
}
