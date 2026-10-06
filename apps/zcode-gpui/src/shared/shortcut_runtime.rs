use crate::shared::shortcuts::{
    SHORTCUT_DEFINITIONS, ShortcutCommandId as Command, matches_binding,
};
use gpui::KeyDownEvent;
use std::collections::{HashMap, HashSet};

pub(crate) const SUPPORTED: &[Command] = &[
    Command::OpenSettings,
    Command::OpenCommandCenter,
    Command::SwitchTheme,
    Command::ToggleTerminal,
    Command::ToggleSidePane,
    Command::NewTask,
    Command::OpenWorkspace,
];

pub(crate) fn label(command: Command) -> &'static str {
    let (english, chinese) = match command {
        Command::OpenSettings => ("Settings", "设置"),
        Command::OpenCommandCenter => ("Command center", "命令中心"),
        Command::SwitchTheme => ("Switch theme", "切换主题"),
        Command::ToggleTerminal => ("Toggle terminal", "切换终端"),
        Command::ToggleSidePane => ("Toggle tool dock", "切换工具面板"),
        Command::NewTask => ("New task", "新任务"),
        Command::OpenWorkspace => ("Open workspace", "打开工作区"),
        _ => (command.as_str(), command.as_str()),
    };
    crate::shared::i18n::label(english, chinese)
}

pub(crate) fn bindings(
    command: Command,
    overrides: Option<&HashMap<String, Vec<String>>>,
) -> Vec<String> {
    overrides
        .and_then(|map| map.get(command.as_str()))
        .cloned()
        .unwrap_or_else(|| {
            SHORTCUT_DEFINITIONS
                .iter()
                .find(|d| d.id == command)
                .unwrap()
                .default_bindings
                .iter()
                .map(|s| (*s).to_owned())
                .collect()
        })
}

pub(crate) fn match_event(
    event: &KeyDownEvent,
    overrides: Option<&HashMap<String, Vec<String>>>,
) -> Option<Command> {
    SUPPORTED.iter().copied().find(|command| {
        bindings(*command, overrides).iter().any(|binding| {
            matches_binding(binding, &event.keystroke.key, event.keystroke.modifiers)
        })
    })
}

pub(crate) fn validate(overrides: Option<&HashMap<String, Vec<String>>>) -> Result<(), String> {
    let mut seen = HashSet::new();
    for command in SUPPORTED {
        for binding in bindings(*command, overrides) {
            let parts: Vec<_> = binding.split('+').collect();
            let key = parts.last().copied().unwrap_or("");
            let modifiers = &parts[..parts.len().saturating_sub(1)];
            if key.is_empty()
                || modifiers.is_empty()
                || !modifiers
                    .iter()
                    .all(|m| matches!(*m, "CmdOrCtrl" | "Ctrl" | "Cmd" | "Shift" | "Alt"))
            {
                return Err(format!("Invalid shortcut: {binding}"));
            }
            if !modifiers
                .iter()
                .any(|m| matches!(*m, "CmdOrCtrl" | "Ctrl" | "Cmd"))
            {
                return Err("Global shortcuts require Ctrl or Command".into());
            }
            let mut canonical: Vec<_> = modifiers
                .iter()
                .map(|m| {
                    if *m == "CmdOrCtrl" {
                        if cfg!(target_os = "macos") {
                            "Cmd"
                        } else {
                            "Ctrl"
                        }
                    } else {
                        m
                    }
                })
                .collect();
            canonical.sort();
            canonical.dedup();
            if canonical.len() != modifiers.len() {
                return Err(format!("Duplicate shortcut modifier: {binding}"));
            }
            let identity = format!("{}+{}", canonical.join("+"), key.to_lowercase());
            if !seen.insert(identity) {
                return Err(format!("Shortcut is already assigned: {binding}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overrides_disable_reset_and_conflicts() {
        assert!(validate(None).is_ok());
        let mut map = HashMap::new();
        map.insert("openSettings".into(), vec![]);
        assert!(bindings(Command::OpenSettings, Some(&map)).is_empty());
        map.insert("openSettings".into(), vec!["CmdOrCtrl+k".into()]);
        assert!(validate(Some(&map)).is_err());
        map.insert("openSettings".into(), vec!["banana+k".into()]);
        assert!(validate(Some(&map)).is_err());
        map.remove("openSettings");
        assert_eq!(bindings(Command::OpenSettings, Some(&map)), ["CmdOrCtrl+,"]);
    }
}
