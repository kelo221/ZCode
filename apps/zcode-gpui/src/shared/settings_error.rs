//! Error type of the settings reader/writer (split from shared/settings.rs
//! for the 400-line cap).

#[derive(Debug)]
pub enum SettingsError {
    DesktopRunning,
    Invalid(String),
    /// The existing file does not parse; writing would replace it wholesale.
    Unreadable,
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DesktopRunning => {
                write!(
                    f,
                    "Cannot modify settings while GCode desktop is running. Please change this in GCode desktop."
                )
            }
            Self::Unreadable => write!(
                f,
                "setting.json could not be parsed; not overwriting it. Fix it in GCode desktop."
            ),
            Self::Invalid(e) => write!(f, "Invalid preference: {e}"),
            Self::Io(e) => write!(f, "IO error: {e}"),
            Self::Json(e) => write!(f, "JSON error: {e}"),
        }
    }
}

impl std::error::Error for SettingsError {}

impl From<std::io::Error> for SettingsError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for SettingsError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}
