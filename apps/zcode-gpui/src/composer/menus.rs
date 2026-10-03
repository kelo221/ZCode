//! Composer pickers built on ely-gpui-component menus: model (provider-grouped),
//! thinking level and collaboration mode. Desktop parity:
//! packages/ui/src/v4/composer/V4ComposerModeControls.tsx + ModelConfigSelect.
//!
//! Ely hosts render a `Ghost` trigger button and float the panel under it,
//! flipping above when the composer sits at the window's bottom edge
//! (ely `float`/`opens_up`), so the old hand-rolled bottom_full popup is gone.

use crate::app::root::RootView;
use crate::composer::catalog::{ModelOption, group_models};
use crate::composer::config_cmds::{EffectiveConfig, title_case};
use ely_gpui_component::buttons::ButtonVariant;
use ely_gpui_component::menus::{DropdownMenu, Menu, MenuItem};
use ely_gpui_component::primitives::IconName;
use gpui::{Context, Entity, IntoElement, ParentElement, Styled, div};

pub const MODES: [(&str, &str); 4] = [
    ("build", "Ask before changes"),
    ("edit", "Edit automatically"),
    ("plan", "Plan mode"),
    ("yolo", "Full access"),
];

impl RootView {
    /// Model + thinking-level pickers for the composer's bottom row.
    pub(crate) fn composer_selectors(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let effective = self.state.read(cx).effective_config();
        let models: Vec<ModelOption> = self
            .state
            .read(cx)
            .active_ws_key()
            .and_then(|k| self.state.read(cx).workspace_configs.get(&k))
            .map(|c| c.models.clone())
            .unwrap_or_default();
        let root = cx.entity();

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .children((!models.is_empty()).then(|| {
                DropdownMenu::new(
                    "model-menu",
                    effective.trigger_display(&models),
                    model_menu(&effective, &models, root.clone()),
                )
                .variant(ButtonVariant::Ghost)
                .into_any_element()
            }))
            .children((!effective.thought_levels.is_empty()).then(|| {
                DropdownMenu::new(
                    "thought-menu",
                    effective.thought_display(),
                    thought_menu(&effective, root.clone()),
                )
                .icon(IconName::Lightbulb)
                .variant(ButtonVariant::Ghost)
                .into_any_element()
            }))
    }

    /// Mode picker (left of the composer row); Full access is highlighted by
    /// the selected row's radio state.
    pub(crate) fn mode_menu(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let effective: EffectiveConfig = self.state.read(cx).effective_config();
        let current = effective.mode.clone();
        let label = MODES
            .iter()
            .find(|(id, _)| *id == current)
            .map(|(_, l)| *l)
            .unwrap_or("Ask before changes");
        let root = cx.entity();

        DropdownMenu::new("mode-menu", label, mode_menu(&current, root))
            .icon(IconName::Shield)
            .variant(ButtonVariant::Ghost)
    }
}

/// Provider-grouped model menu (desktop parity: registry groups render as
/// titled sections). Rows carry the RAW option value semantics the draft
/// config lookup (`find_model_option`) matches on.
fn model_menu(effective: &EffectiveConfig, models: &[ModelOption], root: Entity<RootView>) -> Menu {
    let mut menu = Menu::new();
    for group in group_models(models) {
        let items = group.items.iter().map(|opt| {
            let selected = effective.provider == opt.provider && effective.model == opt.model;
            let value_provider = opt.provider.clone();
            let value_model = opt.model.clone();
            // Raw option value (may carry a `$thought` suffix) so draft-config
            // lookup (`find_model_option`) matches exactly.
            let raw_value = opt.value.clone();
            let thought = if opt.thought_levels.is_empty() {
                String::new()
            } else {
                opt.default_thought.clone()
            };
            let root = root.clone();
            MenuItem::radio(opt.name.clone(), selected).on_click(move |_, cx| {
                root.update(cx, |root, cx| {
                    root.state.update(cx, |s, cx| {
                        if s.effective_config().has_session {
                            s.switch_model(&value_provider, &value_model, &thought, cx);
                        } else {
                            s.ui_model_value = Some(raw_value.clone());
                        }
                    });
                });
            })
        });
        menu = menu.group(group.label, items);
    }
    menu
}

/// Thinking-level menu: "default" plus the current model's levels.
fn thought_menu(effective: &EffectiveConfig, root: Entity<RootView>) -> Menu {
    let current = effective.thought.clone();
    let (provider, model) = (effective.provider.clone(), effective.model.clone());
    let mut levels = vec![String::new()];
    levels.extend(effective.thought_levels.iter().cloned());
    let root = root.clone();

    let mut menu = Menu::new();
    for level in levels {
        let selected = if level.is_empty() {
            current.is_empty()
        } else {
            level == current
        };
        let label = if level.is_empty() {
            "Default".to_string()
        } else {
            title_case(&level)
        };
        let (provider, model) = (provider.clone(), model.clone());
        let lv = level.clone();
        let root = root.clone();
        menu = menu.item(MenuItem::radio(label, selected).on_click(move |_, cx| {
            root.update(cx, |root, cx| {
                root.state.update(cx, |s, cx| {
                    if !s.effective_config().has_session {
                        return;
                    }
                    let thought = lv.clone();
                    if provider.is_empty() || model.is_empty() {
                        s.switch_thought(&thought, cx);
                    } else {
                        s.switch_model(&provider, &model, &thought, cx);
                    }
                });
            });
        }));
    }
    menu
}

/// Collaboration-mode menu (build | edit | plan | yolo).
fn mode_menu(current: &str, root: Entity<RootView>) -> Menu {
    let mut menu = Menu::new();
    for (id, label) in MODES {
        let selected = current == id;
        let root = root.clone();
        menu = menu.item(MenuItem::radio(label, selected).on_click(move |_, cx| {
            root.update(cx, |root, cx| {
                root.state.update(cx, |s, cx| {
                    if s.effective_config().has_session {
                        s.switch_mode(id, cx);
                    } else {
                        s.ui_mode = Some(id.to_string());
                    }
                });
            });
        }));
    }
    menu
}
