//! Right dock container: tab strip (Review / Files) + the active
//! pane, toggled from the header pill or Ctrl+B (PARITY.md M3 "pane
//! container: dockable right pane + shortcuts").

use crate::app::root::RootView;
use crate::shared::theme::{BORDER, CARD, HOVER, MUTED, PANEL, TEXT};
use gpui::{
    AnyElement, Context, CursorStyle, InteractiveElement, IntoElement, ParentElement, SharedString,
    Styled, actions, div, prelude::*, px, rgb,
};

// Ctrl+B toggles the right dock (aligned with desktop SidePane shortcuts).
actions!(zcode_gpui, [ToggleDock]);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DockTab {
    Review,
    Files,
    Workflows,
    Usage,
    Mcp,
    Plugins,
}

/// Desktop `w-80`: the floating status panel is 340px wide.
pub const DOCK_WIDTH: f32 = 340.;
/// Desktop `max-h-[min(64dvh,32rem)]`: the panel never exceeds 540px.
pub const DOCK_MAX_HEIGHT: f32 = 540.;
const TABS: [DockTab; 6] = [
    DockTab::Review,
    DockTab::Files,
    DockTab::Workflows,
    DockTab::Usage,
    DockTab::Mcp,
    DockTab::Plugins,
];

impl DockTab {
    pub fn label(self) -> &'static str {
        match self {
            DockTab::Review => "Review",
            DockTab::Files => "Files",
            DockTab::Workflows => "Workflows",
            DockTab::Usage => "Usage",
            DockTab::Mcp => "MCP",
            DockTab::Plugins => "Plugins",
        }
    }
}

impl RootView {
    /// The whole dock column, shown when `dock_open`.
    pub(crate) fn dock_pane(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.sync_dock_workspace(cx);
        let active = self.dock_tab;
        // Floating overlay (desktop "Git tools" parity): draws over the
        // transcript instead of taking layout space from it.
        div()
            .absolute()
            .top_2()
            .right_2()
            .w(px(DOCK_WIDTH))
            .h(px(DOCK_MAX_HEIGHT))
            .flex()
            .flex_col()
            .rounded_lg()
            .overflow_hidden()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(0x3f3f46))
            .min_h_0()
            // Tab strip.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_2()
                    .h(px(34.))
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .children(TABS.map(|tab| {
                        let selected = tab == active;
                        div()
                            .id(SharedString::from(format!("dock-tab-{}", tab.label())))
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .text_size(px(11.))
                            .cursor(CursorStyle::PointingHand)
                            .when(selected, |el| el.bg(rgb(CARD)).text_color(rgb(TEXT)))
                            .when(!selected, |el| {
                                el.text_color(rgb(MUTED)).hover(|h| h.bg(rgb(HOVER)))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.dock_tab = tab;
                                this.on_dock_tab(tab, cx);
                            }))
                            .child(tab.label())
                    }))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("dock-close")
                            .px_1p5()
                            .rounded_sm()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .cursor(CursorStyle::PointingHand)
                            .hover(|h| h.text_color(rgb(TEXT)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.dock_open = false;
                                cx.notify();
                            }))
                            .child("✕"),
                    ),
            )
            // Active pane body.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(match active {
                        DockTab::Review => self.review_pane(cx),
                        DockTab::Files => self.files_pane(cx),
                        DockTab::Workflows => self.workflows_pane(cx),
                        DockTab::Usage => self.usage_pane(cx),
                        DockTab::Mcp => self.mcp_pane(cx),
                        DockTab::Plugins => self.plugins_pane(cx),
                    }),
            )
            .into_any_element()
    }

    /// First-open hooks: fetch git status / load the file tree / queries.
    pub(crate) fn on_dock_tab(&mut self, tab: DockTab, cx: &mut Context<Self>) {
        match tab {
            DockTab::Review => self.sync_git_workspace(cx),
            DockTab::Files => self.ensure_files_loaded(cx),
            DockTab::Workflows => {}
            DockTab::Usage => {
                self.state.update(cx, |state, cx| {
                    state.fetch_usage_stats("7d", cx);
                });
            }
            DockTab::Mcp => {
                self.state.update(cx, |state, cx| {
                    state.fetch_mcp_servers(cx);
                });
            }
            DockTab::Plugins => {
                self.state.update(cx, |state, cx| {
                    state.fetch_plugins_overview(cx);
                });
            }
        }
        cx.notify();
    }

    /// Re-sync the open pane after the active workspace changed underneath
    /// it. Both checks are no-ops once the pane has requested that workspace.
    fn sync_dock_workspace(&mut self, cx: &mut Context<Self>) {
        match self.dock_tab {
            DockTab::Review => self.sync_git_workspace(cx),
            DockTab::Files => self.ensure_files_loaded(cx),
            DockTab::Workflows | DockTab::Usage | DockTab::Mcp | DockTab::Plugins => {}
        }
    }
}
