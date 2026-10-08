use crate::app::root::RootView;
use crate::shared::{i18n::label, theme::active_theme};
use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::primitives::{Icon, IconName};
use gpui::{AnyElement, Context, IntoElement, div, prelude::*, rgb};

impl RootView {
    pub(super) fn profile_activation(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = &self.state.read(cx).profiles;
        let busy = p.activating || p.closing || p.stopping;
        let theme = active_theme();
        let mut body = div()
            .flex()
            .flex_col()
            .gap_4()
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(Icon::new(IconName::Bot))
                    .child(label("Manage local agents", "管理本地智能体")),
            )
            .child(div().text_color(rgb(theme.muted)).child(label(
                "Use the existing Services Host to manage your local user and workspace profiles.",
                "使用现有服务 Host 管理本地用户和工作区配置。",
            )));
        if self.subagents.activation_confirm {
            body = body.child(div().flex().flex_col().gap_2().rounded_lg().border_1().border_color(rgb(theme.border)).p_4()
                .child(label("Before enabling", "启用前须知"))
                .child(label("The full Host adds roughly 190–194 MiB RSS while active. Startup and listing may migrate or normalize existing storage.", "完整 Host 活跃时约增加 190–194 MiB 内存。启动和列举可能迁移或规范化现有存储。"))
                .child(label("Edits rewrite supported metadata; unknown YAML fields and comments may be discarded. Desktop and other configuration writers must be closed.", "编辑会重写受支持的元数据；未知 YAML 字段和注释可能丢失。请关闭桌面端及其他配置写入客户端。"))
                .child(label("Application preference saves pause until this owned Host exits. Concurrent clients are unsupported; process detection is best-effort, not a lock.", "应用偏好保存会暂停，直至本 Host 完全退出。不支持并发客户端；进程检测并非互斥锁。")));
        }
        let mut controls = div().flex().gap_2().flex_wrap();
        if self.subagents.activation_confirm {
            controls = controls
                .child(
                    Button::new(
                        "subagents-activate-confirm",
                        label("Enable local management", "启用本地管理"),
                    )
                    .primary()
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.subagents.activation_confirm = false;
                        this.state.update(cx, |s, cx| s.activate_profiles(cx));
                    })),
                )
                .child(
                    Button::new("subagents-activate-cancel", label("Cancel", "取消"))
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.subagents.activation_confirm = false;
                            cx.notify();
                        })),
                );
        } else {
            controls = controls.child(
                Button::new(
                    "subagents-activate",
                    label("Enable local management", "启用本地管理"),
                )
                .disabled(busy)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.subagents.activation_confirm = true;
                    cx.notify();
                })),
            );
        }
        if busy {
            body = body.child(label(
                "Finishing preference handoff…",
                "正在完成偏好写入交接…",
            ));
        }
        if p.closing && !p.stopping && !p.activating && p.connection_error.is_some() {
            body =
                body.child(
                    Button::new(
                        "subagents-retry-cleanup",
                        label("Retry cleanup and reload", "重试清理并加载"),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.update(cx, |s, cx| s.close_profiles(cx))
                    })),
                );
        }
        if let Some(e) = &p.connection_error {
            body = body.child(div().text_color(rgb(theme.danger)).child(e.clone()));
        }
        #[cfg(test)]
        let controls = crate::app::test_support::track_children(
            controls,
            if self.subagents.activation_confirm {
                vec![
                    "subagents-activate-confirm".into(),
                    "subagents-activate-cancel".into(),
                ]
            } else {
                vec!["subagents-activate".into()]
            },
        );
        body.child(controls).into_any_element()
    }

    pub(super) fn profile_review(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = &self.state.read(cx).profiles;
        let message = if p.recovery_reviewed && p.recovery_observed {
            label(
                "Intended state observed in refreshed inventory. This is not proof the original RPC succeeded.",
                "已在新列表中观察到预期状态。这不证明原 RPC 成功。",
            )
        } else if p.recovery_reviewed {
            label(
                "Refreshed inventory does not uniquely match the intended result. Writes remain blocked; inspect the configuration before another review.",
                "新列表与预期结果不唯一匹配。写入仍被阻止；再次检查前请核对配置。",
            )
        } else {
            label(
                "A write outcome is uncertain. Reload and review without replaying the action.",
                "写入结果不确定。重新加载并检查，不会重放操作。",
            )
        };
        let busy = p.mutation_pending || p.recovery_reviewing || p.closing || p.stopping;
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(rgb(active_theme().warning))
            .child(message)
            .child(
                div()
                    .flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new(
                            "subagents-review",
                            label("Reload and review outcome", "重新加载并检查结果"),
                        )
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state.update(cx, |s, cx| s.review_profiles(cx))
                        })),
                    )
                    .child(
                        Button::new(
                            "subagents-acknowledge",
                            label("Acknowledge observed outcome", "确认观察结果"),
                        )
                        .disabled(busy || !p.recovery_reviewed || !p.recovery_observed)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state.update(cx, |s, cx| {
                                if s.profiles.recovery_reviewed
                                    && s.profiles.recovery_observed
                                    && !s.profiles.recovery_reviewing
                                {
                                    s.profiles.uncertain = false;
                                    s.profiles.recovery = None;
                                    cx.notify();
                                }
                            });
                            this.subagents.form = None;
                            this.subagents.action_receipt = None;
                        })),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn profile_delete_confirmation(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = self.subagents.delete_target.as_ref().unwrap().clone();
        let scope = self.subagents.scope.clone();
        let blocked =
            self.state.read(cx).profiles.mutation_pending || self.state.read(cx).profiles.uncertain;
        div()
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(rgb(active_theme().border))
            .flex()
            .flex_col()
            .gap_3()
            .child(format!(
                "{} {}?",
                label("Delete", "删除"),
                target.config.name
            ))
            .child(label(
                "This removes the profile file. It cannot be undone here.",
                "将删除配置文件。此处无法撤销。",
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new(
                            "subagents-delete-confirm",
                            label("Delete agent", "删除智能体"),
                        )
                        .variant(ButtonVariant::Danger)
                        .disabled(blocked)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.subagents.scope != scope {
                                return;
                            }
                            let Some(params) = target.delete_params() else {
                                return;
                            };
                            let receipt = uuid::Uuid::now_v7().to_string();
                            this.subagents.action_receipt = Some(receipt.clone());
                            this.state.update(cx, |s, cx| {
                                s.submit_profile_mutation(
                                    receipt,
                                    scope.clone(),
                                    "deleteAgent",
                                    serde_json::to_value(params).unwrap(),
                                    Some(target.clone()),
                                    cx,
                                )
                            });
                            this.subagents.delete_target = None;
                            cx.notify();
                        })),
                    )
                    .child(
                        Button::new("subagents-delete-cancel", label("Cancel", "取消")).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.subagents.delete_target = None;
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .into_any_element()
    }
}
