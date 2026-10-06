//! Transcript: virtualized chat list, row rendering and scroll behavior
//! (follow-bottom, history anchoring, middle-click autoscroll).

pub(crate) mod chat;
pub(crate) mod list;
pub(crate) mod reasoning;
pub(crate) mod scroll;
#[cfg(test)]
mod scroll_tests;
pub(crate) mod subagent_card;
pub(crate) mod tool_card;
