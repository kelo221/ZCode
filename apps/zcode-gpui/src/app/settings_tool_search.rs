//! Settings › MCP Servers "Load MCP tools on demand" (PARITY.md "Load MCP tools
//! on demand"): the shared `toolSearchMode` that defers MCP tool schemas.

use crate::app::settings_rows::{ControlWidth, settings_card, settings_row};
use crate::shared::i18n::label;
use crate::shared::preferences::{PreferenceChange, Preferences};
use crate::shared::settings::AppSettings;
use ely_gpui_component::forms::{Choice, Select};
use gpui::{AnyElement, IntoElement, div, prelude::*};

pub(crate) fn tool_search_card(settings: &AppSettings, blocked: bool) -> AnyElement {
    let select = div().w_full().child(
        Select::new(
            "settings-tool-search-mode",
            [
                Choice::new("auto", label("Auto", "自动")),
                Choice::new("local", label("Local", "本地")),
                Choice::new("off", label("Off", "关闭")),
            ],
        )
        .selected(settings.tool_search_mode().to_owned())
        .disabled(blocked)
        .on_change(|value, _, cx| {
            Preferences::enqueue(PreferenceChange::ToolSearchMode(value.to_string()), cx)
        }),
    );
    #[cfg(test)]
    let select =
        crate::app::test_support::track_children(select, vec!["settings-tool-search-mode".into()]);
    settings_card([settings_row(
        label("Load MCP tools on demand", "按需加载 MCP 工具"),
        Some(label(
            "Agents see only MCP tool names and load a tool's full definition with ToolSearch when they need it, which keeps the context window small. Auto uses the provider's native tool search where supported, otherwise Local. Applies to new and resumed sessions. Experimental: native search on a provider that doesn't fully support it can miss the prompt cache and use more tokens.",
            "Agent 只看到 MCP 工具名称，需要时通过 ToolSearch 加载完整定义，以节省上下文窗口。自动模式在供应商支持时使用原生工具搜索，否则使用本地模式。对新建和恢复的会话生效。实验性：供应商未完整支持原生搜索时可能导致缓存失效并增加 Token 用量。",
        ).into()),
        select,
        ControlWidth::Default,
    )])
    .into_any_element()
}
