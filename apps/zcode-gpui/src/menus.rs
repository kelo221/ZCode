//! Header selectors (model / thinking level popup menus) and the collaboration
//! mode bar (build / edit / plan / full-access), mirroring the desktop picker
//! semantics (packages/ui/src/v4/composer/V4ComposerModeControls.tsx).

use crate::config_cmds::EffectiveConfig;
use crate::model::format_preview;
use crate::theme::{ACCENT, BG, BORDER, CARD, MUTED, TEXT};
use gpui::{
    ClickEvent, Context, CursorStyle, IntoElement, ParentElement, SharedString, Styled, div,
    prelude::*, px, rgb,
};

#[derive(Clone, Copy, PartialEq)]
pub enum MenuKind {
    Model,
    Thought,
}

pub const MODES: [(&str, &str); 4] = [
    ("build", "Ask before changes"),
    ("edit", "Edit automatically"),
    ("plan", "Plan mode"),
    ("yolo", "Full access"),
];

impl crate::ui::RootView {
    fn menu_button(
        &self,
        id: &'static str,
        label: SharedString,
        kind: MenuKind,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let open = self.open_menu == Some(kind);
        div()
            .id(id)
            .px_2()
            .py_1()
            .rounded_md()
            .text_size(px(11.5))
            .text_color(rgb(if open { ACCENT } else { MUTED }))
            .cursor(CursorStyle::PointingHand)
            .when(open, |el| el.bg(rgb(CARD)))
            .hover(|s| s.bg(rgb(CARD)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                cx.stop_propagation();
                this.open_menu = if this.open_menu == Some(kind) {
                    None
                } else {
                    Some(kind)
                };
                cx.notify();
            }))
            .child(label)
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
            .when(selected, |el| el.bg(rgb(BG)))
            .hover(|s| s.bg(rgb(BG)))
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
                    .text_size(px(12.5))
                    .text_color(rgb(if selected { ACCENT } else { TEXT }))
                    .child(div().w(px(10.)).child(if selected { "✓" } else { "" }))
                    .child(label),
            )
            .children((!detail.is_empty()).then(|| {
                div()
                    .text_size(px(10.5))
                    .text_color(rgb(MUTED))
                    .pl(px(16.))
                    .child(detail)
            }))
    }

    fn menu_shell(&self, content: Vec<gpui::AnyElement>) -> impl IntoElement {
        div()
            .id("menu-scroll")
            .absolute()
            .top_full()
            .right_0()
            .mt_1()
            .w(px(300.))
            .max_h(px(380.))
            .overflow_y_scroll()
            .bg(rgb(CARD))
            .border_1()
            .border_color(rgb(BORDER))
            .rounded_md()
            .py_1()
            .children(content)
    }

    /// Model + thinking-level buttons with popup menus, for the header bar.
    pub(crate) fn header_selectors(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let effective = self.state.read(cx).effective_config();
        let models: Vec<crate::catalog::ModelOption> = self
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
                    cx,
                )
                .into_any_element()
            }))
            .children((!effective.thought_levels.is_empty()).then(|| {
                self.menu_button(
                    "thought-menu",
                    effective.thought_display().into(),
                    MenuKind::Thought,
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
                                        format!("thinking: {level}"),
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
                        ),
                    )
                },
            )
    }

    /// Segmented mode control: build / edit / plan / yolo.
    pub(crate) fn mode_bar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let effective: EffectiveConfig = self.state.read(cx).effective_config();
        let current = effective.mode.clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .children(MODES.map(|(id, label)| {
                let selected = current == id;
                div()
                    .id(id)
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .text_size(px(11.))
                    .cursor(CursorStyle::PointingHand)
                    .when(selected, |el| {
                        el.bg(rgb(BG))
                            .border_1()
                            .border_color(rgb(ACCENT))
                            .text_color(rgb(ACCENT))
                    })
                    .when(!selected, |el| {
                        el.text_color(rgb(MUTED)).hover(|s| s.bg(rgb(CARD)))
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        this.state.update(cx, |s, cx| {
                            if s.effective_config().has_session {
                                s.switch_mode(id, cx);
                            } else {
                                s.ui_mode = Some(id.to_string());
                            }
                        });
                        cx.notify();
                    }))
                    .child(label)
            }))
    }
}
