use crate::app::{root::RootView, settings::SettingsSection};
use crate::shared::i18n::label;
use crate::shared::theme::{active_theme, font_size_sm};
use ely_gpui_component::buttons::{Button, ButtonVariant};
use ely_gpui_component::forms::{Input, InputEvent, TextInput};
use gpui::{AnyElement, Context, IntoElement, Window, div, prelude::*, px, rgb};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SettingsGroup {
    Preferences,
    Capabilities,
    Activity,
}

impl SettingsGroup {
    fn title(self) -> &'static str {
        match self {
            Self::Preferences => label("Preferences", "偏好设置"),
            Self::Capabilities => label("Capabilities", "能力"),
            Self::Activity => label("Activity", "活动"),
        }
    }
}

impl SettingsSection {
    pub(crate) fn title(self) -> &'static str {
        let (en, zh) = self.labels();
        label(en, zh)
    }

    fn labels(self) -> (&'static str, &'static str) {
        match self {
            Self::General => ("General", "常规"),
            Self::Appearance => ("Appearance", "外观"),
            Self::Shortcuts => ("Shortcuts", "快捷键"),
            Self::Memory => ("Memory", "记忆"),
            Self::Models => ("Configured models", "已配置模型"),
            Self::Subagents => ("Subagents", "子代理"),
            Self::Plugins => ("Plugins", "插件"),
            Self::Mcp => ("MCP Servers", "MCP 服务器"),
            Self::Usage => ("Usage stats", "用量统计"),
        }
    }

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::General => "settings-general",
            Self::Appearance => "settings-appearance",
            Self::Shortcuts => "settings-shortcuts",
            Self::Memory => "settings-memory",
            Self::Models => "settings-models",
            Self::Subagents => "settings-subagents",
            Self::Plugins => "settings-plugins",
            Self::Mcp => "settings-mcp",
            Self::Usage => "settings-usage",
        }
    }
}

pub(crate) fn search_matches(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    query
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}

pub(crate) fn navigation_sections(query: &str) -> Vec<(SettingsGroup, SettingsSection)> {
    use SettingsGroup::*;
    use SettingsSection::*;
    [
        (Preferences, General),
        (Preferences, Appearance),
        (Preferences, Shortcuts),
        (Capabilities, Memory),
        (Capabilities, Models),
        (Capabilities, Subagents),
        (Capabilities, Plugins),
        (Capabilities, Mcp),
        (Activity, Usage),
    ]
    .into_iter()
    .filter(|(_, section)| {
        let (en, zh) = section.labels();
        search_matches(query, &format!("{en} {zh}"))
    })
    .collect()
}

pub(crate) struct SettingsLayout {
    pub compact: bool,
    pub navigation_height: f32,
}

pub(crate) fn settings_layout(width: f32) -> SettingsLayout {
    SettingsLayout {
        compact: width < 900.,
        navigation_height: 160.,
    }
}

#[derive(Clone, Copy)]
pub(crate) enum SettingsSearch {
    Navigation,
    Shortcuts,
    Models,
}

impl SettingsSearch {
    fn id(self) -> &'static str {
        match self {
            Self::Navigation => "settings-search",
            Self::Shortcuts => "settings-shortcut-search",
            Self::Models => "settings-model-search",
        }
    }
    fn placeholder(self) -> &'static str {
        match self {
            Self::Navigation => label("Search settings", "搜索设置"),
            Self::Shortcuts => label("Search shortcuts", "搜索快捷键"),
            Self::Models => label(
                "Search provider, model or reasoning",
                "搜索提供商、模型或推理级别",
            ),
        }
    }
    fn query(self, view: &RootView) -> &str {
        match self {
            Self::Navigation => &view.settings.query,
            Self::Shortcuts => &view.settings.shortcut_query,
            Self::Models => &view.settings.model_query,
        }
    }
    fn set(self, view: &mut RootView, text: String) {
        match self {
            Self::Navigation => view.settings.query = text,
            Self::Shortcuts => view.settings.shortcut_query = text,
            Self::Models => view.settings.model_query = text,
        }
    }
}

impl RootView {
    pub(crate) fn settings_search(
        &mut self,
        field: SettingsSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = field.id();
        let text = field.query(self).to_owned();
        // Ely entity 仅投影 RootView 的搜索草稿；重新进入 section 时从同一 owner 恢复。
        let input = window.use_keyed_state(id, cx, |window, cx| {
            let mut input = TextInput::new(window, cx).placeholder(field.placeholder());
            input.set_text(text, cx);
            input
        });
        // keyed 输入会跨语言切换保留；只更新提示投影，不能重建 entity 丢失搜索草稿或焦点。
        if input.read(cx).placeholder_text().as_ref() != field.placeholder() {
            input.update(cx, |input, cx| {
                input.set_placeholder(field.placeholder(), cx)
            });
        }
        #[cfg(test)]
        if cx.has_global::<crate::app::test_support::TestLabels>() {
            let placeholder = input.read(cx).placeholder_text().to_string();
            cx.global_mut::<crate::app::test_support::TestLabels>()
                .0
                .insert(id.into(), placeholder);
        }
        let view = cx.entity().downgrade();
        window.use_keyed_state(format!("{id}-subscription"), cx, |_, cx| {
            cx.subscribe(&input, move |_, input, event: &InputEvent, cx| {
                if *event == InputEvent::Changed {
                    let text = input.read(cx).text().to_owned();
                    let _ = view.update(cx, |view, cx| {
                        field.set(view, text);
                        cx.notify();
                    });
                }
            })
        });
        let wrapper = div().w_full().min_w_0().child(Input::new(&input));
        #[cfg(test)]
        let wrapper = crate::app::test_support::track_children(wrapper, vec![id.into()]);
        wrapper.into_any_element()
    }

    pub(crate) fn render_settings_navigation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let search = self.settings_search(SettingsSearch::Navigation, window, cx);
        let mut groups = div().flex().flex_col().gap_4();
        let sections = navigation_sections(&self.settings.query);
        for group in [
            SettingsGroup::Preferences,
            SettingsGroup::Capabilities,
            SettingsGroup::Activity,
        ] {
            let entries: Vec<_> = sections
                .iter()
                .filter(|(g, _)| *g == group)
                .map(|(_, s)| *s)
                .collect();
            if entries.is_empty() {
                continue;
            }
            let mut buttons = div().flex().flex_col().gap_1();
            for section in &entries {
                let section = *section;
                buttons = buttons.child(
                    Button::new(section.id(), section.title())
                        .full_width()
                        .variant(if self.settings.section == section {
                            ButtonVariant::Subtle
                        } else {
                            ButtonVariant::Ghost
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_settings_section(section, cx)
                        })),
                );
            }
            #[cfg(test)]
            let buttons = crate::app::test_support::track_children(
                buttons,
                entries.iter().map(|s| s.id().to_owned()).collect(),
            );
            groups = groups.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_color(rgb(active_theme().muted))
                            .text_size(px(font_size_sm()))
                            .child(group.title()),
                    )
                    .child(buttons),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(search)
            .child(groups)
            .when(sections.is_empty(), |el| {
                el.child(label("No matching settings", "没有匹配的设置"))
            })
            .into_any_element()
    }
}
