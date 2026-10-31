//! Settings › System prompt (GPUI only, PARITY.md "System prompt override").
//! The editor starts from the committed text (override, else the agent's
//! default). Apply commits it; blank or default text applies as a reset.

use crate::app::root::RootView;
use crate::shared::i18n::label;
use crate::shared::theme::active_theme;
use crate::system_prompt::applied_override;
use ely_gpui_component::buttons::Button;
use ely_gpui_component::forms::{Input, InputEvent, TextInput};
use gpui::{AnyElement, AppContext, Context, Entity, IntoElement, Window, div, prelude::*};

pub(crate) struct PromptEditor {
    pub input: Entity<TextInput>,
    /// The committed text the editor was last filled with. An editor still
    /// showing it is "untouched" and follows later commits (default arriving,
    /// Reset, another window applying).
    pub seeded: String,
}

impl RootView {
    fn seed_prompt_editor(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        match &mut self.settings.system_prompt {
            Some(editor) => {
                editor
                    .input
                    .update(cx, |input, cx| input.set_text(text.clone(), cx));
                editor.seeded = text;
            }
            None => {
                let input = cx.new(|cx| {
                    let mut input = TextInput::new(window, cx)
                        .multi_line(16, 4000)
                        .placeholder(label("System prompt", "系统提示词"));
                    input.set_text(text.clone(), cx);
                    input
                });
                cx.subscribe(&input, |_, _, event: &InputEvent, cx| {
                    if *event == InputEvent::Changed {
                        cx.notify();
                    }
                })
                .detach();
                self.settings.system_prompt = Some(PromptEditor {
                    input,
                    seeded: text,
                });
            }
        }
    }

    pub(crate) fn system_prompt_pane(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = active_theme();
        let prompt = &self.state.read(cx).system_prompt;
        let custom = prompt.override_text.clone();
        let default = prompt.default_prompt.clone();
        let (reading, unavailable, error) =
            (prompt.reading, prompt.unavailable, prompt.error.clone());
        let committed = custom
            .clone()
            .or_else(|| default.as_ref().map(|d| d.text.clone()))
            .unwrap_or_default();
        let untouched = self
            .settings
            .system_prompt
            .as_ref()
            .is_none_or(|e| e.input.read(cx).text() == e.seeded);
        if untouched
            && self
                .settings
                .system_prompt
                .as_ref()
                .is_none_or(|e| e.seeded != committed)
        {
            self.seed_prompt_editor(committed.clone(), window, cx);
        }
        let editor = self.settings.system_prompt.as_ref().unwrap();
        let text = editor.input.read(cx).text().to_owned();
        let default_text = default.as_ref().map(|d| d.text.clone());
        let next = applied_override(&text, default_text.as_deref());
        let apply_disabled = unavailable || next == custom;
        let reset_disabled = unavailable || (custom.is_none() && untouched);

        let apply = Button::new("settings-system-prompt-apply", label("Apply", "应用"))
            .disabled(apply_disabled)
            .on_click(cx.listener(move |this, _, _, cx| {
                let next = next.clone();
                this.state
                    .update(cx, |s, cx| s.apply_system_prompt(next.clone(), cx));
                if let Some(editor) = this.settings.system_prompt.as_mut() {
                    editor.seeded =
                        next.unwrap_or_else(|| default_text.clone().unwrap_or_default());
                }
                cx.notify();
            }));
        let reset = Button::new("settings-system-prompt-reset", label("Reset", "重置"))
            .disabled(reset_disabled)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.state
                    .update(cx, |s, cx| s.apply_system_prompt(None, cx));
                let fresh = this
                    .state
                    .read(cx)
                    .system_prompt
                    .default_prompt
                    .as_ref()
                    .map(|d| d.text.clone())
                    .unwrap_or_default();
                this.seed_prompt_editor(fresh, window, cx);
                cx.notify();
            }));
        let badge = if custom.is_some() {
            label("Custom prompt active", "正在使用自定义提示词")
        } else {
            label("Default prompt", "默认提示词")
        };
        let status = if unavailable {
            Some(label(
                "This agent backend doesn't support prompt overrides. Run GPUI with its own backend (bun source or zcode-backend).",
                "当前智能体后端不支持覆盖提示词。请使用 GPUI 自带的后端（bun 源码或 zcode-backend）。",
            ))
        } else if reading {
            Some(label("Loading the default prompt…", "正在加载默认提示词…"))
        } else if default.as_ref().is_some_and(|d| !d.from_session) {
            Some(label(
                "Showing the base text only: open a chat in this workspace to load the full default, including environment details.",
                "仅显示基础文本：在此工作区打开一个对话即可加载完整默认提示词（含环境信息）。",
            ))
        } else if default.is_none() {
            Some(label(
                "Open a workspace to load the default prompt.",
                "打开一个工作区以加载默认提示词。",
            ))
        } else {
            None
        };
        let field = div().w_full().child(Input::new(&editor.input));
        #[cfg(test)]
        let field = crate::app::test_support::track_children(
            field,
            vec!["settings-system-prompt-input".into()],
        );
        let actions = div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex_1().child(badge))
            .child(reset)
            .child(apply);
        #[cfg(test)]
        let actions = crate::app::test_support::track_children(
            actions,
            vec![
                "settings-system-prompt-badge".into(),
                "settings-system-prompt-reset".into(),
                "settings-system-prompt-apply".into(),
            ],
        );
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().text_color(theme.subtle).child(label(
                "GPUI only. Your text replaces the agent's whole system prompt, including environment, git status, memory and desktop instructions. The first line \"You are GCode, an interactive coding agent\", skills, AGENTS.md and the date are always kept. Applies to every workspace, on the next turn of running chats.",
                "仅限 GPUI。你的文本会替换智能体的整个系统提示词，包括环境、Git 状态、记忆和桌面说明。首行 \"You are GCode, an interactive coding agent\"、技能、AGENTS.md 与日期始终保留。对所有工作区生效，运行中的对话在下一轮生效。",
            )))
            .child(actions)
            .children(status.map(|s| div().text_color(theme.subtle).child(s)))
            .children(error.map(|e| div().text_color(theme.destructive).child(e)))
            .child(field)
            .into_any_element()
    }
}
