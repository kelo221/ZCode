use crate::app::root::RootView;
use crate::shared::{
    i18n::label,
    theme::{CARD, DANGER, ui_size},
    theme_colors::color as rgb,
};
use ely_gpui_component::buttons::Button;
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};

impl RootView {
    pub(crate) fn workflow_settings_control(
        &self,
        run: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (key, sid) = self
            .state
            .read(cx)
            .active_ws_key()
            .zip(self.state.read(cx).active.clone())?;
        if !self
            .state
            .read(cx)
            .workflow_settings_allowed(&key, &sid, run)
        {
            return None;
        }
        let id = run.to_owned();
        let button = div().child(
            Button::new(format!("configure-wf-{run}"), label("Configure", "配置")).on_click(
                cx.listener(move |this, _, _, cx| {
                    this.state
                        .update(cx, |s, cx| s.open_workflow_settings(&key, &sid, &id, cx))
                }),
            ),
        );
        #[cfg(test)]
        let button =
            crate::app::test_support::track_children(button, vec![format!("configure-wf-{run}")]);
        Some(button.into_any_element())
    }
    pub(crate) fn workflow_settings_form(
        &self,
        run: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (key, sid) = self
            .state
            .read(cx)
            .active_ws_key()
            .zip(self.state.read(cx).active.clone())?;
        if !self
            .state
            .read(cx)
            .workflow_settings_allowed(&key, &sid, run)
        {
            return None;
        }
        let ws = self.state.read(cx).ws(&key)?;
        let form = ws.workflow_settings.get(&format!("{sid}\0{run}"))?;
        let (model, concurrency, error) = (
            form.model.clone(),
            form.concurrency.clone(),
            self.state
                .read(cx)
                .workflow_settings_feedback(&key, &sid, run, cx),
        );
        let pending = self
            .state
            .read(cx)
            .workflow_settings_pending(&key, &sid, run);
        let model_field = div().child(model);
        #[cfg(test)]
        let model_field =
            crate::app::test_support::track_children(model_field, vec![format!("wf-model-{run}")]);
        let limit_field = div().child(concurrency);
        #[cfg(test)]
        let limit_field =
            crate::app::test_support::track_children(limit_field, vec![format!("wf-limit-{run}")]);
        let can_apply = self
            .state
            .read(cx)
            .workflow_settings_can_apply(&key, &sid, run, cx);
        let reload_key = key.clone();
        let reload_sid = sid.clone();
        let reload_id = run.to_owned();
        let reload = div().child(
            Button::new(
                format!("reload-wf-{run}"),
                label("Reload current settings", "重新加载当前设置"),
            )
            .disabled(pending || form.needs_snapshot)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.reload_workflow_settings(&reload_key, &reload_sid, &reload_id, cx)
                })
            })),
        );
        #[cfg(test)]
        let reload =
            crate::app::test_support::track_children(reload, vec![format!("reload-wf-{run}")]);
        let id = run.to_owned();
        let apply = div().child(
            Button::new(format!("apply-wf-{run}"), label("Apply", "应用"))
                .disabled(!can_apply)
                .loading(pending)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state
                        .update(cx, |s, cx| s.apply_workflow_settings(&key, &sid, &id, cx))
                })),
        );
        #[cfg(test)]
        let apply =
            crate::app::test_support::track_children(apply, vec![format!("apply-wf-{run}")]);
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .p_2()
                .bg(rgb(CARD))
                .text_size(px(ui_size(11.)))
                .child(label(
                    "Subagent model: provider/model[$level]; blank uses session",
                    "子代理模型：provider/model[$level]；留空使用会话模型",
                ))
                .child(model_field)
                .child(label(
                    "Concurrency: blank removes run limit",
                    "并发数：留空移除当前运行限制",
                ))
                .child(limit_field)
                .child(apply)
                .child(reload)
                .children(error.map(|e| div().text_color(rgb(DANGER)).child(e)))
                .into_any_element(),
        )
    }
}
