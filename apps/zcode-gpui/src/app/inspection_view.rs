use crate::app::{root::RootView, settings::SettingsSection};
use crate::shared::i18n::label;
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, div};

#[derive(Clone, Copy)]
pub(crate) enum InspectionKind {
    Mcp,
    Plugins,
    Usage,
}

impl RootView {
    pub(crate) fn inspection_feedback(
        &self,
        kind: InspectionKind,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state.read(cx);
        let inspection = state.active_inspection();
        let status = inspection.map(|i| match kind {
            InspectionKind::Mcp => (i.mcp.loading, i.mcp.error.clone()),
            InspectionKind::Plugins => (i.plugins.loading, i.plugins.error.clone()),
            InspectionKind::Usage => i
                .usage
                .get(&i.usage_range)
                .map(|q| (q.loading, q.error.clone()))
                .unwrap_or_default(),
        });
        let connected = state
            .active_ws_key()
            .and_then(|key| state.ws(&key))
            .is_some_and(|ws| ws.started && ws.inbound.is_some());
        let (loading, error) = status.unwrap_or_default();
        let plugin_errors = if matches!(kind, InspectionKind::Plugins) {
            inspection
                .map(|i| {
                    i.plugin_operation_error
                        .iter()
                        .cloned()
                        .chain(i.plugins.value.iter().flat_map(|v| v.errors.clone()))
                        .chain(
                            i.plugins
                                .value
                                .iter()
                                .filter(|v| !v.capability_supported)
                                .map(|_| {
                                    label("Plugin management is unavailable", "插件管理不可用")
                                        .to_owned()
                                }),
                        )
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            vec![]
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .children((!connected).then(|| {
                div().child(label(
                    "Connect a workspace to load this section",
                    "连接工作区后加载此部分",
                ))
            }))
            .children(loading.then(|| div().child(label("Loading…", "加载中…"))))
            .children(error.map(|error| div().child(error)))
            .children(plugin_errors.into_iter().map(|error| div().child(error)))
            .into_any_element()
    }

    pub(crate) fn ensure_inspection(&mut self, kind: InspectionKind, cx: &mut Context<Self>) {
        let needed = self
            .state
            .read(cx)
            .active_inspection()
            .is_some_and(|i| match kind {
                InspectionKind::Mcp => !i.mcp.attempted,
                InspectionKind::Plugins => !i.plugins.attempted,
                InspectionKind::Usage => i.usage.get(&i.usage_range).is_none_or(|q| !q.attempted),
            });
        if needed {
            self.state.update(cx, |state, cx| match kind {
                InspectionKind::Mcp => state.fetch_mcp_servers(cx),
                InspectionKind::Plugins => state.fetch_plugins_overview(cx),
                InspectionKind::Usage => {
                    let range = state
                        .active_inspection()
                        .map(|i| i.usage_range.clone())
                        .unwrap_or_else(|| "7d".into());
                    state.fetch_usage_stats(&range, cx);
                }
            });
        }
    }

    pub(crate) fn select_settings_section(
        &mut self,
        section: SettingsSection,
        cx: &mut Context<Self>,
    ) {
        if self.settings.section != section && self.settings.section == SettingsSection::Subagents {
            self.subagents.form = None;
            self.state.update(cx, |s, cx| s.close_profiles(cx));
        }
        // 原因：关闭 Settings 会停掉 Host，但重新打开时 section 未变，旧判断不视为“进入”，
        // 页面停在 Resume。修复：Settings 尚未打开时进入 Subagents 也算进入。
        let entering_subagents =
            enters_subagents(self.settings.section, section, self.settings.open);
        let entering_plugins = section == SettingsSection::Plugins
            && (self.settings.section != section || !self.settings.open);
        let entering_browser = section == SettingsSection::Browser
            && (self.settings.section != section || !self.settings.open);
        self.settings.section = section;
        if entering_browser {
            self.state
                .update(cx, |s, cx| s.refresh_browser_registration(cx));
        }
        if entering_subagents && self.state.read(cx).profiles.consented {
            self.state.update(cx, |s, cx| s.activate_profiles(cx));
        }
        // 插件商店并入 Settings 后（CONTEXT.md「商店页结构」），进入时落在公开分段；
        // 已在该页时保留用户所选分段。
        if entering_plugins {
            self.plugin_segment = crate::app::plugin_pane::PluginSegment::Public;
        }
        cx.notify();
    }
}

fn enters_subagents(current: SettingsSection, target: SettingsSection, open: bool) -> bool {
    target == SettingsSection::Subagents && (current != target || !open)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopening_settings_on_subagents_counts_as_entering() {
        use SettingsSection::{General, Subagents};
        assert!(enters_subagents(General, Subagents, true));
        assert!(enters_subagents(Subagents, Subagents, false));
        assert!(!enters_subagents(Subagents, Subagents, true));
        assert!(!enters_subagents(Subagents, General, false));
    }
}
