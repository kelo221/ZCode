use crate::app::root::RootView;
use crate::shared::i18n::{LocalePreference, t};
use crate::shared::preferences::{PreferenceChange, PreferenceOwner, Preferences};
use crate::shared::theme::{ThemeMode, active_theme, font_size_base};
use ely_gpui_component::buttons::Button;
use ely_gpui_component::forms::{Choice, Select, Slider, Switch};
use ely_gpui_component::primitives::IconName;
use gpui::{AnyElement, Context, FocusHandle, IntoElement, Window, div, prelude::*, px, rgb};

gpui::actions!(settings, [OpenSettings]);

#[derive(Default, Clone, Copy, PartialEq)]
pub(crate) enum SettingsSection {
    #[default]
    General,
    Appearance,
    Shortcuts,
    Memory,
    Plugins,
    Mcp,
    Usage,
}

pub(crate) struct SettingsView {
    pub open: bool,
    pub section: SettingsSection,
    pub focus: FocusHandle,
}

impl RootView {
    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.open = true;
        self.quickpick_open = false;
        self.cancel_middle_scroll(cx);
        window.focus(&self.settings.focus, cx);
        cx.notify();
    }

    pub(crate) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.open = false;
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
        let error = preferences.error.clone();
        let mut navigation = div().w(px(190.)).flex().flex_col().gap_2().p_4();
        for (section, key, id) in [
            (
                SettingsSection::General,
                "settings.systemTitle",
                "settings-general",
            ),
            (
                SettingsSection::Appearance,
                "settings.appearanceTitle",
                "settings-appearance",
            ),
            (
                SettingsSection::Shortcuts,
                "settings.shortcuts.title",
                "settings-shortcuts",
            ),
            (SettingsSection::Memory, "", "settings-memory"),
            (
                SettingsSection::Plugins,
                "settings.pluginsTitle",
                "settings-plugins",
            ),
            (
                SettingsSection::Mcp,
                "settings.mcpServersTitle",
                "settings-mcp",
            ),
            (
                SettingsSection::Usage,
                "settings.usageStatsTitle",
                "settings-usage",
            ),
        ] {
            let name = match section {
                SettingsSection::Memory => crate::shared::i18n::label("Memory", "记忆"),
                SettingsSection::Plugins => crate::shared::i18n::label("Plugins", "插件"),
                SettingsSection::Mcp => crate::shared::i18n::label("MCP Servers", "MCP 服务器"),
                SettingsSection::Usage => crate::shared::i18n::label("Usage stats", "用量统计"),
                _ => t(key),
            };
            navigation = navigation.child(Button::new(id, name).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.select_settings_section(section, cx);
                },
            )));
        }
        #[cfg(test)]
        let navigation = crate::app::test_support::track_children(
            navigation,
            [
                "settings-general",
                "settings-appearance",
                "settings-shortcuts",
                "settings-memory",
                "settings-plugins",
                "settings-mcp",
                "settings-usage",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        );
        let content = match self.settings.section {
            SettingsSection::General => {
                let selected = settings
                    .locale_preference
                    .as_deref()
                    .or(settings.locale.as_deref())
                    .unwrap_or("system")
                    .to_owned();
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(t("settings.locale"))
                    .child(
                        Select::new(
                            "settings-locale",
                            [
                                Choice::new(
                                    "system",
                                    crate::shared::i18n::label("System", "跟随系统"),
                                ),
                                Choice::new("en-US", "English"),
                                Choice::new("zh-CN", "简体中文"),
                            ],
                        )
                        .selected(selected)
                        .disabled(saving)
                        .on_change(|value, _, cx| {
                            Preferences::enqueue(
                                PreferenceChange::Locale(LocalePreference::parse(value)),
                                cx,
                            )
                        }),
                    )
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
                        .disabled(saving)
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
                            .disabled(saving)
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
                    .disabled(saving)
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
            SettingsSection::Shortcuts => self.render_shortcut_settings(&settings, saving, cx),
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
                    .p_4()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        Button::new(
                            "settings-back",
                            crate::shared::i18n::label("Back to workspace", "返回工作区"),
                        )
                        .on_click(
                            cx.listener(|this, _, window, cx| this.close_settings(window, cx)),
                        ),
                    )
                    .child(t("quickPick.command.settings")),
            )
            .child(
                div().flex_1().min_h_0().flex().child(navigation).child(
                    div()
                        .id("settings-content")
                        .flex_1()
                        .min_w_0()
                        .overflow_y_scroll()
                        .p_6()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(content)
                        .when(saving, |el| {
                            el.child(crate::shared::i18n::label("Saving…", "正在保存…"))
                        })
                        .children(
                            error.map(|error| div().text_color(rgb(theme.danger)).child(error)),
                        )
                        .child(
                            Button::new(
                                "settings-file",
                                crate::shared::i18n::label("Open settings file", "打开设置文件"),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                let path = crate::shared::settings::settings_file_path();
                                if let Err(error) =
                                    crate::shared::os::file_launcher::open_in_editor(&path)
                                {
                                    this.state.update(cx, |state, cx| {
                                        state.push_error(error.to_string());
                                        cx.notify();
                                    });
                                }
                            })),
                        ),
                ),
            )
            .into_any_element()
    }
}
