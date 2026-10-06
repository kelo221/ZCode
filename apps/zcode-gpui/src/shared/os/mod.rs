//! OS lifecycle and platform integration modules.

pub mod file_launcher;
pub mod folder_picker;
pub mod keep_awake;
pub mod log_export;
pub mod notifications;
#[cfg(not(test))]
pub(crate) mod sensitive_url;
pub mod single_instance;
pub mod updater;
