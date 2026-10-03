//! App shell: the root window view composing every slice, the right dock
//! container, and `AppState` (per-workspace agents + mirrored conversations).

pub(crate) mod dock;
pub(crate) mod mcp_pane;
pub(crate) mod os_lifecycle;
pub(crate) mod parts;
pub(crate) mod plugin_cards;
pub(crate) mod plugin_pane;
pub(crate) mod plugin_sources;
pub(crate) mod quickpick;
pub(crate) mod quickpick_items;
pub(crate) mod root;
pub(crate) mod store;
pub(crate) mod usage_pane;
