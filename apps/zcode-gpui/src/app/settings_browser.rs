//! Settings › Browser Use (BROWSER_BRIDGE.md): bridge status, the browsers
//! connected through the ZCode extension, host registration and the steps to
//! load the unpacked extension.

use crate::app::root::RootView;
use crate::app::settings_rows::{ControlWidth, settings_card, settings_row};
use crate::browser_bridge::registration::{Registered, extension_dir};
use crate::shared::i18n::label;
use crate::shared::theme::active_theme;
use ely_gpui_component::buttons::Button;
use gpui::{AnyElement, ClipboardItem, Context, IntoElement, SharedString, div, prelude::*};

impl RootView {
    pub(crate) fn browser_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = active_theme();
        let bridge = &self.state.read(cx).browser;
        let listening = bridge.listener.as_ref().map(|l| l.addr.to_string());
        let bridge_status: SharedString = match (&listening, &bridge.error) {
            (Some(addr), _) => addr.clone().into(),
            (None, Some(error)) => error.clone().into(),
            (None, None) if crate::shared::isolation::active().is_some() => {
                label("Off in scratch instances", "临时实例中已关闭").into()
            }
            (None, None) => label("Not running", "未运行").into(),
        };
        let browsers: Vec<(String, String)> = bridge
            .browsers()
            .into_iter()
            .map(|b| {
                (
                    format!("{} ({})", b.name, b.profile),
                    format!("{} {}", label("Extension", "扩展"), b.extension_version),
                )
            })
            .collect();
        let registration = bridge.registration.clone();
        let registration_error = bridge.registration_error.clone();
        let extension = extension_dir();

        let (host_status, registered): (SharedString, bool) = match &registration {
            None => (label("Unknown", "未知").into(), false),
            Some(status) if !status.host_exists => (
                format!(
                    "{} {}",
                    label("Host program missing:", "缺少宿主程序："),
                    status.host.display()
                )
                .into(),
                matches!(status.registered, Registered::Ours),
            ),
            Some(status) => match &status.registered {
                Registered::Ours => (label("Registered", "已注册").into(), true),
                Registered::No => (label("Not registered", "未注册").into(), false),
                Registered::Other(path) => (
                    format!(
                        "{} {path}",
                        label("Registered for another copy:", "已为另一个副本注册：")
                    )
                    .into(),
                    true,
                ),
                Registered::Unsupported => (
                    label(
                        "Registration is only implemented on Windows",
                        "目前仅支持在 Windows 上注册",
                    )
                    .into(),
                    false,
                ),
            },
        };
        let scratch = crate::shared::isolation::active().is_some();
        let can_register = !scratch
            && registration
                .as_ref()
                .is_some_and(|s| s.host_exists && s.registered != Registered::Unsupported);
        let register = Button::new(
            "settings-browser-register",
            if registered {
                label("Register again", "重新注册")
            } else {
                label("Register", "注册")
            },
        )
        .disabled(!can_register)
        .on_click(cx.listener(|this, _, _, cx| {
            this.state.update(cx, |s, cx| s.register_browser_host(cx));
        }));
        let unregister = Button::new("settings-browser-unregister", label("Remove", "移除"))
            .disabled(scratch || !registered)
            .on_click(cx.listener(|this, _, _, cx| {
                this.state.update(cx, |s, cx| s.unregister_browser_host(cx));
            }));
        let open_path = extension.clone();
        let open = Button::new(
            "settings-browser-open-extension",
            label("Open folder", "打开文件夹"),
        )
        .disabled(extension.is_none())
        .on_click(move |_, _, _| {
            if let Some(path) = &open_path {
                let _ = crate::shared::os::file_launcher::open_with_system_default(path);
            }
        });
        let copy_path = extension.clone();
        let copy = Button::new(
            "settings-browser-copy-extension",
            label("Copy path", "复制路径"),
        )
        .disabled(extension.is_none())
        .on_click(move |_, _, cx| {
            if let Some(path) = &copy_path {
                cx.write_to_clipboard(ClipboardItem::new_string(path.display().to_string()));
            }
        });

        let mut status_rows = vec![settings_row(
            label("Bridge", "桥接"),
            Some(
                label(
                    "The agent's browser tool drives tabs in your own browser through the ZCode extension.",
                    "Agent 的浏览器工具通过 ZCode 扩展控制你自己浏览器中的标签页。",
                )
                .into(),
            ),
            div().text_color(theme.subtle).child(bridge_status),
            ControlWidth::Wide,
        )];
        if browsers.is_empty() {
            status_rows.push(settings_row(
                label("Connected browsers", "已连接的浏览器"),
                Some(
                    label(
                        "None. Open Helium or Chrome with the ZCode extension loaded.",
                        "无。请打开已加载 ZCode 扩展的 Helium 或 Chrome。",
                    )
                    .into(),
                ),
                div(),
                ControlWidth::Default,
            ));
        }
        for (name, version) in browsers {
            status_rows.push(settings_row(
                name,
                Some(version.into()),
                div()
                    .text_color(theme.subtle)
                    .child(label("Connected", "已连接")),
                ControlWidth::Default,
            ));
        }

        let mut column = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(settings_card(status_rows))
            .child(settings_card([
                settings_row(
                    label("Native host", "本地宿主"),
                    Some(host_status),
                    div().flex().gap_2().child(register).child(unregister),
                    ControlWidth::Wide,
                ),
                settings_row(
                    label("Extension", "扩展"),
                    Some(
                        extension
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| {
                                label("Not found next to ZCode", "未在 ZCode 旁找到").into()
                            })
                            .into(),
                    ),
                    div().flex().gap_2().child(open).child(copy),
                    ControlWidth::Wide,
                ),
            ]));
        if let Some(error) = registration_error {
            column = column.child(div().text_color(theme.destructive).child(error));
        }
        column
            .child(div().text_color(theme.subtle).child(label(
                "Setup: 1. In Helium or Chrome open chrome://extensions and turn on Developer mode. 2. Click Load unpacked and choose the extension folder above. 3. Click Register. The extension connects within a minute, or right away after you reload it.",
                "设置步骤：1. 在 Helium 或 Chrome 中打开 chrome://extensions 并开启开发者模式。2. 点击“加载已解压的扩展程序”，选择上面的扩展文件夹。3. 点击“注册”。扩展会在一分钟内连接，重新加载扩展后会立即连接。",
            )))
            .into_any_element()
    }
}
