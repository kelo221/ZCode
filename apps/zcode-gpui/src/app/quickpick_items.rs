//! QuickPick items model and cross-project session discovery.

#![allow(dead_code)]

use crate::backend::workspace::WorkspaceHandle;
use crate::shared::i18n::t;
use crate::shared::theme::{ThemeMode, theme_mode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickPickAction {
    NewTask,
    OpenWorkspace,
    ToggleSidebar,
    ToggleTerminal,
    SwitchTheme,
    OpenSettings,
    OpenInEditor,
    RevealInFileManager,
    ExportLogs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickPickItem {
    pub id: String,
    pub title: String,
    pub section: String,
    pub shortcut: Option<String>,
    pub action: QuickPickItemAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuickPickItemAction {
    Command(QuickPickAction),
    OpenSession { sid: String, ws_key: String },
}

fn cmd_item(
    id: &str,
    title: &str,
    sec: &str,
    sc: Option<&str>,
    act: QuickPickAction,
) -> QuickPickItem {
    QuickPickItem {
        id: id.into(),
        title: title.into(),
        section: sec.into(),
        shortcut: sc.map(Into::into),
        action: QuickPickItemAction::Command(act),
    }
}

pub fn get_quickpick_items(workspaces: &[WorkspaceHandle], query: &str) -> Vec<QuickPickItem> {
    let switch_theme_title = match theme_mode() {
        ThemeMode::ZaiLight => t("quickPick.command.switchThemeToDark"),
        _ => t("quickPick.command.switchThemeToLight"),
    };

    let mut items = vec![
        cmd_item(
            "new-task",
            t("quickPick.command.newTask"),
            "Suggested",
            Some("Ctrl+N"),
            QuickPickAction::NewTask,
        ),
        cmd_item(
            "open-workspace",
            t("quickPick.command.openWorkspace"),
            "Suggested",
            Some("Ctrl+O"),
            QuickPickAction::OpenWorkspace,
        ),
        cmd_item(
            "toggle-sidebar",
            t("quickPick.command.toggleSidebar"),
            "Panels",
            Some("Ctrl+B"),
            QuickPickAction::ToggleSidebar,
        ),
        cmd_item(
            "toggle-terminal",
            t("quickPick.command.toggleTerminal"),
            "Panels",
            Some("Ctrl+J"),
            QuickPickAction::ToggleTerminal,
        ),
        cmd_item(
            "switch-theme",
            switch_theme_title,
            "Configure",
            Some("Ctrl+Shift+L"),
            QuickPickAction::SwitchTheme,
        ),
        cmd_item(
            "settings",
            t("quickPick.command.settings"),
            "Configure",
            Some("Ctrl+,"),
            QuickPickAction::OpenSettings,
        ),
        cmd_item(
            "open-editor",
            "Open Workspace in VS Code",
            "Workspace",
            None,
            QuickPickAction::OpenInEditor,
        ),
        cmd_item(
            "reveal-folder",
            "Reveal Workspace in File Manager",
            "Workspace",
            None,
            QuickPickAction::RevealInFileManager,
        ),
        cmd_item(
            "export-logs",
            "Export Diagnostic Logs (.zip)",
            "Developer",
            None,
            QuickPickAction::ExportLogs,
        ),
    ];

    for w in workspaces {
        for s in &w.sessions {
            items.push(QuickPickItem {
                id: format!("session:{}", s.session_id),
                title: s.title.clone(),
                section: "Tasks".into(),
                shortcut: None,
                action: QuickPickItemAction::OpenSession {
                    sid: s.session_id.clone(),
                    ws_key: w.key.clone(),
                },
            });
        }
    }

    if query.is_empty() {
        items
    } else {
        let q = query.to_lowercase();
        items
            .into_iter()
            .filter(|item| {
                item.title.to_lowercase().contains(&q)
                    || item.section.to_lowercase().contains(&q)
                    || item.id.to_lowercase().contains(&q)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quickpick_items_filter() {
        let items = get_quickpick_items(&[], "");
        assert!(!items.is_empty());

        let new_task_found = items.iter().any(|i| i.id == "new-task");
        assert!(new_task_found);

        let filtered = get_quickpick_items(&[], "theme");
        assert!(filtered.iter().any(|i| i.id == "switch-theme"));
    }
}
