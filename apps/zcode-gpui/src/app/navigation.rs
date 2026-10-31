use crate::app::root::RootView;
use crate::shared::preferences::PreferenceOwner;
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
        if self.settings.open {
            window.focus(&self.settings.focus, cx);
        } else {
            self.return_focus_to_composer(window, cx);
        }
        cx.notify();
    }

    /// Hand keyboard focus back to the chat after an overlay closes. On the
    /// phone build a focused composer raises the soft keyboard, so focus
    /// goes to the neutral handle and only a tap on the composer focuses it.
    pub(crate) fn return_focus_to_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let focus = if state.phone_client.enabled {
            self.settings.focus.clone()
        } else {
            state.composer.read(cx).focus.clone()
        };
        window.focus(&focus, cx);
    }

    /// Phone build: a touch outside the composer block drops its focus so
    /// the soft keyboard hides, as in native chat apps.
    pub(crate) fn release_phone_composer_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if state.phone_client.enabled && state.composer.read(cx).focus.is_focused(window) {
            window.focus(&self.settings.focus, cx);
        }
    }

    pub(crate) fn restore_settings_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        // 录制器在提交或取消后主动 blur；仅补回空焦点，不抢占其他 Settings 控件。
        if self.settings.open && window.focused(cx).is_none() {
            window.focus(&self.settings.focus, cx);
        }
    }

    pub(crate) fn handle_navigation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // Android's back key arrives as an unmapped key named "back".
        if event.keystroke.key == "back" {
            return self.handle_back(window, cx);
        }
        if event.keystroke.key == "escape" && self.close_image_viewer(cx) {
            return true;
        }
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
                self.show_chat_page();
                self.state
                    .update(cx, |state, cx| state.new_conversation_chat(cx));
            }
            Some(ShortcutCommandId::OpenWorkspace) => {
                self.close_settings(window, cx);
                self.open_project_dialog(cx);
            }
            Some(ShortcutCommandId::ToggleSidebar) if !self.settings.open => self.toggle_sidebar(),
            Some(ShortcutCommandId::ZoomIn) => self.zoom(Some(1.), cx),
            Some(ShortcutCommandId::ZoomOut) => self.zoom(Some(-1.), cx),
            Some(ShortcutCommandId::ResetZoom) => self.zoom(None, cx),
            Some(ShortcutCommandId::CloseActiveContext) if !self.settings.open => {
                if !self.close_active_context() {
                    return false;
                }
            }
            Some(ShortcutCommandId::PreviousConversation) => {
                self.select_adjacent_conversation(false, window, cx)
            }
            Some(ShortcutCommandId::NextConversation) => {
                self.select_adjacent_conversation(true, window, cx)
            }
            _ => return self.handle_transcript_scroll_key(event, window, cx),
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
                |this, action: &crate::shared::markdown::file_ref::OpenFileReference, _, cx| {
                    this.open_file_reference(&action.raw, cx)
                },
            ))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.handle_navigation_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
    }
}
