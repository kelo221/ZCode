//! Composer: the message input, its mode/model/thinking pickers and the
//! session config catalog those pickers switch between.

pub(crate) mod attachment;
mod attachment_input;
pub(crate) mod autocomplete;
pub(crate) mod catalog;
pub(crate) mod chips;
pub(crate) mod config_cmds;
mod context_ring;
pub(crate) mod delivery;
pub(crate) mod editing;
pub(crate) mod held_queue;
pub(crate) mod ime;
pub(crate) mod input;
mod keys;
pub(crate) mod menus;
pub(crate) mod pasted_text;
mod plan_shortcut;
pub(crate) mod reference_view;
pub(crate) mod references;
pub(crate) mod slash;
pub(crate) mod slash_catalog;
pub(crate) mod submission_config;
pub(crate) mod text_view;
pub(crate) mod thought_cycle;
pub(crate) mod tray;
