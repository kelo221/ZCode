//! Plugin Store pane implementing the vocabulary and UX of CONTEXT.md:
//! Public/Personal segments, Installed Strip, Manage Installed and detail/configuration.

use crate::app::root::RootView;
use crate::shared::plugins::PluginsOverviewResult;
use crate::shared::theme::ui_size;
use crate::shared::theme::{ACCENT, BORDER, CARD, HOVER, MUTED, PANEL, SUCCESS, TEXT, WARNING};
use crate::shared::theme_colors::color as rgb;
use gpui::{
    AnyElement, Context, CursorStyle, ElementId, InteractiveElement, IntoElement, ParentElement,
    Styled, div, prelude::*, px,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PluginSegment {
    #[default]
    Public,
    Personal,
    ManageInstalled,
}

impl RootView {
    pub(crate) fn plugins_pane(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_inspection(crate::app::inspection_view::InspectionKind::Plugins, cx);
        let feedback =
            self.inspection_feedback(crate::app::inspection_view::InspectionKind::Plugins, cx);
        let overview = self
            .state
            .read(cx)
            .active_inspection()
            .and_then(|i| i.plugins.value.clone());

        div()
            .id("plugins-pane")
            .flex_1()
            .flex()
            .flex_col()
            .bg(rgb(PANEL))
            .min_h_0()
            .child(self.render_plugins_header(cx))
            .child(feedback)
            .child(self.render_installed_strip(&overview, cx))
            .child(match overview {
                Some(overview) => div()
                    .id("plugins-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        if let Some(config) = self.selected_plugin_config(window, cx) {
                            config
                        } else if let Some(detail) = self.selected_plugin_detail(cx) {
                            detail
                        } else {
                            match if self.settings.open {
                                PluginSegment::ManageInstalled
                            } else {
                                self.plugin_segment
                            } {
                                PluginSegment::Public => self.render_public_segment(&overview, cx),
                                PluginSegment::Personal => {
                                    self.render_personal_segment(&overview, cx)
                                }
                                PluginSegment::ManageInstalled => {
                                    self.render_manage_installed(&overview, cx)
                                }
                            }
                        },
                    ),
                None => div()
                    .id("plugins-loading")
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(ui_size(12.)))
                    .text_color(rgb(MUTED))
                    .child(crate::shared::i18n::label(
                        "No plugin catalog loaded",
                        "尚未加载插件目录",
                    )),
            })
            .into_any_element()
    }

    fn render_plugins_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let active = self.plugin_segment;
        let management = self.settings.open;
        let tabs = if management {
            &[(PluginSegment::ManageInstalled, "Manage Installed")][..]
        } else {
            &[
                (PluginSegment::Public, "Public (Official)"),
                (PluginSegment::Personal, "Personal Sources"),
                (PluginSegment::ManageInstalled, "Manage Installed"),
            ][..]
        };

        div()
            .id("plugins-header")
            .flex()
            .flex_col()
            .items_start()
            .gap_1()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .children(tabs.iter().map(|&(seg, label)| {
                        let is_active = active == seg;
                        div()
                            .id(ElementId::NamedInteger("seg-tab".into(), seg as u64))
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .text_size(px(ui_size(11.)))
                            .font_weight(if is_active {
                                gpui::FontWeight::SEMIBOLD
                            } else {
                                gpui::FontWeight::NORMAL
                            })
                            .bg(if is_active { rgb(CARD) } else { rgb(PANEL) })
                            .text_color(if is_active { rgb(TEXT) } else { rgb(MUTED) })
                            .cursor(CursorStyle::PointingHand)
                            .hover(|h| h.bg(rgb(HOVER)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.plugin_segment = seg;
                                cx.notify();
                            }))
                            .child(label)
                    })),
            )
            .child(self.plugin_mutation_button(
                "refresh-plugins".into(),
                crate::shared::i18n::label("Manual Refresh", "手动刷新").into(),
                crate::app::plugin_controls::PluginMutation::Refresh,
                cx,
            ))
            .into_any_element()
    }

    fn render_installed_strip(
        &self,
        overview: &Option<PluginsOverviewResult>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(ov) = overview else {
            return div().id("installed-strip-empty").into_any_element();
        };
        if ov.installed_plugins.is_empty() {
            return div().id("installed-strip-none").into_any_element();
        }

        div()
            .id("installed-strip")
            .flex()
            .items_center()
            .gap_1p5()
            .px_3()
            .py_1p5()
            .bg(rgb(0x131d2e))
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .text_size(px(ui_size(10.)))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child("INSTALLED:"),
            )
            .children(ov.installed_plugins.iter().map(|p| {
                let name = p.display_label().to_string();
                let enabled = p.enabled;
                let strip_id = format!("strip-{}", p.id);
                let owner = self.state.read(cx).active_ws_key();
                let identity = crate::backend::plugin_detail::PluginDetailIdentity {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    marketplace: p.marketplace.clone(),
                };
                div()
                    .id(ElementId::Name(strip_id.into()))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1p5()
                    .py_0p5()
                    .rounded_sm()
                    .bg(rgb(CARD))
                    .text_size(px(ui_size(10.)))
                    .text_color(rgb(TEXT))
                    .cursor(CursorStyle::PointingHand)
                    .hover(|h| h.bg(rgb(HOVER)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(owner) = &owner {
                            this.state.update(cx, |s, cx| {
                                s.open_plugin_detail(owner, identity.clone(), cx)
                            });
                            this.restore_orphan_management(owner, &identity, cx);
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(ui_size(8.)))
                            .text_color(if enabled { rgb(SUCCESS) } else { rgb(MUTED) })
                            .child(if enabled { "●" } else { "○" }),
                    )
                    .child(name)
            }))
            .into_any_element()
    }

    fn render_public_segment(
        &self,
        overview: &PluginsOverviewResult,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let official_mp = overview.marketplaces.iter().find(|m| m.is_official);
        let featured_ids = official_mp
            .and_then(|m| m.featured.as_ref())
            .cloned()
            .unwrap_or_default();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .when(!featured_ids.is_empty(), |el| {
                let featured_plugins: Vec<_> = overview
                    .available_plugins
                    .iter()
                    .filter(|p| featured_ids.contains(&p.id))
                    .collect();
                if featured_plugins.is_empty() {
                    return el;
                }
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .child(
                            div()
                                .text_size(px(ui_size(11.)))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(ACCENT))
                                .child("★ FEATURED PLUGINS"),
                        )
                        .children(
                            featured_plugins
                                .into_iter()
                                .map(|p| self.render_available_card(p, true, cx)),
                        ),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(
                        div()
                            .text_size(px(ui_size(11.)))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(MUTED))
                            .child("ALL OFFICIAL PLUGINS"),
                    )
                    .children(
                        overview
                            .available_plugins
                            .iter()
                            .filter(|p| p.marketplace == "zcode-plugins-official")
                            .map(|p| self.render_available_card(p, false, cx)),
                    ),
            )
            .when(!overview.restorable_builtins.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .child(
                            div()
                                .text_size(px(ui_size(11.)))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(WARNING))
                                .child("RESTORABLE BUILTINS"),
                        )
                        .children(
                            overview
                                .restorable_builtins
                                .iter()
                                .map(|p| self.render_restorable_builtin_row(p, cx)),
                        ),
                )
            })
            .into_any_element()
    }

    fn render_personal_segment(
        &mut self,
        overview: &PluginsOverviewResult,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let personal_markets: Vec<_> = overview
            .marketplaces
            .iter()
            .filter(|m| !m.is_official)
            .collect();

        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_size(px(ui_size(11.)))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(MUTED))
                    .child("PERSONAL SOURCES"),
            )
            .children(self.render_add_source(cx))
            .child(if personal_markets.is_empty() {
                div()
                    .p_3()
                    .rounded_md()
                    .bg(rgb(CARD))
                    .text_size(px(ui_size(11.)))
                    .text_color(rgb(MUTED))
                    .child("No personal marketplace sources added yet.")
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(personal_markets.into_iter().map(|m| {
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(self.render_marketplace_card(m, cx))
                            .children(
                                overview
                                    .available_plugins
                                    .iter()
                                    .filter(|p| p.marketplace == m.id)
                                    .map(|p| self.render_available_card(p, false, cx)),
                            )
                    }))
                    .into_any_element()
            })
            .into_any_element()
    }

    fn render_manage_installed(
        &self,
        overview: &PluginsOverviewResult,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if overview.installed_plugins.is_empty() {
            return div()
                .p_3()
                .rounded_md()
                .bg(rgb(CARD))
                .text_size(px(ui_size(11.)))
                .text_color(rgb(MUTED))
                .child("No installed plugins.")
                .into_any_element();
        }

        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(
                overview
                    .installed_plugins
                    .iter()
                    .map(|p| self.render_installed_row(p, cx)),
            )
            .into_any_element()
    }
}
