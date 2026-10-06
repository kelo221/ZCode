use crate::app::store::AppState;
use crate::composer::slash::{SlashAction, classify_slash_command};
use gpui::Context;
use serde_json::json;

impl AppState {
    pub(crate) fn enable_plan_override(&mut self) {
        let mut config = self.submission_override();
        // planEnabled 的展示值会变成 plan；重复准备必须保留已冻结的底层 edit/yolo 模式。
        let effective = self.effective_config();
        let mode = match config
            .get("mode")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&effective.mode)
        {
            "edit" => "edit",
            "yolo" => "yolo",
            _ => "build",
        };
        if !config.is_object() {
            config = json!({});
        }
        config["mode"] = json!(mode);
        config["planEnabled"] = json!(true);
        self.draft_submission_overrides
            .insert(self.composer_draft_key(), config);
    }

    pub(crate) fn prepare_plan_shortcut(&mut self, cx: &mut Context<Self>) -> bool {
        if self.composer_intent != crate::conversation::msg_actions::ComposerIntent::Send {
            return false;
        }
        let composer = self.composer.read(cx);
        let SlashAction::Plan(task) = classify_slash_command(composer.text()) else {
            return false;
        };
        if self.is_read_only_view() || composer.marked_utf16.is_some() {
            return true;
        }
        if !composer.attachments.is_empty() || composer.chips.has_chips() {
            // /plan 只消费纯文本；不能先清空附件/引用再将原始命令误发成普通 prompt。
            self.push_error(
                crate::shared::i18n::label(
                    "/plan accepts text only; draft kept",
                    "/plan 仅支持纯文本，草稿已保留",
                )
                .into(),
            );
            cx.notify();
            return true;
        }
        self.enable_plan_override();
        if task.is_empty() {
            self.held_confirmation = None;
            self.composer.update(cx, |composer, cx| {
                composer.set_text("");
                cx.notify();
            });
            cx.notify();
            return true;
        }
        false
    }
}

#[cfg(test)]
#[path = "plan_failure_tests.rs"]
mod failure_tests;
#[cfg(test)]
#[path = "plan_shortcut_tests.rs"]
mod tests;
