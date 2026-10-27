//! Up-arrow recall of messages already sent in the active conversation, like
//! a shell: Up in an empty box shows the newest one, more Ups go older, Down
//! goes newer and past the newest empties the box again. Typing into a
//! recalled message ends recall, so the arrows move the caret as usual.

use crate::composer::input::Composer;
use gpui::{App, Context};
use std::rc::Rc;

/// Sent messages of the active conversation, newest first.
pub(crate) type RecallSource = Rc<dyn Fn(&App) -> Vec<String>>;

#[derive(Default)]
pub(crate) struct Recall {
    /// Position in the source list of the message on screen.
    shown: Option<usize>,
    pub(crate) source: Option<RecallSource>,
}

/// Next (shown index, box text) for an Up (`older`) or Down press, or `None`
/// when the key should act as a normal caret move.
pub(crate) fn recall_step(
    sent: &[String],
    shown: Option<usize>,
    content: &str,
    older: bool,
) -> Option<(Option<usize>, String)> {
    let on_screen = match shown {
        Some(i) if sent.get(i).is_some_and(|s| s == content) => Some(i),
        Some(_) => return None,
        None if content.is_empty() => None,
        None => return None,
    };
    match (on_screen, older) {
        (None, true) => sent.first().map(|s| (Some(0), s.clone())),
        (None, false) => None,
        (Some(i), true) => sent.get(i + 1).map(|s| (Some(i + 1), s.clone())),
        (Some(0), false) => Some((None, String::new())),
        (Some(i), false) => Some((Some(i - 1), sent[i - 1].clone())),
    }
}

/// User-typed messages of a conversation, newest first. Only loaded rows
/// count; older pages appear once the transcript has fetched them.
pub(crate) fn sent_messages(conv: &crate::conversation::model::ConversationState) -> Vec<String> {
    conv.rows
        .values()
        .rev()
        .filter_map(|row| match row {
            crate::conversation::rows::Row::UserInput { text, origin, .. }
                if origin == "realUser" && !text.trim().is_empty() =>
            {
                Some(text.clone())
            }
            _ => None,
        })
        .collect()
}

/// Recall from the active conversation of the `AppState` owning `composer`.
pub(crate) fn wire_active_conversation(
    composer: &gpui::Entity<Composer>,
    cx: &mut Context<crate::app::store::AppState>,
) {
    let owner = cx.weak_entity();
    composer.update(cx, |c, _| {
        c.recall.source = Some(Rc::new(move |cx: &App| {
            owner
                .upgrade()
                .and_then(|s| {
                    let s = s.read(cx);
                    Some(sent_messages(s.conversations.get(s.active.as_ref()?)?))
                })
                .unwrap_or_default()
        }));
    });
}

impl Composer {
    /// Try a recall step; `false` leaves the key to caret movement.
    pub(crate) fn recall(&mut self, older: bool, cx: &mut Context<Self>) -> bool {
        if self.single_line || self.marked_utf16.is_some() {
            return false;
        }
        let Some(source) = self.recall.source.clone() else {
            return false;
        };
        let sent = source(cx);
        let Some((shown, text)) = recall_step(&sent, self.recall.shown, &self.content, older)
        else {
            self.recall.shown = None;
            return false;
        };
        self.set_text(&text);
        self.recall.shown = shown;
        cx.notify();
        true
    }
}

#[cfg(test)]
#[path = "recall_tests.rs"]
mod tests;
