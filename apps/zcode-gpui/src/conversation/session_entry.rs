//! Sidebar row of one session (sessions-index entry, wire
//! `packages/shared/src/zcode-protocol-v4/sessions-index.ts`).

use crate::conversation::model::phase_is_active;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct SessionEntry {
    pub session_id: String,
    pub title: String,
    pub phase: String,
    pub last_activity_at: Option<i64>,
    /// Sidebar sort key. A running turn bumps `last_activity_at` on every
    /// step, so two busy projects kept swapping places; this only advances
    /// when a run starts (the user's message) or the session isn't running
    /// (the run finished). See [`SessionEntry::settle_sort_activity`].
    pub sort_activity_at: Option<i64>,
    pub preview: String,
    /// `hasBackgroundWork`: a background subagent, shell or workflow is running.
    pub has_background_work: bool,
}

impl SessionEntry {
    pub fn from_value(v: &Value) -> Option<Self> {
        let session_id = v.get("sessionId")?.as_str()?.to_string();
        let last_activity_at = v.get("lastActivityAt").and_then(Value::as_i64);
        Some(SessionEntry {
            session_id,
            title: v
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Untitled")
                .to_string(),
            phase: v
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string(),
            last_activity_at,
            sort_activity_at: last_activity_at,
            preview: v
                .get("lastAssistantPreview")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            has_background_work: v
                .get("hasBackgroundWork")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    /// Keep the previous sort stamp while the same run is still going.
    pub fn settle_sort_activity(&mut self, previous: Option<&SessionEntry>) {
        if phase_is_active(&self.phase)
            && let Some(previous) = previous
            && phase_is_active(&previous.phase)
        {
            self.sort_activity_at = previous.sort_activity_at;
        }
    }
}
