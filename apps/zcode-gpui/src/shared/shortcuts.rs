//! Keyboard shortcuts registry mirroring packages/shared/src/shortcutCommands.ts.
//! Single source of truth for command IDs and default key bindings.

#![allow(dead_code)]

use gpui::Modifiers;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutScope {
    Global,
    Composer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutCommandId {
    OpenCommandCenter,
    OpenSettings,
    FindInTask,
    ToggleSidebar,
    SwitchTheme,
    ToggleTerminal,
    ToggleSidePane,
    PreviousConversation,
    NextConversation,
    NavigateBack,
    NavigateForward,
    OpenModelMenu,
    CycleSessionMode,
    CycleThoughtLevel,
    NewTask,
    OpenWorkspace,
    CloseActiveContext,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    ComposerSend,
    ComposerInsertNewline,
    ToggleInterfaceMode,
    OpenOnboarding,
}

impl ShortcutCommandId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OpenCommandCenter => "openCommandCenter",
            Self::OpenSettings => "openSettings",
            Self::FindInTask => "findInTask",
            Self::ToggleSidebar => "toggleSidebar",
            Self::SwitchTheme => "switchTheme",
            Self::ToggleTerminal => "toggleTerminal",
            Self::ToggleSidePane => "toggleSidePane",
            Self::PreviousConversation => "previousConversation",
            Self::NextConversation => "nextConversation",
            Self::NavigateBack => "navigateBack",
            Self::NavigateForward => "navigateForward",
            Self::OpenModelMenu => "openModelMenu",
            Self::CycleSessionMode => "cycleSessionMode",
            Self::CycleThoughtLevel => "cycleThoughtLevel",
            Self::NewTask => "newTask",
            Self::OpenWorkspace => "openWorkspace",
            Self::CloseActiveContext => "closeActiveContext",
            Self::ZoomIn => "zoomIn",
            Self::ZoomOut => "zoomOut",
            Self::ResetZoom => "resetZoom",
            Self::ComposerSend => "composerSend",
            Self::ComposerInsertNewline => "composerInsertNewline",
            Self::ToggleInterfaceMode => "toggleInterfaceMode",
            Self::OpenOnboarding => "openOnboarding",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "openCommandCenter" => Some(Self::OpenCommandCenter),
            "openSettings" => Some(Self::OpenSettings),
            "findInTask" => Some(Self::FindInTask),
            "toggleSidebar" => Some(Self::ToggleSidebar),
            "switchTheme" => Some(Self::SwitchTheme),
            "toggleTerminal" => Some(Self::ToggleTerminal),
            "toggleSidePane" => Some(Self::ToggleSidePane),
            "previousConversation" => Some(Self::PreviousConversation),
            "nextConversation" => Some(Self::NextConversation),
            "navigateBack" => Some(Self::NavigateBack),
            "navigateForward" => Some(Self::NavigateForward),
            "openModelMenu" => Some(Self::OpenModelMenu),
            "cycleSessionMode" => Some(Self::CycleSessionMode),
            "cycleThoughtLevel" => Some(Self::CycleThoughtLevel),
            "newTask" => Some(Self::NewTask),
            "openWorkspace" => Some(Self::OpenWorkspace),
            "closeActiveContext" => Some(Self::CloseActiveContext),
            "zoomIn" => Some(Self::ZoomIn),
            "zoomOut" => Some(Self::ZoomOut),
            "resetZoom" => Some(Self::ResetZoom),
            "composerSend" => Some(Self::ComposerSend),
            "composerInsertNewline" => Some(Self::ComposerInsertNewline),
            "toggleInterfaceMode" => Some(Self::ToggleInterfaceMode),
            "openOnboarding" => Some(Self::OpenOnboarding),
            _ => None,
        }
    }
}

pub struct ShortcutDefinition {
    pub id: ShortcutCommandId,
    pub scope: ShortcutScope,
    pub default_bindings: &'static [&'static str],
}

