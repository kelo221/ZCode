//! Right dock container: tab strip (Review / Files / Workflows) + the active
//! pane, toggled from the header pill or CmdOrCtrl+Alt+B (PARITY.md M3 "pane
//! container: dockable right pane + shortcuts"). Plugins, MCP servers and
//! usage stats are Settings-only, as on Desktop (`settingsPageConfig.ts`).

use crate::app::layout::HEADER_HEIGHT;
use crate::app::layout::{PANEL_RADIUS, SIDE_PANE_WIDTH};
use crate::app::root::RootView;
use crate::shared::theme::ui_size;
use crate::shared::theme::{BORDER, HOVER, MUTED, PANEL, SELECTED, TEXT};
use crate::shared::theme_colors::{color as rgb, tinted};
use ely_gpui_component::buttons::{ButtonVariant, IconButton};
use ely_gpui_component::primitives::IconName;
use ely_gpui_component::theme::ControlSize;
use gpui::{
    AnyElement, Context, CursorStyle, InteractiveElement, IntoElement, ParentElement, SharedString,
    Styled, actions, div, prelude::*, px,
};

actions!(zcode_gpui, [ToggleDock]);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DockTab {
    Review,
    Files,
    Workflows,
}

/// Desktop `w-80`: the floating status panel is 340px wide.
pub const DOCK_WIDTH: f32 = 340.;
/// Desktop `max-h-[min(64dvh,32rem)]`: the panel never exceeds 540px.
pub const DOCK_MAX_HEIGHT: f32 = 540.;
const TABS: [DockTab; 3] = [DockTab::Review, DockTab::Files, DockTab::Workflows];

impl DockTab {
    pub fn label(self) -> &'static str {
        match self {
            DockTab::Review => "Review",
            DockTab::Files => "Files",
            DockTab::Workflows => "Workflows",
        }
    }
}

impl RootView {
    /// The whole dock column, shown when `dock_open`. Docked it is a full-height
    /// side panel beside the centre panel; on narrow windows it floats over the
    /// transcript instead.
    pub(crate) fn dock_pane(&mut self, docked: bool, cx: &mut Context<Self>) -> AnyElement {
        self.sync_dock_workspace(cx);
        let active = self.dock_tab;
        div()
            .when(docked, |el| {
                el.w(px(SIDE_PANE_WIDTH))
                    .flex_none()
                    .h_full()
                    .rounded(px(PANEL_RADIUS))
            })
            .when(!docked, |el| {
                el.absolute()
                    .top_2()
                    .right_2()
                    .w(px(DOCK_WIDTH))
                    .h(px(DOCK_MAX_HEIGHT))
                    .rounded_lg()
            })
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .min_h_0()
            // Tab strip: header height and divider match the workspace header;
            // tabs scroll horizontally when the overlay is too narrow for all of them.
            .child(
                div()
                    .h(px(HEADER_HEIGHT))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .pl_2()
                    .pr_2()
                    .border_b_1()
                    .border_color(tinted(BORDER, 0.5))
                    .child(
                        div()
                            .id("dock-tabs")
                            .flex_1()
                            .min_w_0()
                            .overflow_x_scroll()
                            .flex()
                            .items_center()
                            .gap_0p5()
                            .children(TABS.map(|tab| {
                                let selected = tab == active;
                                div()
                                    .id(SharedString::from(format!("dock-tab-{}", tab.label())))
                                    .flex_none()
                                    .h(px(28.))
                                    .px(px(10.))
                                    .flex()
                                    .items_center()
                                    .rounded_lg()
                                    .text_size(px(ui_size(13.)))
                                    .cursor(CursorStyle::PointingHand)
                                    .when(selected, |el| el.bg(rgb(SELECTED)).text_color(rgb(TEXT)))
                                    .when(!selected, |el| {
                                        el.text_color(rgb(MUTED))
                                            .hover(|h| h.bg(rgb(HOVER)).text_color(rgb(TEXT)))
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.dock_tab = tab;
                                        this.on_dock_tab(tab, cx);
                                    }))
                                    .child(tab.label())
                            })),
                    )
                    .child(
                        IconButton::new("dock-close", IconName::X)
                            .size(ControlSize::Md)
                            .variant(ButtonVariant::Ghost)
                            .tooltip("Close")
                            .on_click(cx.listener(|this, _, _, cx| {
                                cx.stop_propagation();
                                this.dock_open = false;
                                cx.notify();
                            })),
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
                    }),
            )
            .into_any_element()
    }

    /// First-open hooks: fetch git status / load the file tree.
    pub(crate) fn on_dock_tab(&mut self, tab: DockTab, cx: &mut Context<Self>) {
        match tab {
            DockTab::Review => self.sync_git_workspace(cx),
            DockTab::Files => self.ensure_files_loaded(cx),
            DockTab::Workflows => {}
        }
        cx.notify();
    }

    /// Re-sync the open pane after the active workspace changed underneath
    /// it. Both checks are no-ops once the pane has requested that workspace.
    fn sync_dock_workspace(&mut self, cx: &mut Context<Self>) {
        match self.dock_tab {
            DockTab::Review => self.sync_git_workspace(cx),
            DockTab::Files => self.ensure_files_loaded(cx),
            DockTab::Workflows => {}
        }
    }
}
