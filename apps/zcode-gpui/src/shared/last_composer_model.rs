use serde::{Deserialize, Serialize};

/// Model choice remembered across new drafts (Desktop `composerRecent.ts`).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct LastComposerModel {
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub thought: String,
}
