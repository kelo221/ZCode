use crate::app::{
    root::RootView,
    subagent_profiles::{AgentColor, AgentSource, ProfileRow, allows_all_tools},
};
use crate::shared::{i18n::label, theme::active_theme};
use ely_gpui_component::{
    buttons::{Button, ButtonVariant},
    data_display::{Avatar, Badge},
    forms::Switch,
    primitives::{IconName, Tooltip},
    theme::{AvatarSize, ControlSize},
};
use gpui::{AnyElement, Context, IntoElement, div, prelude::*, px, rgb};

impl RootView {
    pub(super) fn profile_group(
        &self,
        title: &str,
        rows: Vec<ProfileRow>,
        blocked: bool,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if rows.is_empty() {
            return div().into_any_element();
        }
        div()
            .flex()
            .flex_col()
            .gap_3()
            .min_w_0()
            .child(format!("{title} ({})", rows.len()))
            .child(self.profile_rows(rows, blocked, compact, cx))
            .into_any_element()
    }

    pub(super) fn profile_rows(
        &self,
        rows: Vec<ProfileRow>,
        blocked: bool,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = active_theme();
        let mut list = div()
            .flex()
            .flex_col()
            .min_w_0()
            .rounded_lg()
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.card))
            .overflow_hidden();
        if rows.is_empty() {
            list = list.child(
                div()
                    .px_4()
                    .py_3()
                    .text_color(rgb(theme.muted))
                    .child(label("No custom agents", "暂无自定义智能体")),
            );
        }
        for (index, row) in rows.into_iter().enumerate() {
            let agent = row.agent;
            let name = if agent.source == AgentSource::Plugin {
                agent
                    .config
                    .name
                    .rsplit(':')
                    .next()
                    .unwrap_or(&agent.config.name)
            } else {
                &agent.config.name
            };
            let avatar = Avatar::new(format!("agent-avatar-{}", agent.id), name.to_owned())
                .icon(if agent.source == AgentSource::Plugin {
                    IconName::Puzzle
                } else {
                    IconName::Bot
                })
                .size(AvatarSize::Sm)
                .square();
            let avatar = if let Some(color) = agent.config.color {
                avatar.ring_color(rgb(agent_color(color)).into())
            } else {
                avatar
            };
            let tools = if allows_all_tools(agent.config.tools.as_deref()) {
                label("All tools", "全部工具").to_owned()
            } else {
                format!(
                    "{} {}",
                    agent.config.tools.as_ref().map_or(0, Vec::len),
                    label("tools", "个工具")
                )
            };
            let mut heading = div()
                .flex()
                .items_center()
                .flex_wrap()
                .gap_2()
                .min_w_0()
                .child(div().min_w_0().truncate().child(name.to_owned()))
                .child(Badge::new(tools));
            if agent.override_target().is_none()
                && let Some(model) = &agent.config.model_selection
            {
                heading = heading.child(
                    div()
                        .max_w(px(220.))
                        .truncate()
                        .child(Badge::new(model.model_id.clone())),
                );
            }
            let description = div()
                .id(format!("profile-description-{}", agent.id))
                .min_w_0()
                .text_color(rgb(theme.muted))
                .line_clamp(2)
                .tooltip(Tooltip::text(agent.config.description.clone()))
                .child(agent.config.description.clone());
            let edit = agent.clone();
            let key_edit = agent.clone();
            let editable = agent.can_edit() && !blocked;
            let mut text = div()
                .id(format!("profile-edit-{}", agent.id))
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .tab_index(0)
                .child(heading)
                .child(description)
                .when(editable, |el| {
                    el.cursor_pointer()
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_profile_form(Some(edit.clone()), false, window, cx)
                        }))
                        .on_key_down(cx.listener(
                            move |this, event: &gpui::KeyDownEvent, window, cx| {
                                if event.keystroke.key == "enter" || event.keystroke.key == "space"
                                {
                                    cx.stop_propagation();
                                    this.open_profile_form(
                                        Some(key_edit.clone()),
                                        false,
                                        window,
                                        cx,
                                    );
                                }
                            },
                        ))
                });
            let inline = agent
                .override_target()
                .map(|_| self.profile_inline_controls(&agent, blocked, cx));
            if compact && let Some(inline) = inline {
                text = text.child(inline);
            }
            let mut actions = div().flex().items_center().gap_2().flex_shrink_0();
            if !compact && agent.override_target().is_some() {
                actions = actions.child(self.profile_inline_controls(&agent, blocked, cx));
            }
            if agent.can_toggle_enabled() {
                let target = agent.clone();
                let scope = self.subagents.scope.clone();
                let weak = cx.entity().downgrade();
                actions = actions.child(
                    Switch::new(format!("profile-enabled-{}", agent.id), agent.enabled)
                        .disabled(blocked)
                        .on_change(move |enabled, _, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                if this.subagents.scope != scope {
                                    return;
                                }
                                let receipt = uuid::Uuid::now_v7().to_string();
                                this.subagents.action_receipt = Some(receipt.clone());
                                let Some(params) = target.enabled_params(enabled) else {
                                    return;
                                };
                                this.state.update(cx, |s, cx| {
                                    s.submit_profile_mutation(
                                        receipt,
                                        scope.clone(),
                                        "setEnabled",
                                        serde_json::to_value(params).unwrap(),
                                        Some(target.clone()),
                                        cx,
                                    )
                                });
                            });
                        }),
                );
            }
            if agent.can_delete() {
                let target = agent.clone();
                actions = actions.child(
                    Button::new(format!("profile-delete-{}", agent.id), "")
                        .icon(IconName::Trash2)
                        .size(ControlSize::Sm)
                        .variant(ButtonVariant::Ghost)
                        .disabled(blocked)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.subagents.delete_target = Some(target.clone());
                            cx.notify();
                        })),
                );
            }
            list = list.child(
                div()
                    .flex()
                    .items_start()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .min_w_0()
                    .when(index != 0, |el| {
                        el.border_t_1().border_color(rgb(theme.border))
                    })
                    .child(div().w(px(36.)).flex_shrink_0().child(avatar))
                    .child(text)
                    .child(actions),
            );
        }
        list.into_any_element()
    }
}

fn agent_color(color: AgentColor) -> u32 {
    let t = active_theme();
    match color {
        AgentColor::Red => t.danger,
        AgentColor::Blue => t.user_blue,
        AgentColor::Green => t.success,
        AgentColor::Yellow | AgentColor::Orange => t.tool,
        AgentColor::Purple | AgentColor::Pink => t.reasoning,
        AgentColor::Cyan => t.accent,
    }
}
