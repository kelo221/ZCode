use crate::app::root::RootView;
use crate::shared::preferences::{PreferenceChange, PreferenceOwner, Preferences};
use crate::shared::shortcuts::ShortcutCommandId;
use gpui::{Context, Div, KeyDownEvent, Window};

impl RootView {
    pub(crate) fn open_command_center(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.quickpick_open = true;
        self.quickpick_query.clear();
        self.quickpick_selected = 0;
        // 所有入口统一转移焦点，否则侧栏搜索输入会落入 composer 而不是命令中心。
        window.focus(&self.settings.focus, cx);
        cx.notify();
    }

    pub(crate) fn close_command_center(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.quickpick_open = false;
        let focus = if self.settings.open {
            self.settings.focus.clone()
        } else {
            self.state.read(cx).composer.read(cx).focus.clone()
        };
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(crate) fn restore_settings_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        // 录制器在提交或取消后主动 blur；仅补回空焦点，不抢占其他 Settings 控件。
        if self.settings.open && window.focused(cx).is_none() {
            window.focus(&self.settings.focus, cx);
        }
    }

    pub(crate) fn switch_theme(&mut self, cx: &mut Context<Self>) {
        let next = if crate::shared::theme::active_theme()
            == crate::shared::theme::ThemePalette::zai_light()
        {
            crate::shared::theme::ThemeMode::ZaiDark
        } else {
            crate::shared::theme::ThemeMode::ZaiLight
        };
        Preferences::enqueue(PreferenceChange::Theme(next), cx);
    }

    pub(crate) fn handle_navigation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.quickpick_open {
            return crate::app::quickpick::handle_quickpick_key(self, event, window, cx);
        }
        if event.keystroke.key == "escape" && self.settings.open {
            self.close_settings(window, cx);
            return true;
        }
        let owner = cx.global::<PreferenceOwner>().0.clone();
        let command = crate::shared::shortcut_runtime::match_event(
            event,
            owner.read(cx).snapshot.shortcut_bindings.as_ref(),
        );
        match command {
            Some(ShortcutCommandId::OpenSettings) => self.open_settings(window, cx),
            Some(ShortcutCommandId::OpenCommandCenter) => self.open_command_center(window, cx),
            Some(ShortcutCommandId::SwitchTheme) => self.switch_theme(cx),
            Some(ShortcutCommandId::ToggleTerminal) if !self.settings.open => {
                self.term_open = !self.term_open;
                if self.term_open {
                    self.ensure_term(cx);
                    window.focus(&self.term.focus, cx);
                }
            }
            Some(ShortcutCommandId::ToggleSidePane) if !self.settings.open => {
                self.dock_open = !self.dock_open;
                if self.dock_open {
                    self.on_dock_tab(self.dock_tab, cx);
                }
            }
            Some(ShortcutCommandId::NewTask) => {
                self.close_settings(window, cx);
                self.state
                    .update(cx, |state, cx| state.new_conversation_chat(cx));
            }
            Some(ShortcutCommandId::OpenWorkspace) => {
                self.close_settings(window, cx);
                self.open_project_dialog(cx);
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    pub(crate) fn navigation_handlers(&self, element: Div, cx: &mut Context<Self>) -> Div {
        use gpui::InteractiveElement;
        element
            .on_action(
                cx.listener(|this, _: &crate::app::settings::OpenSettings, window, cx| {
                    this.open_settings(window, cx)
                }),
            )
            .on_action(cx.listener(
                |this, _: &crate::app::quickpick::SwitchThemeAction, _, cx| this.switch_theme(cx),
            ))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.handle_navigation_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
    }
}
