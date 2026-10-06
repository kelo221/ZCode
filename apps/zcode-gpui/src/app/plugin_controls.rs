use crate::app::root::RootView;
use ely_gpui_component::buttons::{Button, ButtonVariant};
use gpui::{AnyElement, Context, IntoElement, ParentElement, div};

pub(crate) enum PluginMutation {
    Install(String, String),
    Update(String, String),
    Uninstall(String, String),
    Enable(String, bool),
    Restore(String),
    Remove(String),
    Refresh,
    Add,
}

impl RootView {
    pub(crate) fn plugin_mutation_button(
        &self,
        id: String,
        label: String,
        action: PluginMutation,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace = self.state.read(cx).active_ws_key();
        let available = workspace
            .as_ref()
            .is_some_and(|key| self.state.read(cx).plugin_operation_available(key));
        let pending = workspace
            .as_ref()
            .is_some_and(|key| self.state.read(cx).plugin_action_pending(key));
        let button = div().child(
            Button::new(id.clone(), label)
                .variant(ButtonVariant::Secondary)
                .disabled(!available)
                .loading(pending)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let Some(key) = &workspace else { return };
                    this.state.update(cx, |s, cx| match &action {
                        PluginMutation::Install(name, market) => {
                            s.install_plugin_for(key, name, market, cx)
                        }
                        PluginMutation::Update(id, market) => {
                            s.update_plugin_for(key, id, market, cx)
                        }
                        PluginMutation::Uninstall(id, market) => {
                            s.uninstall_plugin_for(key, id, market, cx)
                        }
                        PluginMutation::Enable(id, enabled) => {
                            s.set_plugin_enabled_for(key, id, *enabled, cx)
                        }
                        PluginMutation::Restore(id) => s.restore_builtin_plugin_for(key, id, cx),
                        PluginMutation::Remove(market) => {
                            s.remove_plugin_marketplace_for(key, market, cx)
                        }
                        PluginMutation::Refresh => s.refresh_plugin_sources_for(key, cx),
                        PluginMutation::Add => s.add_plugin_source_for(key, cx),
                    });
                })),
        );
        #[cfg(test)]
        let button = crate::app::test_support::track_children(button, vec![id]);
        button.into_any_element()
    }
}
