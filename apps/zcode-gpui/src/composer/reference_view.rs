use crate::app::root::RootView;
use crate::composer::{
    autocomplete::AutocompleteSuggestion,
    references::{CatalogState, reference_query},
};
use ely_gpui_component::buttons::Button;
use gpui::{AnyElement, Context, ParentElement, Styled, div, prelude::*};

impl RootView {
    pub(crate) fn reference_suggestions(
        &self,
        text: &str,
        cx: &Context<Self>,
    ) -> Vec<AutocompleteSuggestion> {
        let Some((kind, query, _)) = reference_query(text) else {
            return vec![];
        };
        let state = self.state.read(cx);
        let Some(ws) = state.active_ws_key().and_then(|key| state.ws(&key)) else {
            return vec![];
        };
        let Some(CatalogState::Ready(entries)) = ws
            .reference_catalogs
            .get(&state.active)
            .map(|catalog| catalog.state(kind))
        else {
            return vec![];
        };
        let query = query.to_lowercase();
        entries
            .iter()
            .filter(|entry| {
                entry.name.to_lowercase().contains(&query)
                    || entry.description.to_lowercase().contains(&query)
            })
            .take(8)
            .map(|entry| AutocompleteSuggestion::Mention {
                category: entry.category,
                label: entry.name.clone(),
                description: entry.description.clone(),
                insert_text: entry.insert_text.clone(),
            })
            .collect()
    }

    pub(crate) fn reference_feedback(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.state.read(cx);
        let (kind, _, _) = reference_query(state.composer.read(cx).text())?;
        let ws = state.active_ws_key().and_then(|key| state.ws(&key));
        let catalog = ws
            .and_then(|ws| ws.reference_catalogs.get(&state.active))
            .map(|catalog| catalog.state(kind));
        let message = match catalog {
            Some(CatalogState::Loading) => {
                crate::shared::i18n::label("Loading references…", "正在加载引用…").to_string()
            }
            Some(CatalogState::Failed(error)) => error.clone(),
            Some(CatalogState::Ready(entries)) if entries.is_empty() => {
                crate::shared::i18n::label("No available references", "没有可用引用").to_string()
            }
            Some(CatalogState::Ready(_)) => {
                crate::shared::i18n::label("Available references", "可用引用").to_string()
            }
            _ => crate::shared::i18n::label(
                "Connect a workspace to load references",
                "连接工作区以加载引用",
            )
            .to_string(),
        };
        let loading = matches!(catalog, Some(CatalogState::Loading));
        Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(gpui::px(crate::shared::theme::font_size_sm()))
                .child(div().flex_1().child(message))
                .child(
                    Button::new(
                        "refresh-references",
                        crate::shared::i18n::label("Refresh", "刷新"),
                    )
                    .size(ely_gpui_component::theme::ControlSize::Sm)
                    .disabled(loading)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.update(cx, |state, cx| {
                            state.ensure_reference_catalog(kind, true, cx)
                        });
                    })),
                )
                .into_any_element(),
        )
    }
}
