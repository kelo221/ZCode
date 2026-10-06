use crate::app::store::AppState;
use gpui::Modifiers;
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum SubmitTrigger {
    #[default]
    Ordinary,
    ModifiedEnter,
    ModifiedPointer,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum InputRouting {
    StartNow,
    Enqueue,
    Guide,
    Reject,
    Choice,
}
impl InputRouting {
    pub(crate) fn from_value(value: &Value) -> Option<Self> {
        match value.get("mode")?.as_str()? {
            "startNow" => Some(Self::StartNow),
            "enqueue" => Some(Self::Enqueue),
            "guide" => Some(Self::Guide),
            "reject" => Some(Self::Reject),
            "choice" => Some(Self::Choice),
            _ => None,
        }
    }
}

pub(crate) fn primary_modifier(modifiers: &Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        modifiers.platform
    } else {
        modifiers.control
    }
}

impl AppState {
    pub(crate) fn submit_delivery(
        &self,
        trigger: SubmitTrigger,
        text: &str,
    ) -> Result<Option<&'static str>, &'static str> {
        if self.draft
            || self.active.is_none()
            || self.composer_intent != crate::conversation::msg_actions::ComposerIntent::Send
            || !matches!(
                crate::composer::slash::classify_slash_command(text.trim()),
                crate::composer::slash::SlashAction::Plain(_)
                    | crate::composer::slash::SlashAction::Plan(_)
            )
        {
            return Ok(None);
        }
        let Some(conv) = self.active_conversation() else {
            return if trigger != SubmitTrigger::ModifiedEnter {
                Ok(None)
            } else {
                Err("Input routing is not ready; draft kept")
            };
        };
        match conv.input_routing {
            Some(InputRouting::Reject) => return Err("Input is currently unavailable; draft kept"),
            Some(InputRouting::Choice) => {
                return Err("Held queue requires a clear/keep decision before sending; draft kept");
            }
            None if trigger == SubmitTrigger::ModifiedEnter
                || trigger == SubmitTrigger::ModifiedPointer && conv.can_stop =>
            {
                return Err("Input routing is not ready; draft kept");
            }
            _ => {}
        }
        Ok(self.opposite_delivery(trigger))
    }

    pub(crate) fn opposite_delivery(&self, trigger: SubmitTrigger) -> Option<&'static str> {
        let conv = self.active_conversation()?;
        let reverse = trigger == SubmitTrigger::ModifiedEnter
            || trigger == SubmitTrigger::ModifiedPointer && conv.can_stop;
        reverse.then_some(if conv.config.followup_mode == "guide" {
            "queue"
        } else {
            "startNow"
        })
    }
}

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod tests;
