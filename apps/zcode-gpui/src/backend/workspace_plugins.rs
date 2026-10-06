use crate::backend::workspace::{Pending, WorkspaceHandle};

impl WorkspaceHandle {
    pub(crate) fn reset_plugin_queries(&mut self) {
        let mut form = self.inspection.plugin_config.take();
        if let Some(form) = &mut form {
            form.needs_reload = true;
            form.confirmation = false;
            form.error = Some("Connection interrupted; Reload scoped configuration before writing; pending outcome is unknown".into());
        }
        // inspection 已清空；旧 MCP/Usage 回执也必须移除，避免重连后重新注入旧查询事实。
        self.pending.retain(|_, p| {
            !matches!(
                p,
                Pending::PluginPrompt(_)
                    | Pending::PluginDescribe(_)
                    | Pending::PluginConfig(_)
                    | Pending::PluginAction(_)
                    | Pending::FetchPluginsOverview
                    | Pending::FetchMcpList
                    | Pending::FetchUsageStats(_)
            )
        });
        self.inspection = Default::default();
        self.inspection.plugin_config = form;
    }
}
