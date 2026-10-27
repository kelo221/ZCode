//! Right-click menu of the message box: Undo, Redo, Cut, Copy, Paste and
//! Select All, greyed out when they would do nothing.

use crate::composer::input::Composer;
use ely_gpui_component::menus::{Menu, MenuItem};
use gpui::{App, Context, Window};

fn keys(key: &str) -> String {
    let primary = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    format!("{primary}-{key}")
}

impl Composer {
    pub(crate) fn edit_menu(&self, cx: &mut Context<Self>) -> Menu {
        let me = cx.weak_entity();
        let item = |label: &str,
                    key: &str,
                    enabled: bool,
                    run: fn(&mut Composer, &mut Context<Composer>)| {
            let me = me.clone();
            MenuItem::new(label.to_string())
                .keys(keys(key))
                .disabled(!enabled)
                .on_click(move |window: &mut Window, cx: &mut App| {
                    let _ = me.update(cx, |c, cx| {
                        window.focus(&c.focus, cx);
                        run(c, cx);
                    });
                })
        };
        let selected = self.selection().is_some();
        let label = crate::shared::i18n::label;
        Menu::new()
            .item(item(
                label("Undo", "撤销"),
                "z",
                self.history.can_undo(),
                |c, cx| {
                    c.undo_redo(true, cx);
                },
            ))
            .item(item(
                label("Redo", "重做"),
                if cfg!(target_os = "macos") {
                    "shift-z"
                } else {
                    "y"
                },
                self.history.can_redo(),
                |c, cx| {
                    c.undo_redo(false, cx);
                },
            ))
            .separator()
            .item(item(label("Cut", "剪切"), "x", selected, |c, cx| {
                c.copy_selection(true, cx);
            }))
            .item(item(label("Copy", "复制"), "c", selected, |c, cx| {
                c.copy_selection(false, cx);
            }))
            .item(item(label("Paste", "粘贴"), "v", true, |c, cx| {
                c.paste_clipboard(cx)
            }))
            .separator()
            .item(item(
                label("Select All", "全选"),
                "a",
                !self.content.is_empty(),
                |c, cx| c.select_all(cx),
            ))
    }
}
