//! Conversation domain: the mirrored V4 conversation model (rows, turn
//! metadata, plan, queue, pending interactions), the user actions on it and
//! the interaction cards.

pub(crate) mod interaction_actions;
pub(crate) mod interaction_view;
pub(crate) mod interactions;
pub(crate) mod model;
pub(crate) mod msg_actions;
pub(crate) mod queue;
pub(crate) mod rows;
pub(crate) mod turn_meta;
pub(crate) mod workflows;
pub(crate) mod workflows_types;
pub(crate) mod workflows_view;
