//! Shared kernel used by several slices: design tokens and the markdown /
//! diff renderers. Nothing here depends on a feature slice.

pub(crate) mod diff_view;
pub(crate) mod markdown;
pub(crate) mod theme;
