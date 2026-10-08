use crate::app::{
    root::RootView,
    settings_navigation::{SettingsSearch, search_matches},
};
use crate::composer::catalog::{ModelGroup, ModelOption, group_models};
use crate::shared::i18n::label;
use crate::shared::theme::{MONO_FONT, active_theme, font_size_sm};
use gpui::{AnyElement, Context, IntoElement, Window, div, prelude::*, px, rgb};

pub(crate) fn filtered_model_groups(models: &[ModelOption], query: &str) -> Vec<ModelGroup> {
    // 复用既有 catalog 分组，先分组再过滤以保持提供商标签和 owner 顺序稳定。
    group_models(models)
        .into_iter()
        .filter_map(|mut group| {
            group.items.retain(|model| {
                search_matches(
                    query,
                    &format!(
                        "{} {} {} {} {} {} {}",
                        group.label,
                        model.provider,
                        model.provider_name,
                        model.name,
                        model.model,
                        model.thought_levels.join(" "),
                        model.default_thought,
                    ),
                )
            });
            (!group.items.is_empty()).then_some(group)
        })
        .collect()
}

impl RootView {
    pub(crate) fn render_model_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state.read(cx);
        let config = state
            .active_ws_key()
            .and_then(|key| state.workspace_configs.get(&key));
        let available = config.is_some();
        let groups = filtered_model_groups(
            config.map_or(&[], |config| config.models.as_slice()),
            &self.settings.model_query,
        );
        let empty = config.is_none_or(|config| config.models.is_empty());
        let no_matches = groups.is_empty();
        let theme = active_theme();
        let mut list = div().flex().flex_col().gap_4();
        for group in groups {
            let mut items = div().flex().flex_col().gap_3();
            for model in group.items {
                let row = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(model.name)
                    .child(
                        div()
                            .font_family(MONO_FONT)
                            .text_size(px(font_size_sm()))
                            .text_color(rgb(theme.muted))
                            .child(format!("{}/{}", model.provider, model.model)),
                    )
                    .child(
                        div()
                            .text_size(px(font_size_sm()))
                            .text_color(rgb(theme.muted))
                            .child(format!(
                                "{}: {} · {}: {}",
                                label("Reasoning levels", "推理级别"),
                                if model.thought_levels.is_empty() {
                                    label("Not provided", "未提供").to_owned()
                                } else {
                                    model.thought_levels.join(", ")
                                },
                                label("Default", "默认"),
                                if model.default_thought.is_empty() {
                                    label("Not provided", "未提供")
                                } else {
                                    &model.default_thought
                                },
                            )),
                    );
                items = items.child(row);
            }
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_color(rgb(theme.muted)).child(group.label))
                    .child(items),
            );
        }
        let search = self.settings_search(SettingsSearch::Models, window, cx);
        div().flex().flex_col().gap_4()
            .child(div().text_color(rgb(theme.muted)).text_size(px(font_size_sm())).child(label(
                "Read-only workspace model catalog. This does not change providers, the selected session model or Subagents defaults.",
                "工作区模型目录仅供查看，不会修改提供商、会话所选模型或子代理默认设置。",
            )))
            .child(search)
            .when(!available, |el| el.child(label("Workspace model catalog is unavailable", "工作区模型目录不可用")))
            .when(available && empty, |el| el.child(label("No configured models in this workspace catalog", "此工作区目录中没有已配置模型")))
            .when(available && !empty && no_matches, |el| el.child(label("No matching models", "没有匹配的模型")))
            .child(list).into_any_element()
    }
}
