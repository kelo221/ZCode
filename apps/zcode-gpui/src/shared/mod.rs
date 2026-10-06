//! Shared kernel used by several slices: design tokens and the markdown /
//! diff renderers. Nothing here depends on a feature slice.

pub(crate) mod data_paths;
pub(crate) mod desktop_lock;
pub(crate) mod diff_view;
pub(crate) mod i18n;
pub(crate) mod identity;
pub(crate) mod markdown;
pub(crate) mod mcp;
pub(crate) mod os;
pub(crate) mod plugin_config;
pub(crate) mod plugin_description;
pub(crate) mod plugin_results;
pub(crate) mod plugins;
pub(crate) mod preferences;
pub(crate) mod redact;
pub(crate) mod saved_workflows;
pub(crate) mod settings;
pub(crate) mod shortcut_runtime;
pub(crate) mod shortcuts;
pub(crate) mod temp_attachments;
pub(crate) mod theme;
pub(crate) mod theme_colors;
pub(crate) mod usage_stats;
pub(crate) mod window_state;
pub(crate) mod workflow_artifact_content;
pub(crate) mod workflow_artifacts;
pub(crate) mod workflow_definition;
pub(crate) mod workflow_history;
pub(crate) mod workflow_mutation;
