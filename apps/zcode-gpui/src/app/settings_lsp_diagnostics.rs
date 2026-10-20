//! Settings › Diagnostics (PARITY.md "Diagnostics after edits"): after each
//! Edit/Write the agent sees the language server's errors for that file, and
//! gets a Diagnostics tool for a full check. Desktop only; the phone build
//! hides the section.

use crate::app::settings_rows::{ControlWidth, settings_card, settings_row};
use crate::shared::i18n::label;
use crate::shared::preferences::{PreferenceChange, Preferences};
use crate::shared::settings::AppSettings;
use ely_gpui_component::forms::Switch;
use gpui::{AnyElement, IntoElement, div, prelude::*};

pub(crate) fn lsp_diagnostics_pane(settings: &AppSettings, blocked: bool) -> AnyElement {
    let switch = div().child(
        Switch::new(
            "settings-lsp-diagnostics-enabled",
            settings.lsp_diagnostics(),
        )
        .disabled(blocked)
        .on_change(|enabled, _, cx| {
            Preferences::enqueue(PreferenceChange::LspDiagnostics(enabled), cx)
        }),
    );
    #[cfg(test)]
    let switch = crate::app::test_support::track_children(
        switch,
        vec!["settings-lsp-diagnostics-enabled".into()],
    );
    settings_card([settings_row(
        label("Diagnostics after edits", "编辑后诊断"),
        Some(label(
            "After each edit, agents see the language server's errors and warnings for the file, and can run a full check with the Diagnostics tool. Uses rust-analyzer, csharp-ls and typescript-language-server when they are on PATH; servers close after 10 minutes without an active agent. Applies to new and resumed sessions.",
            "每次编辑后，Agent 会看到语言服务器对该文件报告的错误和警告，并可用 Diagnostics 工具运行完整检查。使用 PATH 上的 rust-analyzer、csharp-ls 和 typescript-language-server；没有活跃 Agent 10 分钟后关闭服务器。对新建和恢复的会话生效。",
        ).into()),
        switch,
        ControlWidth::Default,
    )])
    .into_any_element()
}
