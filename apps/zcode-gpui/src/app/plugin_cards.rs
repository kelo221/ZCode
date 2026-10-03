//! Card and row renderers for the plugin store.
//!
//! Separated from plugin_pane.rs to maintain the strict <= 400 lines limit;
//! marketplace and restorable-builtin rows live in plugin_sources.rs.

use crate::app::root::RootView;
use crate::shared::plugins::{AvailablePluginSummary, InstalledPluginSummary, available_card_id};
use crate::shared::theme::{ACCENT, BORDER, CARD, DANGER, MUTED, SUCCESS, TEXT, WARNING};
use gpui::{
    AnyElement, Context, CursorStyle, ElementId, InteractiveElement, IntoElement, ParentElement,
    Styled, div, prelude::*, px, rgb,
};

impl RootView {
    pub(crate) fn render_available_card(
        &self,
        p: &AvailablePluginSummary,
        is_featured: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = p.name.clone();
        let market = p.marketplace.clone();
        let installed = p.installed;
        let example_prompts = p
            .listing
            .as_ref()
            .and_then(|l| l.example_prompts.clone())
            .unwrap_or_default();
        let requires_paid = p
            .listing
            .as_ref()
            .and_then(|l| l.requires_paid_plan)
            .unwrap_or(false);

        div()
            .id(ElementId::Name(
                available_card_id(&p.id, &market, is_featured).into(),
            ))
            .flex()
            .flex_col()
            .gap_1p5()
            .p_2p5()
            .rounded_md()
            .bg(rgb(CARD))
            .border_1()
            .border_color(if is_featured {
                rgb(ACCENT)
            } else {
                rgb(BORDER)
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .child(p.display_label().to_string()),
                            )
                            .when(requires_paid, |el| {
                                el.child(
                                    div()
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(rgb(0x382c14))
                                        .text_size(px(9.5))
                                        .text_color(rgb(WARNING))
                                        .child("Paid Plan"),
                                )
                            }),
                    )
                    .child(if installed {
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .bg(rgb(0x133827))
                            .text_size(px(10.))
                            .text_color(rgb(SUCCESS))
                            .child("Installed")
                            .into_any_element()
                    } else {
                        let n = name.clone();
                        let m = market.clone();
                        div()
                            .id("install")
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .bg(rgb(ACCENT))
                            .text_size(px(10.5))
                            .text_color(rgb(0xffffff))
                            .cursor(CursorStyle::PointingHand)
                            .hover(|h| h.bg(rgb(0x2563eb)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                let n = n.clone();
                                let m = m.clone();
                                this.state.update(cx, |s, cx| s.install_plugin(&n, &m, cx));
                            }))
                            .child("Install")
                            .into_any_element()
                    }),
            )
            .child(
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(MUTED))
                    .child(p.description.clone().unwrap_or_default()),
            )
            .when(!example_prompts.is_empty(), |el| {
                el.child(
                    div().flex().flex_wrap().gap_1().mt_1().children(
                        example_prompts
                            .into_iter()
                            .take(2)
                            .enumerate()
                            .map(|(i, prompt)| {
                                let pr = prompt.clone();
                                div()
                                    .id(ElementId::NamedInteger("prompt".into(), i as u64))
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(rgb(0x1a2436))
                                    .border_1()
                                    .border_color(rgb(0x2b3d5c))
                                    .text_size(px(9.5))
                                    .text_color(rgb(ACCENT))
                                    .cursor(CursorStyle::PointingHand)
                                    .hover(|h| h.bg(rgb(0x24334d)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        let text = pr.clone();
                                        this.state.update(cx, |s, cx| {
                                            s.composer.update(cx, |c, _cx| {
                                                c.set_text(&text);
                                            });
                                        });
                                    }))
                                    .child(format!("💬 {prompt}"))
                            }),
                    ),
                )
            })
            .into_any_element()
    }

    pub(crate) fn render_installed_row(
        &self,
        p: &InstalledPluginSummary,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p_id = p.id.clone();
        let p_market = p.marketplace.clone();
        let enabled = p.enabled;
        let has_update = p.update_status.as_deref() == Some("update-available");
        let latest_ver = p.latest_version.clone();

        div()
            .flex()
            .items_center()
            .justify_between()
            .p_2p5()
            .rounded_md()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT))
                                    .child(p.display_label().to_string()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(MUTED))
                                    .child(format!("v{}", p.version.as_deref().unwrap_or("0.1.0"))),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(MUTED))
                            .child(format!("source: {}", p.marketplace)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .when(has_update, |el| {
                        let id = p_id.clone();
                        let m = p_market.clone();
                        let btn_update_id = format!("update-{}", id);
                        el.child(
                            div()
                                .id(ElementId::Name(btn_update_id.into()))
                                .px_2()
                                .py_0p5()
                                .rounded_sm()
                                .bg(rgb(0x1e3a5f))
                                .text_size(px(10.))
                                .text_color(rgb(ACCENT))
                                .cursor(CursorStyle::PointingHand)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    let id = id.clone();
                                    let m = m.clone();
                                    this.state.update(cx, |s, cx| s.update_plugin(&id, &m, cx));
                                }))
                                .child(format!(
                                    "Update ({})",
                                    latest_ver.as_deref().unwrap_or("new")
                                )),
                        )
                    })
                    .child({
                        let id = p_id.clone();
                        let btn_toggle_id = format!("toggle-{}", id);
                        div()
                            .id(ElementId::Name(btn_toggle_id.into()))
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .bg(if enabled {
                                rgb(0x133827)
                            } else {
                                rgb(0x2a2f3a)
                            })
                            .text_size(px(10.))
                            .text_color(if enabled { rgb(SUCCESS) } else { rgb(MUTED) })
                            .cursor(CursorStyle::PointingHand)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                let id = id.clone();
                                this.state
                                    .update(cx, |s, cx| s.set_plugin_enabled(&id, !enabled, cx));
                            }))
                            .child(if enabled { "Enabled" } else { "Disabled" })
                    })
                    .child({
                        let id = p_id.clone();
                        let m = p_market.clone();
                        let btn_uninstall_id = format!("uninstall-{}", id);
                        div()
                            .id(ElementId::Name(btn_uninstall_id.into()))
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .bg(rgb(0x3f1d24))
                            .text_size(px(10.))
                            .text_color(rgb(DANGER))
                            .cursor(CursorStyle::PointingHand)
                            .hover(|h| h.bg(rgb(0x5c1d24)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                let id = id.clone();
                                let m = m.clone();
                                this.state
                                    .update(cx, |s, cx| s.uninstall_plugin(&id, &m, cx));
                            }))
                            .child("Uninstall")
                    }),
            )
            .into_any_element()
    }
}
