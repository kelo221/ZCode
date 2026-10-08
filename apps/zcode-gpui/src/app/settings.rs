use crate::app::root::RootView;
use crate::app::settings_navigation::settings_layout;
use crate::shared::i18n::{LocalePreference, t};
use crate::shared::preferences::{PreferenceChange, PreferenceOwner, Preferences};
use crate::shared::theme::{ThemeMode, active_theme, font_size_base};
use ely_gpui_component::buttons::Button;
use ely_gpui_component::forms::{Choice, Select, Slider, Switch};
use ely_gpui_component::primitives::IconName;
use gpui::{AnyElement, Context, FocusHandle, IntoElement, Window, div, prelude::*, px, rgb};

gpui::actions!(settings, [OpenSettings]);

pub(crate) fn language_choices() -> [Choice; 3] {
    use crate::shared::i18n::label;
    [
        Choice::new("system", label("System", "跟随系统")),
        Choice::new("en-US", label("English", "英语")),
        Choice::new("zh-CN", label("Chinese (Simplified)", "简体中文")),
    ]
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsSection {
    #[default]
    General,
    Appearance,
    Shortcuts,
    Memory,
    Models,
    Subagents,
    Plugins,
    Mcp,
    Usage,
}

impl SettingsSection {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "general" => Self::General,
            "appearance" => Self::Appearance,
            "shortcuts" => Self::Shortcuts,
            "memory" => Self::Memory,
            "models" => Self::Models,
            "subagents" => Self::Subagents,
            "plugins" => Self::Plugins,
            "mcp" => Self::Mcp,
            "usage" => Self::Usage,
            _ => return None,
        })
    }
}

pub(crate) struct SettingsView {
    pub open: bool,
    pub section: SettingsSection,
    pub query: String,
    pub shortcut_query: String,
    pub model_query: String,
    pub focus: FocusHandle,
}