pub static SHORTCUT_DEFINITIONS: &[ShortcutDefinition] = &[
    ShortcutDefinition {
        id: ShortcutCommandId::OpenCommandCenter,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+k", "CmdOrCtrl+Shift+p"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::OpenSettings,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+,"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::FindInTask,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+f"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::ToggleSidebar,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+b"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::SwitchTheme,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+Shift+l"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::ToggleTerminal,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+j"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::ToggleSidePane,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+Alt+b"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::PreviousConversation,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+Shift+["],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::NextConversation,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+Shift+]"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::NavigateBack,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+["],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::NavigateForward,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+]"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::OpenModelMenu,
        scope: ShortcutScope::Global,
        default_bindings: &["Ctrl+m"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::CycleSessionMode,
        scope: ShortcutScope::Global,
        default_bindings: &["Ctrl+Shift+m"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::CycleThoughtLevel,
        scope: ShortcutScope::Global,
        default_bindings: &["Ctrl+t"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::NewTask,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+n"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::OpenWorkspace,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+o"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::CloseActiveContext,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+w"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::ZoomIn,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+="],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::ZoomOut,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+-"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::ResetZoom,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+0"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::ComposerSend,
        scope: ShortcutScope::Composer,
        default_bindings: &["Enter"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::ComposerInsertNewline,
        scope: ShortcutScope::Composer,
        default_bindings: &["Shift+Enter"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::ToggleInterfaceMode,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+Shift+u"],
    },
    ShortcutDefinition {
        id: ShortcutCommandId::OpenOnboarding,
        scope: ShortcutScope::Global,
        default_bindings: &["CmdOrCtrl+Shift+o"],
    },
];

/// Check if a keystroke matches a binding string such as "CmdOrCtrl+k".
pub fn matches_binding(binding: &str, key: &str, modifiers: Modifiers) -> bool {
    let parts: Vec<&str> = binding.split('+').collect();
    if parts.is_empty() {
        return false;
    }

    let target_key = parts.last().copied().unwrap_or_default();
    let mod_parts = &parts[..parts.len() - 1];

    let mut need_ctrl_or_cmd = false;
    let mut need_ctrl = false;
    let mut need_shift = false;
    let mut need_alt = false;
    let mut need_cmd = false;

    for &m in mod_parts {
        match m {
            "CmdOrCtrl" => need_ctrl_or_cmd = true,
            "Ctrl" => need_ctrl = true,
            "Cmd" => need_cmd = true,
            "Shift" => need_shift = true,
            "Alt" => need_alt = true,
            _ => {}
        }
    }

    if need_ctrl_or_cmd {
        if cfg!(target_os = "macos") {
            need_cmd = true;
        } else {
            need_ctrl = true;
        }
    }
    if modifiers.control != need_ctrl || modifiers.platform != need_cmd {
        return false;
    }
    if need_shift != modifiers.shift {
        return false;
    }
    if need_alt != modifiers.alt {
        return false;
    }

    key.eq_ignore_ascii_case(target_key)
}

/// Find matching command ID given key and modifiers, checking user overrides first.
pub fn match_shortcut(
    key: &str,
    modifiers: Modifiers,
    scope: ShortcutScope,
    overrides: Option<&HashMap<String, Vec<String>>>,
) -> Option<ShortcutCommandId> {
    for def in SHORTCUT_DEFINITIONS {
        if def.scope != scope {
            continue;
        }

        // Check if user has an override for this command
        if let Some(user_bindings) = overrides.and_then(|ov| ov.get(def.id.as_str())) {
            for b in user_bindings {
                if matches_binding(b, key, modifiers) {
                    return Some(def.id);
                }
            }
        } else {
            for b in def.default_bindings {
                if matches_binding(b, key, modifiers) {
                    return Some(def.id);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_binding() {
        let mut mods = Modifiers {
            control: !cfg!(target_os = "macos"),
            platform: cfg!(target_os = "macos"),
            ..Default::default()
        };
        assert!(matches_binding("CmdOrCtrl+k", "k", mods));
        assert!(matches_binding("CmdOrCtrl+k", "K", mods));
        assert!(!matches_binding("CmdOrCtrl+k", "j", mods));

        mods.shift = true;
        assert!(matches_binding("CmdOrCtrl+Shift+p", "p", mods));
        assert!(!matches_binding("CmdOrCtrl+k", "k", mods));
    }

    #[test]
    fn test_match_shortcut_defaults() {
        let mods = Modifiers {
            control: !cfg!(target_os = "macos"),
            platform: cfg!(target_os = "macos"),
            ..Default::default()
        };
        let cmd = match_shortcut("k", mods, ShortcutScope::Global, None);
        assert_eq!(cmd, Some(ShortcutCommandId::OpenCommandCenter));

        let shift_mods = Modifiers {
            control: !cfg!(target_os = "macos"),
            platform: cfg!(target_os = "macos"),
            shift: true,
            ..Default::default()
        };
        let cmd_theme = match_shortcut("l", shift_mods, ShortcutScope::Global, None);
        assert_eq!(cmd_theme, Some(ShortcutCommandId::SwitchTheme));
    }

    #[test]
    fn test_match_shortcut_override() {
        let mut overrides = HashMap::new();
        // Override openCommandCenter to Ctrl+y instead of Ctrl+k
        overrides.insert(
            "openCommandCenter".to_string(),
            vec!["CmdOrCtrl+y".to_string()],
        );

        let mods = Modifiers {
            control: !cfg!(target_os = "macos"),
            platform: cfg!(target_os = "macos"),
            ..Default::default()
        };
        assert_eq!(
            match_shortcut("k", mods, ShortcutScope::Global, Some(&overrides)),
            None
        );
        assert_eq!(
            match_shortcut("y", mods, ShortcutScope::Global, Some(&overrides)),
            Some(ShortcutCommandId::OpenCommandCenter)
        );
    }
}
