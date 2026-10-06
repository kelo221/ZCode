use crate::{app::root::RootView, shared::i18n::label};
use ely_gpui_component::buttons::Button;
use gpui::{AnyElement, Context, IntoElement, ParentElement, SharedString, div};

impl RootView {
    pub(crate) fn workflow_navigation_control(
        &self,
        source: &str,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.state.read(cx);
        let key = state.active_ws_key()?;
        let sid = state.active.as_deref()?;
        let origin = state.workflow_link_origin(&key, sid)?;
        let (target, tool) = state.workflow_successor(&key, sid, source)?;
        let source = source.to_owned();
        let id = format!("workflow-successor-{source}");
        let button = Button::new(
            SharedString::from(id.clone()),
            label("Open successor", "打开后继运行"),
        )
        .size(ely_gpui_component::theme::ControlSize::Sm)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.state.update(cx, |state, cx| {
                if state.open_workflow_successor(&origin, &source, &target, &tool) {
                    cx.notify();
                }
            });
        }));
        let wrapper = div().child(button);
        #[cfg(test)]
        let wrapper = crate::app::test_support::track_children(wrapper, vec![id]);
        Some(wrapper.into_any_element())
    }
    pub(crate) fn workflow_all_runs_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let origin = self.state.read(cx).workflow_focus()?.clone();
        let button = Button::new("workflow-all-runs", label("All runs", "全部运行")).on_click(
            cx.listener(move |this, _, _, cx| {
                this.state.update(cx, |state, cx| {
                    if state.workflow_link_current(&origin) {
                        state.all_workflow_runs();
                        cx.notify();
                    }
                });
            }),
        );
        let wrapper = div().child(button);
        #[cfg(test)]
        let wrapper =
            crate::app::test_support::track_children(wrapper, vec!["workflow-all-runs".into()]);
        Some(wrapper.into_any_element())
    }
    pub(crate) fn workflow_open_run_control(
        &self,
        run: &str,
        tool: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.state.read(cx);
        if state.workflow_focus().is_some() {
            return None;
        }
        let key = state.active_ws_key()?;
        let sid = state.active.clone()?;
        let origin = state.workflow_link_origin(&key, &sid)?;
        let run = run.to_owned();
        let tool = tool.filter(|tool| !tool.trim().is_empty())?.to_owned();
        let id = format!("workflow-open-{run}");
        let button = Button::new(
            SharedString::from(id.clone()),
            label("Open run", "打开运行"),
        )
        .size(ely_gpui_component::theme::ControlSize::Sm)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.state.update(cx, |state, cx| {
                if state.workflow_link_current(&origin)
                    && state.focus_workflow(&key, &sid, &run, &tool)
                {
                    cx.notify();
                }
            });
        }));
        let wrapper = div().child(button);
        #[cfg(test)]
        let wrapper = crate::app::test_support::track_children(wrapper, vec![id]);
        Some(wrapper.into_any_element())
    }
}
