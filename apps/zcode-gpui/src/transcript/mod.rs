//! Transcript: virtualized chat list, row rendering and scroll behavior
//! (follow-bottom, history anchoring, middle-click autoscroll).

pub(crate) mod chat;
pub(crate) mod image_viewer;
pub(crate) mod list;
pub(crate) mod message_actions;
pub(crate) mod noise_free;
pub(crate) mod reasoning;
pub(crate) mod scroll;
#[cfg(test)]
mod scroll_tests;
pub(crate) mod subagent_card;
pub(crate) mod tagged_text;
pub(crate) mod tool_card;
#[cfg(test)]
mod tool_card_tests;
pub(crate) mod tool_images;
pub(crate) mod tool_inputs;
pub(crate) mod user_row;
