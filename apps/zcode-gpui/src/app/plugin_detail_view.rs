use crate::app::root::RootView;
use crate::backend::plugin_detail::PluginDetailIdentity;
use crate::shared::{
    i18n::label,
    plugins::AvailablePluginSummary,
    theme::{DANGER, MUTED, ui_size},
    theme_colors::color as rgb,
};
use ely_gpui_component::buttons::{Button, ButtonVariant};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div, px};

impl RootView {
    pub(crate) fn restore_orphan_management(
        &mut self,
        key: &str,
        identity: &PluginDetailIdentity,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.read(cx);
        if state.active_ws_key().as_deref() == Some(key)
            && state.ws(key).is_some_and(|w| {
                w.inspection.plugin_detail.is_none()
                    && w.inspection.plugins.value.as_ref().is_some_and(|o| {
                        o.installed_plugins
                            .iter()
                            .any(|p| p.id == identity.id && p.marketplace == identity.marketplace)
                    })
            })
        {
            self.plugin_segment = crate::app::plugin_pane::PluginSegment::ManageInstalled;
        }
    }
    pub(crate) fn plugin_detail_button(
        &self,
        p: &AvailablePluginSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = self.state.read(cx).active_ws_key();
        let identity = PluginDetailIdentity {
            id: p.id.clone(),
            name: p.name.clone(),
            marketplace: p.marketplace.clone(),
        };
        let id = format!("plugin-details-{}", p.id);
        let button = div().child(
            Button::new(id.clone(), label("Details", "详情"))
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(key) = &key {
                        this.state
                            .update(cx, |s, cx| s.open_plugin_detail(key, identity.clone(), cx));
                    }
                })),
        );
        #[cfg(test)]
        let button = crate::app::test_support::track_children(button, vec![id]);
        button.into_any_element()
    }
    pub(crate) fn selected_plugin_detail(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.state.read(cx);
        let key = state.active_ws_key()?;
        let ws = state.ws(&key)?;
        let identity = ws.inspection.plugin_detail.clone()?;
        let plugin = ws.inspection.plugins.value.as_ref().and_then(|o| {
            o.available_plugins.iter().find(|p| {
                p.id == identity.id
                    && p.name == identity.name
                    && p.marketplace == identity.marketplace
            })
        })?;
        let query = ws.inspection.plugin_descriptions.get(&identity.cache_key());
        let loading = query.is_some_and(|q| q.loading);
        let mut body = div()
            .flex()
            .flex_col()
            .gap_2()
            .text_size(px(ui_size(12.)))
            .child(plugin.display_label().to_owned())
            .children(plugin.description.clone())
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .child(format!("{} / {}", identity.marketplace, identity.name)),
            );
        let owner = key.clone();
        let receipt = identity.clone();
        let buttons = div().flex().flex_wrap().gap_1().child(
            Button::new("plugin-detail-close", label("Back to store", "返回商店"))
                .variant(ButtonVariant::Secondary)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state
                        .update(cx, |s, cx| s.close_plugin_detail(&owner, &receipt, cx))
                })),
        );
        let owner = key.clone();
        let receipt = identity.clone();
        let buttons = buttons.child(
            Button::new(
                "plugin-detail-refresh",
                label("Refresh details", "刷新详情"),
            )
            .disabled(loading)
            .loading(loading)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.state.update(cx, |s, cx| {
                    s.fetch_plugin_description(&owner, &receipt, true, cx)
                })
            })),
        );
        #[cfg(test)]
        let buttons = crate::app::test_support::track_children(
            buttons,
            vec!["plugin-detail-close".into(), "plugin-detail-refresh".into()],
        );
        body = body.child(buttons);
        if let Some(error) = query.and_then(|q| q.error.clone()) {
            body = body.child(div().text_color(rgb(DANGER)).child(error));
        }
        if let Some(description) = query.and_then(|q| q.value.as_ref()) {
            body = body.children(
                description
                    .feedback()
                    .into_iter()
                    .map(|e| div().text_color(rgb(DANGER)).child(e)),
            );
            if let Some(meta) = &description.metadata {
                for (title, value) in [
                    ("Author", &meta.author),
                    ("Author URL", &meta.author_url),
                    ("Homepage", &meta.homepage),
                    ("Version", &meta.version),
                ] {
                    if let Some(value) = value {
                        body = body.child(
                            div()
                                .text_color(rgb(MUTED))
                                .child(format!("{title}: {value}")),
                        );
                    }
                }
            }
            if description.components.is_empty() {
                body = body.child(label("No components returned", "未返回组件"));
            }
            for group in &description.components {
                body = body.child(group.kind.clone());
                for item in &group.items {
                    body = body
                        .child(item.name.clone())
                        .children(item.description.clone());
                }
            }
        } else if !loading && query.is_none() {
            body = body.child(label(
                "Refresh to load component details",
                "刷新以加载组件详情",
            ));
        } else if loading {
            body = body.child(label("Loading details…", "加载详情中…"));
        }
        Some(body.into_any_element())
    }
}
