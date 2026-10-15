//! Settings › Phone (REMOTE_CONTROL.md, LAN slice). On the desktop: the
//! gateway switch, pairing link with QR code, paired phone and Revoke. On
//! the phone build: the paired desktop, pairing from the clipboard, Forget.

use crate::app::root::RootView;
use crate::app::settings_rows::{ControlWidth, settings_card, settings_row};
use crate::shared::i18n::label;
use crate::shared::theme::active_theme;
use ely_gpui_component::buttons::Button;
use ely_gpui_component::forms::Switch;
use gpui::{AnyElement, ClipboardItem, Context, IntoElement, div, prelude::*, px};

/// Size of one QR module in logical pixels.
const QR_MODULE: f32 = 4.;
const QR_QUIET_ZONE: usize = 4;

fn qr_code(text: &str) -> Option<AnyElement> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let width = code.width();
    let colors = code.to_colors();
    let column = (0..width).fold(div().flex().flex_col(), |column, y| {
        column.child((0..width).fold(div().flex(), |row, x| {
            let dark = colors[y * width + x] == qrcode::Color::Dark;
            row.child(div().size(px(QR_MODULE)).bg(if dark {
                gpui::black()
            } else {
                gpui::white()
            }))
        }))
    });
    Some(
        div()
            .flex_none()
            .bg(gpui::white())
            .p(px(QR_MODULE * QR_QUIET_ZONE as f32))
            .child(column)
            .into_any_element(),
    )
}

impl RootView {
    pub(crate) fn phone_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        if self.state.read(cx).phone_client.enabled {
            self.phone_client_pane(cx)
        } else {
            self.phone_gateway_pane(cx)
        }
    }

    fn phone_gateway_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = active_theme();
        let state = self.state.read(cx);
        let gateway = &state.phone;
        let enabled = gateway.enabled();
        let address = gateway.address();
        let device = gateway.paired_device();
        let connected = gateway.connected_sockets();
        let link = gateway.link.clone();
        let error = gateway.error.clone();
        let entity = self.state.clone();
        let switch =
            Switch::new("settings-phone-enabled", enabled).on_change(move |enabled, _, cx| {
                entity.update(cx, |s, cx| s.set_phone_gateway_enabled(enabled, cx));
            });
        let status = match (enabled, address) {
            (_, Some(addr)) => addr.to_string(),
            (true, None) => label("Not listening", "未在监听").into(),
            (false, None) => label("Off", "已关闭").into(),
        };
        let revoke = Button::new("settings-phone-revoke", label("Revoke", "撤销"))
            .disabled(device.is_none())
            .on_click(cx.listener(|this, _, _, cx| {
                this.state.update(cx, |s, cx| s.revoke_phone(cx));
            }));
        let mut column = div().flex().flex_col().gap_4().child(settings_card([
            settings_row(
                label("Allow phone remote control", "允许手机远程控制"),
                Some(label(
                    "A paired phone on the same network can open your workspaces, read and send messages, and answer prompts. Off by default.",
                    "同一网络中已配对的手机可以打开工作区、查看和发送消息并回答提示。默认关闭。",
                ).into()),
                div().child(switch),
                ControlWidth::Default,
            ),
            settings_row(
                label("Listening on", "监听地址"),
                None,
                div().text_color(theme.subtle).child(status),
                ControlWidth::Wide,
            ),
            settings_row(
                label("Paired phone", "已配对手机"),
                Some(
                    match &device {
                        Some(name) => format!(
                            "{name} · {connected} {}",
                            label("open connections", "个连接")
                        ),
                        None => label("None", "无").to_string(),
                    }
                    .into(),
                ),
                revoke,
                ControlWidth::Default,
            ),
        ]));
        if let Some(error) = error {
            column = column.child(div().text_color(theme.destructive).child(error));
        }
        if address.is_some() {
            let pair = Button::new(
                "settings-phone-new-link",
                label("New pairing link", "生成配对链接"),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.state.update(cx, |s, cx| s.new_phone_pairing_link(cx));
            }));
            column = column.child(div().flex().child(pair));
        }
        if let Some(link) = link {
            let copy_link = link.clone();
            let copy = Button::new("settings-phone-copy-link", label("Copy link", "复制链接"))
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copy_link.clone()))
                });
            column = column.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_4()
                    .items_start()
                    .children(qr_code(&link))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(200.))
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(label(
                                "Scan with the phone's camera within 5 minutes. Pairing replaces any previously paired phone.",
                                "请在 5 分钟内用手机相机扫描。新配对会替换之前配对的手机。",
                            ))
                            .child(div().flex().child(copy)),
                    ),
            );
        }
        column.into_any_element()
    }

    fn phone_client_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = active_theme();
        let client = &self.state.read(cx).phone_client;
        let desktop = client
            .target
            .as_ref()
            .map(|t| format!("{}:{}", t.host, t.port));
        let paired = desktop.is_some();
        let status = client.status.clone();
        let paste = Button::new(
            "settings-phone-pair-clipboard",
            label("Pair from clipboard", "从剪贴板配对"),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            let text = cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .unwrap_or_default();
            this.state.update(cx, |s, cx| s.pair_with_link(&text, cx));
        }));
        let forget = Button::new(
            "settings-phone-forget",
            label("Forget desktop", "忘记桌面端"),
        )
        .disabled(!paired)
        .on_click(cx.listener(|this, _, _, cx| {
            this.state.update(cx, |s, cx| s.forget_desktop(cx));
        }));
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(settings_card([
                settings_row(
                    label("Desktop", "桌面端"),
                    Some(status.into()),
                    div()
                        .text_color(theme.subtle)
                        .child(desktop.unwrap_or_else(|| label("Not paired", "未配对").into())),
                    ControlWidth::Default,
                ),
                settings_row(
                    label("Forget this desktop", "忘记此桌面端"),
                    None,
                    forget,
                    ControlWidth::Default,
                ),
            ]))
            .child(div().text_color(theme.subtle).child(label(
                "To pair, open Settings › Phone on the desktop, turn it on and scan the QR code with the camera. You can also copy the link there and use Pair from clipboard.",
                "配对方法：在桌面端打开 设置 › 手机，开启后用相机扫描二维码；也可以复制链接后点击“从剪贴板配对”。",
            )))
            .child(div().flex().child(paste))
            .into_any_element()
    }
}
