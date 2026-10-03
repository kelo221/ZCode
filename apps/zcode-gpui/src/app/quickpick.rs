//! Command Center QuickPick overlay (Ctrl+K / Ctrl+Shift+P)
//! Mirrors packages/ui/src/quickpick/quickPickCommands.ts and cross-project session navigation.

#![allow(dead_code)]

pub use crate::app::quickpick_items::*;
use crate::app::root::RootView;
use crate::shared::theme::*;
use gpui::{
    AnyElement, Context, ElementId, InteractiveElement, IntoElement, MouseButton, ParentElement,
    Styled, Window, div, px, rgb, rgba,
};

gpui::actions!(quickpick, [ToggleQuickPick, SwitchThemeAction]);

pub fn render_quickpick_modal(
    this: &mut RootView,
    _window: &mut Window,
    cx: &mut Context<RootView>,
) -> AnyElement {
    let app = this.state.read(cx);
    let items = get_quickpick_items(&app.workspaces, &this.quickpick_query);
    let count = items.len();
    let selected = this.quickpick_selected.min(count.saturating_sub(1));

    let mut rows = div()
        .flex()
        .flex_col()
        .gap_0p5()
        .max_h(px(320.))
        .overflow_y_hidden();

    for (idx, item) in items.into_iter().enumerate() {
        let is_selected = idx == selected;
        let action = item.action.clone();
        let bg_color = if is_selected {
            rgb(SELECTED)
        } else {
            rgb(CARD)
        };

        rows = rows.child(
            div()
                .id(ElementId::NamedInteger("qp-row".into(), idx as u64))
                .px_3()
                .py_2()
                .rounded_md()
                .cursor_pointer()
                .bg(bg_color)
                .hover(|s| s.bg(rgb(CARD_HOVER)))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _window, cx| {
                        execute_quickpick_action(this, &action, cx);
                    }),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(10.))
                                .px_1()
                                .rounded_sm()
                                .bg(rgb(HOVER))
                                .text_color(rgb(MUTED))
                                .child(item.section),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(rgb(TEXT))
                                .child(item.title),
                        ),
                )
                .children(item.shortcut.map(|sc| {
                    div()
                        .text_size(px(11.))
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(BORDER))
                        .text_color(rgb(MUTED))
                        .child(sc)
                })),
        );
    }

    let query_empty = this.quickpick_query.is_empty();
    let query_color = if query_empty { rgb(MUTED) } else { rgb(TEXT) };
    let query_display = if query_empty {
        "Type a command or search tasks…".to_string()
    } else {
        this.quickpick_query.clone()
    };

    div()
        .id("quickpick-overlay")
        .absolute()
        .inset_0()
        .bg(rgba(0x00000088))
        .flex()
        .flex_col()
        .items_center()
        .pt(px(60.))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.quickpick_open = false;
                cx.notify();
            }),
        )
        .child(
            div()
                .id("quickpick-card")
                .w(px(580.))
                .bg(rgb(CARD))
                .border_1()
                .border_color(rgb(BORDER))
                .rounded_lg()
                .shadow_lg()
                .p_2()
                .flex()
                .flex_col()
                .gap_2()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _, _, cx| {
                        cx.stop_propagation();
                    }),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .px_2()
                        .py_1p5()
                        .rounded_md()
                        .bg(rgb(PANEL))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .gap_2()
                        .child(icon(I_BULB, 13., MUTED))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(13.))
                                .text_color(query_color)
                                .child(query_display),
                        )
                        .child(
                            div()
                                .text_size(px(10.))
                                .px_1()
                                .rounded_sm()
                                .border_1()
                                .border_color(rgb(BORDER))
                                .text_color(rgb(MUTED))
                                .child("Esc"),
                        ),
                )
                .child(rows),
        )
        .into_any_element()
}

