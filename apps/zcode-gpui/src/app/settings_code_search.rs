//! Settings › Code search (PARITY.md "Built-in fff code search"): the agent's
//! own FindFiles/SearchCode tools backed by an in-memory fff index. Desktop
//! only; the phone build hides the section.

use crate::app::settings_rows::{ControlWidth, settings_card, settings_row};
use crate::shared::i18n::label;
use crate::shared::preferences::{PreferenceChange, Preferences};
use crate::shared::settings::AppSettings;
use ely_gpui_component::forms::Switch;
use gpui::{AnyElement, IntoElement, div, prelude::*};

pub(crate) fn code_search_pane(settings: &AppSettings, blocked: bool) -> AnyElement {
    let switch = div().child(
        Switch::new("settings-code-search-enabled", settings.code_search())
            .disabled(blocked)
            .on_change(|enabled, _, cx| {
                Preferences::enqueue(PreferenceChange::CodeSearch(enabled), cx)
            }),
    );
    #[cfg(test)]
    let switch = crate::app::test_support::track_children(
        switch,
        vec!["settings-code-search-enabled".into()],
    );
    settings_card([settings_row(
        label("Fast code search", "快速代码搜索"),
        Some(label(
            "Gives agents FindFiles and SearchCode, backed by an fff index of the project kept in memory. The index opens on first use and closes one minute after the project has no active agents. Applies to new and resumed sessions; when off or unavailable, agents search with Bash.",
            "为 Agent 提供 FindFiles 和 SearchCode，由常驻内存的 fff 项目索引支撑。首次使用时打开，项目没有活跃 Agent 一分钟后关闭。对新建和恢复的会话生效；关闭或不可用时 Agent 使用 Bash 搜索。",
        ).into()),
        switch,
        ControlWidth::Default,
    )])
    .into_any_element()
}