impl RootView {
    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_section(self.settings.section, window, cx);
    }

    pub(crate) fn open_settings_section(
        &mut self,
        section: SettingsSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_settings_section(section, cx);
        self.settings.open = true;
        self.quickpick_open = false;
        self.cancel_middle_scroll(cx);
        window.focus(&self.settings.focus, cx);
        cx.notify();
    }

    pub(crate) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.open = false;
        self.subagents.form = None;
        self.state.update(cx, |s, cx| s.close_profiles(cx));
        let focus = self.state.read(cx).composer.read(cx).focus.clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(crate) fn settings_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let footer = div().child(
            Button::new("open-settings", t("quickPick.command.settings"))
                .icon(IconName::Settings)
                .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx))),
        );
        #[cfg(test)]
        let footer = crate::app::test_support::track_children(footer, vec!["open-settings".into()]);
        footer.into_any_element()
    }

    pub(crate) fn render_settings(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = active_theme();
        let owner = cx.global::<PreferenceOwner>().0.clone();
        let preferences = owner.read(cx);
        let settings = preferences.snapshot.clone();
        let saving = preferences.saving;
        let read_only = preferences.read_only;
        let suspended = preferences.suspended;
        let blocked = saving || read_only || suspended;
        let error = preferences.error.clone();
        let layout = settings_layout(f32::from(window.viewport_size().width));
        let back = div().child(
            Button::new(
                "settings-back",
                crate::shared::i18n::label("Back to workspace", "返回工作区"),
            )
            .icon(IconName::ArrowLeft)
            .on_click(cx.listener(|this, _, window, cx| this.close_settings(window, cx))),
        );
        #[cfg(test)]
        let back = crate::app::test_support::track_children(back, vec!["settings-back".into()]);
        let navigation = div()
            .id("settings-navigation")
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_4()
            .p_4()
            .when(layout.compact, |el| {
                el.w_full().h(px(layout.navigation_height)).flex_shrink_0()
            })
            .when(!layout.compact, |el| {
                el.w(px(264.)).h_full().flex_shrink_0()
            })
            .child(back)
            .child(self.render_settings_navigation(window, cx));
        let content = match self.settings.section {
            SettingsSection::General => {
                let selected = settings.language_preference().as_str();
                let selector = div().child(
                    Select::new("settings-locale", language_choices())
                        .selected(selected)
                        .disabled(blocked)
                        .on_change(|value, _, cx| {
                            Preferences::enqueue(
                                PreferenceChange::Locale(LocalePreference::parse(value)),
                                cx,
                            )
                        }),
                );
                #[cfg(test)]
                let selector = crate::app::test_support::track_children(
                    selector,
                    vec!["settings-locale".into()],
                );
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(t("settings.locale"))
                    .child(selector)
                    .child(
                        div()
                            .text_color(rgb(theme.muted))
                            .child(crate::shared::i18n::label(
                                "Other settings remain available in ZCode desktop.",
                                "其他设置仍可在 ZCode 桌面端中使用。",
                            )),
                    )
                    .into_any_element()
            }
            SettingsSection::Appearance => {
                let selected = settings
                    .theme_preference
                    .as_deref()
                    .or(settings.theme.as_deref())
                    .unwrap_or("system")
                    .to_owned();
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(t("settings.themeMode"))
                    .child(
                        Select::new(
                            "settings-theme",
                            [
                                Choice::new(
                                    "system",
                                    crate::shared::i18n::label("System", "跟随系统"),
                                ),
                                Choice::new("zai-dark", crate::shared::i18n::label("Dark", "深色")),
                                Choice::new(
                                    "zai-light",
                                    crate::shared::i18n::label("Light", "浅色"),
                                ),
                            ],
                        )
                        .selected(selected)
                        .disabled(blocked)
                        .on_change(|value, _, cx| {
                            Preferences::enqueue(
                                PreferenceChange::Theme(ThemeMode::parse(value)),
                                cx,
                            )
                        }),
                    )
                    .child(format!(
                        "{}: {} px",
                        crate::shared::i18n::label("Interface font size", "界面字体大小"),
                        font_size_base()
                    ))
                    .child(
                        Slider::new("settings-font-size", f64::from(font_size_base()))
                            .range(12., 20.)
                            .step(1.)
                            .disabled(blocked)
                            .on_change(|value, _, cx| {
                                Preferences::enqueue(PreferenceChange::FontSize(value as f32), cx)
                            }),
                    )
                    .into_any_element()
            }
            SettingsSection::Memory => {
                let content = div().flex().flex_col().gap_4().child(
                    Switch::new(
                        "settings-memory-enabled",
                        settings.memory_enabled.unwrap_or(false),
                    )
                    .label(crate::shared::i18n::label("Enable Memory", "启用记忆"))
                    .disabled(blocked)
                    .on_change(|enabled, _, cx| {
                        Preferences::enqueue(PreferenceChange::MemoryEnabled(enabled), cx)
                    }),
                );
                #[cfg(test)]
                let content = crate::app::test_support::track_children(
                    content,
                    vec!["settings-memory-enabled".into()],
                );
                content.child(div().text_color(rgb(theme.muted)).child(
                    crate::shared::i18n::label(
                        "Allows configured CLI memory for newly materialized or resumed sessions. This does not reconfigure an active runtime; CLI memory policy still applies.",
                        "允许新建或恢复的会话使用已配置的 CLI 记忆。不会重新配置运行中的会话；仍受 CLI 记忆策略控制。",
                    ),
                )).into_any_element()
            }
            SettingsSection::Shortcuts => {
                self.render_shortcut_settings(&settings, blocked, window, cx)
            }
            SettingsSection::Models => self.render_model_settings(window, cx),
            SettingsSection::Subagents => self.render_subagents_settings(window, cx),
            SettingsSection::Plugins => self.plugins_pane(window, cx),
            SettingsSection::Mcp => self.mcp_pane(cx),
            SettingsSection::Usage => self.usage_pane(cx),
        };
        div()
            .id("settings-page")
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .track_focus(&self.settings.focus)
            .bg(rgb(theme.panel))
            .text_color(rgb(theme.text))
            .text_size(px(font_size_base()))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .when(layout.compact, |el| el.flex_col())
                    .child(navigation)
                    .child(
                        div()
                            .id("settings-content")
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .overflow_y_scroll()
                            .when(layout.compact, |el| el.p_4())
                            .when(!layout.compact, |el| el.p_6())
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(self.settings.section.title())
                            .child(content)
                            .when(read_only, |el| {
                                el.child(crate::shared::i18n::label(
                            "Application preferences are read-only in isolated Settings mode.",
                            "隔离设置模式下应用偏好设置为只读。",
                        ))
                            })
                            .when(saving, |el| {
                                el.child(crate::shared::i18n::label("Saving…", "正在保存…"))
                            })
                            .children(
                                error.map(|error| div().text_color(rgb(theme.danger)).child(error)),
                            )
                            .when(suspended && self.settings.section != SettingsSection::Subagents, |el| {
                                el.child(crate::shared::i18n::label("Preference saves are paused until the Subagents Host exits.", "子智能体 Host 退出前，偏好保存暂停。"))
                                    .child(Button::new("settings-close-manager", crate::shared::i18n::label("Close manager", "关闭管理"))
                                        .on_click(cx.listener(|this, _, _, cx| this.state.update(cx, |s, cx| s.close_profiles(cx)))))
                            })
                            .when(self.settings.section != SettingsSection::Subagents, |el| el.child(
                                Button::new(
                                    "settings-file",
                                    crate::shared::i18n::label(
                                        "Open settings file",
                                        "打开设置文件",
                                    ),
                                )
                                .disabled(blocked)
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        let path = crate::shared::settings::settings_file_path();
                                        if let Err(error) =
                                            crate::shared::os::file_launcher::open_in_editor(&path)
                                        {
                                            this.state.update(cx, |state, cx| {
                                                state.push_error(error.to_string());
                                                cx.notify();
                                            });
                                        }
                                    },
                                )),
                            )),
                    ),
            )
            .into_any_element()
    }
}
