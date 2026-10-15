//! Sidebar footer: the Settings row with the active workspace's connection
//! state as a dot on the right; the full description is its tooltip.

use crate::app::root::RootView;
use crate::shared::i18n::{label, t};
use crate::shared::theme::{AMBER, DANGER, MUTED, SUCCESS, icon};
use crate::shared::theme_colors::color as rgb;
use ely_gpui_component::primitives::{IconName, Tooltip};
use gpui::{AnyElement, Context, div, prelude::*, px};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tone {
    Connected,
    /// Starting, restarting, connecting or pairing.
    Busy,
    /// Lazy agent not started yet, or nothing to connect to.
    Idle,
    Failed,
}

/// Classifies `WorkspaceHandle::status` strings.
pub(crate) fn tone(status: &str) -> Tone {
    let s = status.to_ascii_lowercase();
    if s == "connected" || s.starts_with("paired with") {
        Tone::Connected
    } else if s.starts_with("error")
        || s.contains("failed")
        || s.contains("offline")
        || s == "no backend"
        || s == "backend down"
    {
        Tone::Failed
    } else if s.starts_with("starting")
        || s.starts_with("restarting")
        || s.starts_with("connecting")
        || s.starts_with("reconnecting")
        || s.starts_with("pairing")
    {
        Tone::Busy
    } else {
        Tone::Idle
    }
}

impl RootView {
    pub(crate) fn sidebar_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let (status, tooltip) = {
            let state = self.state.read(cx);
            match state.active_ws_key().and_then(|k| state.ws(&k)) {
                Some(w) => (w.status.clone(), format!("{} · {}", w.conn_desc, w.status)),
                None => (
                    String::new(),
                    label("No workspace", "没有工作区").to_string(),
                ),
            }
        };
        let dot_color = match tone(&status) {
            Tone::Connected => SUCCESS,
            Tone::Busy => AMBER,
            Tone::Failed => DANGER,
            Tone::Idle => MUTED,
        };
        let dot = div()
            .id("connection-status")
            .flex_none()
            .p_1()
            .tooltip(Tooltip::text(tooltip))
            .child(div().size(px(8.)).rounded_full().bg(rgb(dot_color)));
        // Desktop sidebar footer: a plain sidebar row, not a bordered button.
        let footer = div().pt_1().pb_2().child(
            crate::sessions::items::nav_row("open-settings".into(), false)
                .child(icon(IconName::Settings, 16., MUTED))
                .child(
                    div()
                        .flex_1()
                        .child(t("quickPick.command.settings").to_string()),
                )
                .child(dot)
                .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx))),
        );
        #[cfg(test)]
        let footer = crate::app::test_support::track_children(footer, vec!["open-settings".into()]);
        footer.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{Tone, tone};

    #[test]
    fn workspace_statuses_map_to_dot_tones() {
        assert_eq!(tone("connected"), Tone::Connected);
        assert_eq!(tone("paired with 192.0.2.1"), Tone::Connected);
        assert_eq!(tone("starting (bun + branch source)"), Tone::Busy);
        assert_eq!(tone("restarting (attempt 2/5)"), Tone::Busy);
        assert_eq!(tone("reconnecting to desktop"), Tone::Busy);
        assert_eq!(tone("idle — click to connect"), Tone::Idle);
        assert_eq!(tone("not paired"), Tone::Idle);
        assert_eq!(tone("error: boom"), Tone::Failed);
        assert_eq!(tone("spawn failed"), Tone::Failed);
        assert_eq!(tone("desktop offline (timed out)"), Tone::Failed);
        assert_eq!(tone("no backend"), Tone::Failed);
    }
}
