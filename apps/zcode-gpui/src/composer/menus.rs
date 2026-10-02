//! Composer pickers: collaboration mode (build / edit / plan / full access),
//! model and thinking level, as upward popup menus like the desktop composer
//! (packages/ui/src/v4/composer/V4ComposerModeControls.tsx).

use crate::composer::config_cmds::EffectiveConfig;
use crate::conversation::model::format_preview;
use crate::shared::theme::{
    BORDER, CARD, CARD_HOVER, I_BULB, I_CHEVRON_DOWN, I_SHIELD, MUTED, TEXT, WARNING, icon,
};
use gpui::{
    ClickEvent, Context, CursorStyle, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px, rgb,
};

#[derive(Clone, Copy, PartialEq)]
pub enum MenuKind {
    Mode,
    Model,
    Thought,
}

pub const MODES: [(&str, &str); 4] = [
    ("build", "Ask before changes"),
    ("edit", "Edit automatically"),
    ("plan", "Plan mode"),
    ("yolo", "Full access"),
];

impl crate::app::root::RootView {
    /// Borderless picker trigger: optional icon, label, chevron.
    fn menu_button(
        &self,
        id: &'static str,
        label: SharedString,
        kind: MenuKind,
        lead: Option<(char, u32)>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let open = self.open_menu == Some(kind);
        let color = lead.map(|(_, c)| c).unwrap_or(MUTED);
        div()
            .id(id)
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .px_2()
            .h(px(28.))
            .rounded_md()
            .text_size(px(13.))
            .text_color(rgb(if lead.is_some() { color } else { TEXT }))
            .cursor(CursorStyle::PointingHand)
            .when(open, |el| el.bg(rgb(CARD_HOVER)))
            .hover(|s| s.bg(rgb(CARD_HOVER)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                cx.stop_propagation();
                this.open_menu = if this.open_menu == Some(kind) {
                    None
                } else {
                    Some(kind)
                };
                cx.notify();
            }))
            .children(lead.map(|(g, c)| icon(g, 13., c)))
            .child(label)
            .child(icon(I_CHEVRON_DOWN, 9., MUTED))
    }

    fn menu_option(
        &self,
        label: String,
        detail: String,
        selected: bool,
        index: usize,
        cx: &mut Context<Self>,
        on_pick: impl Fn(&mut Self, usize, &mut Context<Self>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let label: SharedString = label.into();
        let detail: SharedString = detail.into();
        div()
            .id(index)
            .px_3()
            .py_1p5()
            .flex()
            .flex_col()
            .gap_0p5()
            .cursor(CursorStyle::PointingHand)
            .when(selected, |el| el.bg(rgb(CARD_HOVER)))
            .hover(|s| s.bg(rgb(CARD_HOVER)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                cx.stop_propagation();
                on_pick(this, index, cx);
                this.open_menu = None;
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .text_size(px(13.))
                    .text_color(rgb(TEXT))
                    .child(div().w(px(10.)).child(if selected { "✓" } else { "" }))
                    .child(label),
            )
            .children((!detail.is_empty()).then(|| {
                div()
                    .text_size(px(12.))
                    .text_color(rgb(MUTED))
                    .pl(px(16.))
                    .child(detail)
            }))
    }

    /// Popup above its trigger (the pickers live at the bottom of the window).
    fn menu_shell(&self, content: Vec<gpui::AnyElement>, align_left: bool) -> impl IntoElement {
        div()
            .id("menu-scroll")
            .absolute()
            .bottom_full()
            .when(align_left, |el| el.left_0())
            .when(!align_left, |el| el.right_0())
            .mb_2()
            .shadow_lg()
            .w(px(300.))
            .max_h(px(380.))
            .overflow_y_scroll()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded_lg()
            .py_1()
            .children(content)
    }

    /// Model + thinking-level pickers for the composer's bottom row.
    pub(crate) fn composer_selectors(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let effective = self.state.read(cx).effective_config();
        let models: Vec<crate::composer::catalog::ModelOption> = self
            .state
            .read(cx)
            .active_ws_key()
            .and_then(|k| self.state.read(cx).workspace_configs.get(&k))
            .map(|c| c.models.clone())
            .unwrap_or_default();

        div()
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .children((!models.is_empty()).then(|| {
                self.menu_button(
                    "model-menu",
                    effective.model_display().into(),
                    MenuKind::Model,
                    None,
                    cx,
                )
                .into_any_element()
            }))
            .children((!effective.thought_levels.is_empty()).then(|| {
                self.menu_button(
                    "thought-menu",
                    effective.thought_display().into(),
                    MenuKind::Thought,
                    Some((I_BULB, TEXT)),
                    cx,
                )
                .into_any_element()
            }))
            .when(
                self.open_menu == Some(MenuKind::Model) && !models.is_empty(),
                |el| {
                    el.child(
                        self.menu_shell(
                            models
                                .iter()
                                .enumerate()
                                .map(|(i, opt)| {
                                    let selected = effective.provider == opt.provider
                                        && effective.model == opt.model;
                                    let detail = if opt.provider_name.is_empty() {
                                        format_preview(&opt.model, 40)
                                    } else {
                                        format_preview(
                                            &format!("{} · {}", opt.provider_name, opt.model),
                                            48,
                                        )
                                    };
                                    let value_provider = opt.provider.clone();
                                    let value_model = opt.model.clone();
                                    // Store the RAW option value so draft-config
                                    // lookup (find_model_option) matches exactly.
                                    let raw_value = opt.value.clone();
                                    let thought = if opt.thought_levels.is_empty() {
                                        String::new()
                                    } else {
                                        opt.default_thought.clone()
                                    };
                                    self.menu_option(
                                        opt.name.clone(),
                                        detail,
                                        selected,
                                        i,
                                        cx,
                                        move |this, _, cx| {
                                            this.state.update(cx, |s, cx| {
                                                if s.effective_config().has_session {
                                                    s.switch_model(
                                                        &value_provider,
                                                        &value_model,
                                                        &thought,
                                                        cx,
                                                    );
                                                } else {
                                                    s.ui_model_value = Some(raw_value.clone());
                                                }
                                            });
                                        },
                                    )
                                    .into_any_element()
                                })
                                .collect(),
                            false,
                        ),
                    )
                },
            )
            .when(
                self.open_menu == Some(MenuKind::Thought) && !effective.thought_levels.is_empty(),
                |el| {
                    let current = effective.thought.clone();
                    let provider = effective.provider.clone();
                    let model = effective.model.clone();
                    let levels = effective.thought_levels.clone();
                    let mut rows: Vec<(String, String)> =
                        vec![("default".into(), "no explicit thinking level".into())];
                    rows.extend(levels.iter().map(|l| (l.clone(), String::new())));
                    el.child(
                        self.menu_shell(
                            rows.iter()
                                .enumerate()
                                .map(|(i, (level, detail))| {
                                    let selected = *level == current
                                        || (level == "default" && current.is_empty());
                                    let lv = level.clone();
                                    let provider = provider.clone();
                                    let model = model.clone();
                                    self.menu_option(
                                        crate::composer::config_cmds::title_case(level),
                                        detail.clone(),
                                        selected,
                                        i,
                                        cx,
                                        move |this, _, cx| {
                                            let thought =
                                                if lv == "default" { "" } else { lv.as_str() };
                                            this.state.update(cx, |s, cx| {
                                                if s.effective_config().has_session {
                                                    if provider.is_empty() || model.is_empty() {
                                                        s.switch_thought(thought, cx);
                                                    } else {
                                                        s.switch_model(
                                                            &provider, &model, thought, cx,
                                                        );
                                                    }
                                                }
                                            });
                                        },
                                    )
                                    .into_any_element()
                                })
                                .collect(),
                            false,
                        ),
                    )
                },
            )
    }

    /// Mode picker (left of the composer row); Full access is highlighted.
    pub(crate) fn mode_menu(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let effective: EffectiveConfig = self.state.read(cx).effective_config();
        let current = effective.mode.clone();
        let label = MODES
            .iter()
            .find(|(id, _)| *id == current)
            .map(|(_, l)| *l)
            .unwrap_or("Ask before changes");
        let color = if current == "yolo" { WARNING } else { MUTED };
        div()
            .relative()
            .child(self.menu_button(
                "mode-menu",
                label.into(),
                MenuKind::Mode,
                Some((I_SHIELD, color)),
                cx,
            ))
            .when(self.open_menu == Some(MenuKind::Mode), |el| {
                let rows = MODES
                    .iter()
                    .enumerate()
                    .map(|(i, (id, label))| {
                        let id = *id;
                        self.menu_option(
                            label.to_string(),
                            String::new(),
                            current == id,
                            i,
                            cx,
                            move |this, _, cx| {
                                this.state.update(cx, |s, cx| {
                                    if s.effective_config().has_session {
                                        s.switch_mode(id, cx);
                                    } else {
                                        s.ui_mode = Some(id.to_string());
                                    }
                                });
                            },
                        )
                        .into_any_element()
                    })
                    .collect();
                el.child(self.menu_shell(rows, true))
            })
    }
}
