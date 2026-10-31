use crate::app::{
    root::RootView,
    subagent_profiles::{AgentColor, AgentSource, ProfileRow, allows_all_tools},
};
use crate::shared::{
    i18n::{label, t, t_fmt},
    theme::{active_theme, ui_size},
};
use ely_gpui_component::{
    buttons::{ButtonVariant, IconButton},
    forms::Switch,
    primitives::{IconName, Tooltip},
    theme::ControlSize,
};
use gpui::{AnyElement, Context, IntoElement, SharedString, div, prelude::*, px};

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
            .gap_4()
            .min_w_0()
            .child(group_header(title, rows.len()))
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
        // Desktop renderAgentList: one surface card, rows split by border/50.
        let mut list = div()
            .flex()
            .flex_col()
            .min_w_0()
            .rounded_xl()
            .bg(theme.surface)
            .overflow_hidden();
        if rows.is_empty() {
            list = list.child(
                div()
                    .px_4()
                    .py_3()
                    .text_color(theme.subtle)
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
            let avatar = div()
                .relative()
                .flex_none()
                .size(px(36.))
                .rounded_xl()
                .bg(theme.background)
                .flex()
                .items_center()
                .justify_center()
                .child(
                    gpui::svg()
                        .path(IconName::Bot.path())
                        .size(px(16.))
                        .text_color(theme.subtle),
                )
                .children(agent.config.color.map(|color| {
                    div()
                        .absolute()
                        .bottom(px(-4.))
                        .right(px(-4.))
                        .size(px(14.))
                        .rounded_full()
                        .bg(theme.card)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(div().size(px(8.)).rounded_full().bg(agent_color(color)))
                }));
            let tools = if allows_all_tools(agent.config.tools.as_deref()) {
                t("settings.subagents.tools.all").to_owned()
            } else {
                t_fmt(
                    "settings.subagents.toolsCount",
                    &[(
                        "count",
                        &agent.config.tools.as_ref().map_or(0, Vec::len).to_string(),
                    )],
                )
            };
            let mut heading = div()
                .flex()
                .items_center()
                .flex_wrap()
                .gap_2()
                .min_w_0()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(name.to_owned()),
                );
            if agent.override_target().is_none()
                && let Some(model) = &agent.config.model_selection
            {
                heading = heading.child(chip(model.model_id.clone()).max_w(px(220.)));
            }
            heading = heading.child(chip(tools));
            let description = div()
                .id(format!("profile-description-{}", agent.id))
                .min_w_0()
                .mt(px(2.))
                .text_size(px(ui_size(12.)))
                .text_color(theme.subtle)
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
                    IconButton::new(
                        SharedString::from(format!("profile-delete-{}", agent.id)),
                        IconName::Trash2,
                    )
                    .size(ControlSize::Md)
                    .variant(ButtonVariant::Ghost)
                    .disabled(blocked)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.subagents.delete_target = Some(target.clone());
                        cx.notify();
                    })),
                );
            }
            if index != 0 {
                list = list.child(div().h(px(1.)).bg(half_border()));
            }
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .px_4()
                    .py_3()
                    .min_w_0()
                    .text_size(px(ui_size(14.)))
                    .when(editable, |el| el.hover(|s| s.bg(theme.hover)))
                    .child(avatar)
                    .child(text)
                    .child(actions),
            );
        }
        list.into_any_element()
    }
}

/// Desktop `AgentBadge`: model and tool-count chips beside the name.
fn chip(text: impl Into<SharedString>) -> gpui::Div {
    let theme = active_theme();
    div()
        .flex_none()
        .truncate()
        .rounded_md()
        .bg(theme.surface)
        .border_1()
        .border_color(theme.border)
        .px(px(6.))
        .py(px(1.))
        .text_size(px(ui_size(12.)))
        .text_color(theme.subtle)
        .child(text.into())
}

pub(super) fn half_border() -> gpui::Rgba {
    let border = active_theme().border;
    gpui::Rgba {
        a: border.a * 0.5,
        ..border
    }
}

/// Desktop `SettingsResourceGroupHeader`: medium title, subtlest item count.
pub(super) fn group_header(title: &str, count: usize) -> gpui::Div {
    let theme = active_theme();
    let key = if count == 1 {
        "settings.resourceGroup.item.one"
    } else {
        "settings.resourceGroup.item.other"
    };
    div()
        .px_1()
        .flex()
        .items_baseline()
        .gap_3()
        .text_size(px(ui_size(14.)))
        .child(
            div()
                .truncate()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(theme.foreground)
                .child(title.to_owned()),
        )
        .child(
            div()
                .flex_none()
                .text_color(theme.subtlest)
                .child(t_fmt(key, &[("count", &count.to_string())])),
        )
}

/// Desktop `formatCanonicalPluginName`, the fallback when no store listing
/// supplies a display name: title-cased slug parts with known acronyms.
pub(super) fn canonical_plugin_name(name: &str) -> String {
    name.trim()
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| match part.to_lowercase().as_str() {
            "aws" => "AWS".to_owned(),
            "mcp" => "MCP".to_owned(),
            "zcode" => "GCode".to_owned(),
            _ => {
                let mut chars = part.chars();
                chars
                    .next()
                    .map(|c| c.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn agent_color(color: AgentColor) -> gpui::Rgba {
    let t = active_theme();
    match color {
        AgentColor::Red => t.destructive,
        AgentColor::Blue => t.link,
        AgentColor::Green => t.success,
        AgentColor::Yellow | AgentColor::Orange => t.tool,
        AgentColor::Purple | AgentColor::Pink => t.reasoning,
        AgentColor::Cyan => t.info,
    }
}

#[cfg(test)]
mod tests {
    use super::canonical_plugin_name;

    #[test]
    fn plugin_names_follow_desktop_canonical_fallback() {
        assert_eq!(canonical_plugin_name("documents"), "Documents");
        assert_eq!(canonical_plugin_name("aws-mcp_tools"), "AWS MCP Tools");
        assert_eq!(canonical_plugin_name(" zcode-guide "), "GCode Guide");
    }
}
