use crate::app::root::RootView;
use crate::backend::plugin_config::ConfigAction;
use crate::shared::i18n::label;
use crate::shared::plugin_config::{ConfigEdit, ConfigScope, OptionKind};
use crate::shared::theme::{MUTED, WARNING, ui_size};
use crate::shared::theme_colors::color as rgb;
use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::forms::{Input, InputEvent, Switch, TextInput};
use gpui::{AnyElement, AppContext, Context, IntoElement, ParentElement, Styled, Window, div, px};

impl RootView {
    pub(crate) fn plugin_config_button(&self, plugin: &str, cx: &mut Context<Self>) -> AnyElement {
        let owner = self.state.read(cx).active_ws_key();
        let id = plugin.to_string();
        let control = Button::new(
            format!("plugin-config-{plugin}"),
            label("Configure", "配置"),
        )
        .variant(ButtonVariant::Secondary)
        .disabled(
            owner
                .as_ref()
                .is_none_or(|k| !self.state.read(cx).plugin_operation_available(k)),
        )
        .on_click(cx.listener(move |this, _, _, cx| {
            if let Some(key) = &owner {
                this.state.update(cx, |s, cx| {
                    s.open_plugin_config(key, &id, ConfigScope::Workspace, cx)
                });
            }
        }));
        let body = div().child(control);
        #[cfg(test)]
        let body =
            crate::app::test_support::track_children(body, vec![format!("plugin-config-{plugin}")]);
        body.into_any_element()
    }
    pub(crate) fn selected_plugin_config(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let state = self.state.read(cx);
        let key = state.active_ws_key()?;
        let form = state
            .ws(&key)?
            .inspection
            .plugin_config
            .as_ref()
            .filter(|f| f.visible)?;
        let token = form.token.clone();
        let plugin = form.plugin.clone();
        let scope = form.scope;
        let baseline = form.baseline.clone();
        let edits = form.edits.clone();
        let blocked = state.plugin_config_pending(&key);
        let reload = form.needs_reload;
        let confirmation = form.confirmation;
        let error = form.error.clone();
        let dirty = baseline.as_ref().is_some_and(|b| {
            b.patch(
                &edits
                    .iter()
                    .map(|(k, (v, _))| (k.clone(), v.clone()))
                    .collect(),
            )
            .is_ok_and(|p| !p.is_empty())
        });
        // 表单收缩会让确认按钮溢出却不扩展滚动高度；保持内容高度以确保按钮可达。
        let mut body = div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap_2()
            .min_w_0()
            .text_size(px(ui_size(12.)))
            .child(format!(
                "{}: {plugin}",
                label("Plugin configuration", "插件配置")
            ));
        let mut controls = div().flex().flex_wrap().gap_1();
        for (id, text, operation, disabled) in [
            ("plugin-config-close", label("Back", "返回"), 0, false),
            (
                "plugin-config-reload",
                label("Reload scoped values", "重新读取范围配置"),
                1,
                blocked,
            ),
            (
                "plugin-config-save",
                label("Save edited options", "保存已编辑选项"),
                2,
                blocked || reload || !dirty,
            ),
        ] {
            let owner = key.clone();
            let receipt = token.clone();
            let button = Button::new(id, text)
                .variant(ButtonVariant::Secondary)
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state.update(cx, |s, cx| {
                        if !s.plugin_config_matches(&owner, &receipt) {
                            return;
                        }
                        match operation {
                            0 => s.close_plugin_config(&owner, &receipt, cx),
                            1 => s.read_plugin_config(&owner, true, cx),
                            _ => s.submit_plugin_config(&owner, &receipt, ConfigAction::Save, cx),
                        }
                    });
                }));
            let wrapper = div().child(button);
            #[cfg(test)]
            let wrapper = crate::app::test_support::track_children(wrapper, vec![id.into()]);
            controls = controls.child(wrapper);
        }
        body = body.child(controls);
        let mut scopes = div().flex().gap_1();
        for target in [ConfigScope::Workspace, ConfigScope::User] {
            let owner = key.clone();
            let receipt = token.clone();
            let plugin = plugin.clone();
            let id = format!("plugin-config-scope-{}", target.as_str());
            let button = Button::new(id.clone(), target.as_str())
                .variant(ButtonVariant::Secondary)
                .disabled(blocked || target == scope)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state.update(cx, |s, cx| {
                        if s.plugin_config_matches(&owner, &receipt) {
                            s.open_plugin_config(&owner, &plugin, target, cx);
                        }
                    });
                }));
            let wrapper = div().child(button);
            #[cfg(test)]
            let wrapper = crate::app::test_support::track_children(wrapper, vec![id]);
            scopes = scopes.child(wrapper);
        }
        body = body.child(scopes).child(label("Only edited fields become overrides. Blank secret means unchanged; Clear removes an override.","仅已编辑字段写入覆盖值。密钥留空表示保持不变；清除移除覆盖值。"));
        if let Some(error) = error {
            body = body.child(div().text_color(rgb(WARNING)).child(error));
        }
        let mut fields = div().flex().flex_col().flex_shrink_0().gap_2().min_w_0();
        if let Some(baseline) = baseline {
            for (option, declaration) in baseline.user_config.as_ref().into_iter().flatten() {
                let sensitive = declaration.sensitive == Some(true);
                let value = edits.get(option).map(|(v, _)| v.clone());
                let source = baseline.option_sources.as_ref().and_then(|m| m.get(option));
                let title = declaration.title.clone().unwrap_or_else(|| option.clone());
                let mut field = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .min_w_0()
                    .child(format!(
                        "{title} ({}){}",
                        source.map(|s| s.as_str()).unwrap_or("default"),
                        if declaration.required == Some(true) {
                            " · required"
                        } else {
                            ""
                        }
                    ))
                    .children(declaration.description.clone());
                if value == Some(ConfigEdit::Clear) {
                    field = field.child(label("Override will be cleared", "将清除覆盖值"));
                }
                if declaration.kind == Some(OptionKind::Boolean) && !sensitive {
                    let on = match value {
                        Some(ConfigEdit::Bool(v)) => v,
                        _ => baseline.effective(option).as_bool().unwrap_or(false),
                    };
                    let owner = key.clone();
                    let receipt = token.clone();
                    let option = option.clone();
                    let state = self.state.clone();
                    field = field.child(
                        Switch::new(format!("plugin-option-{option}"), on).on_change(
                            move |on, _, cx| {
                                state.update(cx, |s, cx| {
                                    s.edit_plugin_config(
                                        &owner,
                                        &receipt,
                                        &option,
                                        ConfigEdit::Bool(on),
                                        cx,
                                    )
                                });
                            },
                        ),
                    );
                } else {
                    let existing = self
                        .state
                        .read(cx)
                        .ws(&key)?
                        .inspection
                        .plugin_config
                        .as_ref()?
                        .inputs
                        .get(option)
                        .cloned();
                    let input = existing.unwrap_or_else(|| {
                        let initial = match &value {
                            Some(ConfigEdit::Text(v)) => v.clone(),
                            Some(ConfigEdit::Clear) => String::new(),
                            _ => {
                                let effective = baseline.effective(option);
                                effective
                                    .as_str()
                                    .map(str::to_string)
                                    .unwrap_or_else(|| effective.to_string())
                            }
                        };
                        let input = cx.new(|cx| {
                            let mut input = TextInput::new(window, cx);
                            if sensitive {
                                input = input.masked();
                            }
                            input.set_text(initial, cx);
                            input
                        });
                        let owner = key.clone();
                        let receipt = token.clone();
                        let option_key = option.clone();
                        cx.subscribe(&input, move |this, input, event: &InputEvent, cx| {
                            if *event != InputEvent::Changed {
                                return;
                            }
                            let matches = this
                                .state
                                .read(cx)
                                .ws(&owner)
                                .and_then(|w| w.inspection.plugin_config.as_ref())
                                .and_then(|f| f.inputs.get(&option_key))
                                .is_some_and(|current| current == &input);
                            if matches {
                                let text = input.read(cx).text().to_string();
                                this.state.update(cx, |s, cx| {
                                    s.edit_plugin_config(
                                        &owner,
                                        &receipt,
                                        &option_key,
                                        ConfigEdit::Text(text),
                                        cx,
                                    )
                                });
                            }
                        })
                        .detach();
                        self.state.update(cx, |s, _| {
                            s.ws_mut(&key)
                                .unwrap()
                                .inspection
                                .plugin_config
                                .as_mut()
                                .unwrap()
                                .inputs
                                .insert(option.clone(), input.clone());
                        });
                        input
                    });
                    let wrapper = div().w_full().child(Input::new(&input));
                    #[cfg(test)]
                    let wrapper = crate::app::test_support::track_children(
                        wrapper,
                        vec![format!("plugin-option-{option}")],
                    );
                    field = field.child(wrapper);
                }
                if source == Some(&scope) {
                    let owner = key.clone();
                    let receipt = token.clone();
                    let option_key = option.clone();
                    let clear = Button::new(
                        format!("plugin-option-clear-{option}"),
                        label("Clear override", "清除覆盖值"),
                    )
                    .variant(ButtonVariant::Secondary)
                    .disabled(blocked)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |s, cx| {
                            s.edit_plugin_config(
                                &owner,
                                &receipt,
                                &option_key,
                                ConfigEdit::Clear,
                                cx,
                            )
                        });
                    }));
                    let wrapper = div().child(clear);
                    #[cfg(test)]
                    let wrapper = crate::app::test_support::track_children(
                        wrapper,
                        vec![format!("plugin-option-clear-{option}")],
                    );
                    field = field.child(wrapper);
                }
                fields = fields.child(field);
            }
        } else {
            fields = fields.child(label("Loading scoped configuration…", "正在加载范围配置…"));
        }
        body = body.child(div().text_color(rgb(MUTED)).child(if scope == ConfigScope::Workspace {
            label("Reset restores inherited enablement only. Option and secret overrides remain.","重置仅恢复继承的启用状态。选项和密钥覆盖值保留。")
        } else { label("Reset removes user enablement, options and stored secrets. Workspace overrides remain.","重置移除用户启用配置、选项和已保存密钥。工作区覆盖值保留。") }));
        let owner = key.clone();
        let receipt = token.clone();
        let reset = Button::new(
            "plugin-config-reset",
            if scope == ConfigScope::Workspace {
                label("Restore inherited enablement", "恢复继承的启用状态")
            } else {
                label("Remove user configuration", "移除用户配置")
            },
        )
        .variant(ButtonVariant::Secondary)
        .disabled(blocked || reload)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.state.update(cx, |s, cx| {
                s.confirm_plugin_reset(&owner, &receipt, true, cx)
            })
        }));
        let wrapper = div().child(reset);
        #[cfg(test)]
        let wrapper =
            crate::app::test_support::track_children(wrapper, vec!["plugin-config-reset".into()]);
        body = body.child(wrapper);
        if confirmation {
            for (id, text, confirm) in [
                (
                    "plugin-config-confirm",
                    label("Confirm reset", "确认重置"),
                    true,
                ),
                ("plugin-config-cancel", label("Cancel", "取消"), false),
            ] {
                let owner = key.clone();
                let receipt = token.clone();
                let button = Button::new(id, text)
                    .variant(ButtonVariant::Secondary)
                    .disabled(blocked)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |s, cx| {
                            if confirm {
                                s.submit_plugin_config(&owner, &receipt, ConfigAction::Reset, cx);
                            } else {
                                s.confirm_plugin_reset(&owner, &receipt, false, cx);
                            }
                        })
                    }));
                let wrapper = div().child(button);
                #[cfg(test)]
                let wrapper = crate::app::test_support::track_children(wrapper, vec![id.into()]);
                body = body.child(wrapper);
            }
        }
        Some(body.child(fields).into_any_element())
    }
}
