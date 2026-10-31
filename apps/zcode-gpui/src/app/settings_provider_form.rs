//! Text inputs of the Model settings page. Values live in the agent's view;
//! inputs only hold the user's draft and commit one field on Enter or blur
//! (desktop `InlineEditableProviderCard` saves the same way).

use crate::app::root::RootView;
use crate::provider_settings::Provider;
use crate::shared::i18n::label;
use ely_gpui_component::forms::{InputEvent, TextInput};
use gpui::{AppContext, Context, Entity, Focusable, Window};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Field {
    Name,
    BaseUrl,
    ApiKey,
    NewProvider,
}

pub(crate) struct ProviderForm {
    provider: String,
    revision: u64,
    pub name: Entity<TextInput>,
    pub base_url: Entity<TextInput>,
    pub api_key: Entity<TextInput>,
    pub new_provider: Entity<TextInput>,
    pub show_key: bool,
    pub(crate) auto_compact: crate::app::settings_auto_compact::MinutesInputs,
    /// The open add-model dialog of the selected provider.
    pub add_model: Option<crate::app::settings_provider_model_dialog::AddModelDialog>,
    pub dialogs_opened: u64,
}

impl ProviderForm {
    pub(crate) fn input(&self, field: Field) -> &Entity<TextInput> {
        match field {
            Field::Name => &self.name,
            Field::BaseUrl => &self.base_url,
            Field::ApiKey => &self.api_key,
            Field::NewProvider => &self.new_provider,
        }
    }
}

/// The single-field `updateProvider` a commit produces, if the draft differs.
pub(crate) fn field_update(provider: &Provider, field: Field, draft: &str) -> Option<Value> {
    let id = provider.id.as_str();
    match field {
        Field::Name => {
            let name = draft.trim();
            (provider.custom() && !name.is_empty() && name != provider.name).then(
                || json!({ "type": "updateProvider", "providerId": id, "providerName": name }),
            )
        }
        Field::BaseUrl => (provider.custom() && draft.trim() != provider.base_url).then(
            || json!({ "type": "updateProvider", "providerId": id, "baseUrl": draft.trim() }),
        ),
        Field::ApiKey => (provider.uses_api_key() && draft != provider.api_key)
            .then(|| json!({ "type": "updateProvider", "providerId": id, "apiKey": draft })),
        Field::NewProvider => None,
    }
}

fn make_input(field: Field, window: &mut Window, cx: &mut Context<RootView>) -> Entity<TextInput> {
    let placeholder = match field {
        Field::Name => label("Provider name", "提供商名称"),
        Field::BaseUrl => "https://api.example.com/v1",
        Field::ApiKey => label("API key", "API 密钥"),
        Field::NewProvider => label("Custom provider name", "自定义提供商名称"),
    };
    let input = cx.new(|cx| {
        let input = TextInput::new(window, cx).placeholder(placeholder);
        if field == Field::ApiKey {
            input.masked()
        } else {
            input
        }
    });
    cx.subscribe(
        &input,
        move |this: &mut RootView, _, event: &InputEvent, cx| {
            // 新增类输入只在 Enter 时提交；失焦提交会在用户点“取消”时误建对象。
            let commit = match field {
                Field::NewProvider => *event == InputEvent::Submit,
                _ => matches!(event, InputEvent::Submit | InputEvent::Blur),
            };
            if commit {
                this.commit_provider_field(field, cx);
            }
        },
    )
    .detach();
    input
}

impl RootView {
    /// Creates the inputs once, then re-seeds them from the view when the
    /// selection or the view revision changes. A focused input keeps the
    /// user's draft: a response to another field must not wipe it.
    pub(crate) fn sync_provider_form(
        &mut self,
        provider: Option<&Provider>,
        revision: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings.provider_form.is_none() {
            let form = ProviderForm {
                provider: String::new(),
                revision: u64::MAX,
                name: make_input(Field::Name, window, cx),
                base_url: make_input(Field::BaseUrl, window, cx),
                api_key: make_input(Field::ApiKey, window, cx),
                new_provider: make_input(Field::NewProvider, window, cx),
                show_key: false,
                auto_compact: Default::default(),
                add_model: None,
                dialogs_opened: 0,
            };
            self.settings.provider_form = Some(form);
        }
        let form = self.settings.provider_form.as_mut().unwrap();
        let id = provider.map(|p| p.id.clone()).unwrap_or_default();
        let switched = form.provider != id;
        if !switched && form.revision == revision {
            return;
        }
        form.provider = id;
        form.revision = revision;
        if switched {
            form.show_key = false;
            form.api_key
                .update(cx, |input, cx| input.set_masked(true, cx));
            // 切换提供商必须丢弃旧草稿与在途请求，不能把上一提供商的输入带到新目标。
            form.add_model = None;
        }
        let values = [
            (Field::Name, provider.map(|p| p.name.clone())),
            (Field::BaseUrl, provider.map(|p| p.base_url.clone())),
            (Field::ApiKey, provider.map(|p| p.api_key.clone())),
        ];
        for (field, value) in values {
            let input = form.input(field).clone();
            if !switched && input.read(cx).focus_handle(cx).is_focused(window) {
                continue;
            }
            input.update(cx, |input, cx| {
                input.set_text(value.unwrap_or_default(), cx)
            });
        }
    }

    pub(crate) fn commit_provider_field(&mut self, field: Field, cx: &mut Context<Self>) {
        let Some(form) = self.settings.provider_form.as_ref() else {
            return;
        };
        let input = form.input(field).clone();
        let draft = input.read(cx).text().to_owned();
        if field == Field::NewProvider {
            let name = draft.trim().to_owned();
            if name.is_empty() {
                return;
            }
            input.update(cx, |input, cx| input.set_text("", cx));
            let operation = json!({ "type": "createProvider", "providerName": name });
            self.state
                .update(cx, |s, cx| s.mutate_provider_settings(operation, cx));
            return;
        }
        let provider = self
            .state
            .read(cx)
            .provider_settings
            .selected_provider()
            .cloned();
        let Some(operation) = provider.and_then(|p| field_update(&p, field, &draft)) else {
            return;
        };
        self.state
            .update(cx, |s, cx| s.mutate_provider_settings(operation, cx));
    }

    pub(crate) fn toggle_provider_key_visibility(&mut self, cx: &mut Context<Self>) {
        if let Some(form) = self.settings.provider_form.as_mut() {
            form.show_key = !form.show_key;
            let masked = !form.show_key;
            form.api_key
                .update(cx, |input, cx| input.set_masked(masked, cx));
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "settings_provider_form_tests.rs"]
mod tests;
