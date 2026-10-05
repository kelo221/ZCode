//! Shared kernel used by several slices: design tokens and the markdown /
//! diff renderers. Nothing here depends on a feature slice.

pub(crate) mod desktop_lock;
pub(crate) mod diff_view;
pub(crate) mod i18n;
pub(crate) mod identity;
pub(crate) mod markdown;
pub(crate) mod mcp;
pub(crate) mod os;
pub(crate) mod plugins;
pub(crate) mod redact;
pub(crate) mod settings;
pub(crate) mod shortcuts;
pub(crate) mod temp_attachments;
pub(crate) mod theme;
pub(crate) mod usage_stats;
pub(crate) mod window_state;