pub fn execute_quickpick_action(
    this: &mut RootView,
    action: &QuickPickItemAction,
    cx: &mut Context<RootView>,
) {
    this.quickpick_open = false;
    match action {
        QuickPickItemAction::Command(QuickPickAction::NewTask) => {
            this.state.update(cx, |s, cx| s.new_chat(cx));
        }
        QuickPickItemAction::Command(QuickPickAction::OpenWorkspace) => {
            this.state.update(cx, |state, cx| {
                state.push_log("open workspace requested".into());
                cx.notify();
            });
        }
        QuickPickItemAction::Command(QuickPickAction::ToggleSidebar) => {
            this.dock_open = !this.dock_open;
            cx.notify();
        }
        QuickPickItemAction::Command(QuickPickAction::ToggleTerminal) => {
            this.term_open = !this.term_open;
            if this.term_open {
                this.ensure_term(cx);
            }
            cx.notify();
        }
        QuickPickItemAction::Command(QuickPickAction::SwitchTheme) => {
            let next_mode = match theme_mode() {
                ThemeMode::ZaiLight => ThemeMode::ZaiDark,
                _ => ThemeMode::ZaiLight,
            };
            set_theme_mode(next_mode);
            this.state.update(cx, |state, cx| {
                state.push_log(format!("Switched theme to {}", next_mode.as_str()));
                cx.notify();
            });
            cx.notify();
        }
        QuickPickItemAction::Command(QuickPickAction::OpenSettings) => {
            this.state.update(cx, |state, cx| {
                state.push_log("Settings: configured via ~/.zcode/v2/setting.json".into());
                cx.notify();
            });
        }
        QuickPickItemAction::Command(QuickPickAction::OpenInEditor) => {
            if let Some(path) = this.active_workspace_path(cx) {
                let _ = crate::shared::os::file_launcher::open_in_editor(&path);
            }
        }
        QuickPickItemAction::Command(QuickPickAction::RevealInFileManager) => {
            if let Some(path) = this.active_workspace_path(cx) {
                let _ = crate::shared::os::file_launcher::reveal_in_file_manager(&path);
            }
        }
        QuickPickItemAction::Command(QuickPickAction::ExportLogs) => {
            let memory_logs: Vec<String> = this.state.read(cx).log.iter().cloned().collect();
            if let Ok(path) = crate::shared::os::log_export::export_logs(&memory_logs) {
                let _ = crate::shared::os::file_launcher::reveal_in_file_manager(&path);
                this.state.update(cx, |state, cx| {
                    state.push_log(format!("Logs exported to {}", path.display()));
                    cx.notify();
                });
            }
        }
        QuickPickItemAction::OpenSession { sid, ws_key } => {
            this.state.update(cx, |state, cx| {
                state.select_session(ws_key, sid, cx);
            });
        }
    }
    cx.notify();
}

pub fn handle_quickpick_key(
    this: &mut RootView,
    ev: &gpui::KeyDownEvent,
    cx: &mut Context<RootView>,
) -> bool {
    let app = this.state.read(cx);
    let items = get_quickpick_items(&app.workspaces, &this.quickpick_query);
    let count = items.len();

    match ev.keystroke.key.as_str() {
        "escape" => {
            this.quickpick_open = false;
            cx.notify();
            true
        }
        "up" => {
            if count > 0 {
                this.quickpick_selected = if this.quickpick_selected == 0 {
                    count - 1
                } else {
                    this.quickpick_selected - 1
                };
                cx.notify();
            }
            true
        }
        "down" => {
            if count > 0 {
                this.quickpick_selected = (this.quickpick_selected + 1) % count;
                cx.notify();
            }
            true
        }
        "enter" => {
            if count > 0 && this.quickpick_selected < count {
                let action = items[this.quickpick_selected].action.clone();
                execute_quickpick_action(this, &action, cx);
            }
            true
        }
        "backspace" => {
            this.quickpick_query.pop();
            this.quickpick_selected = 0;
            cx.notify();
            true
        }
        _ => {
            if !ev.keystroke.modifiers.control
                && !ev.keystroke.modifiers.alt
                && !ev.keystroke.modifiers.platform
            {
                let text = ev.keystroke.key.as_str();
                if text.chars().count() == 1 {
                    this.quickpick_query.push_str(text);
                    this.quickpick_selected = 0;
                    cx.notify();
                    return true;
                }
            }
            false
        }
    }
}
