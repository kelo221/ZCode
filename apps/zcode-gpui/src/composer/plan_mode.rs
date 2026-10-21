//! Plan overlay controls. Desktop parity: packages/ui/src/v4/composer/
//! V4ComposerModeControls.tsx — Plan is a check item over the permission
//! radios, and the CLI keeps `config.mode` (permission) and
//! `config.planEnabled` (overlay) apart (packages/shared/src/execution-state.ts).

use crate::app::store::AppState;
use crate::shared::i18n::t;
use gpui::Context;
use serde_json::json;

/// Permission radios in Desktop order. Legacy `edit` is listed only while it
/// is the current mode (Desktop hides it from new choices).
pub const PERMISSION_MODES: [&str; 3] = ["build", "guarded", "yolo"];

/// Permission mode as shown and sent: plan and unknown/internal values fall
/// back to build, like Desktop `normalizeAvailableZCodeMode`.
pub fn permission_mode(mode: &str) -> &str {
    match mode {
        "edit" | "guarded" | "yolo" => mode,
        _ => "build",
    }
}

pub fn mode_label(mode: &str) -> &'static str {
    t(match permission_mode(mode) {
        "edit" => "mode.label.glm.edit",
        "guarded" => "mode.label.glm.guarded",
        "yolo" => "mode.label.glm.yolo",
        _ => "mode.label.glm.build",
    })
}

impl AppState {
    /// Turn the plan overlay on or off without touching the permission mode.
    pub(crate) fn set_plan_enabled(&mut self, on: bool, cx: &mut Context<Self>) {
        let effective = self.effective_config();
        let key = self.composer_draft_key();
        if let Some(config) = self.draft_submission_overrides.get_mut(&key) {
            if config.get("mode").is_none() {
                config["mode"] = json!(effective.mode);
            }
            config["planEnabled"] = json!(on);
        } else if effective.has_session {
            if effective.plan_enabled != on {
                // CLI resolveExecutionState: `plan` sets the overlay and keeps the
                // permission; re-sending the permission clears the overlay.
                let mode = if on { "plan" } else { effective.mode.as_str() };
                self.switch_mode(mode, cx);
            }
        } else {
            self.ui_plan = on;
        }
        cx.notify();
    }

    /// Pick a permission mode; an active plan overlay stays on.
    pub(crate) fn select_permission_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        let effective = self.effective_config();
        let key = self.composer_draft_key();
        if let Some(config) = self.draft_submission_overrides.get_mut(&key) {
            config["mode"] = json!(mode);
        } else if effective.has_session && effective.plan_enabled {
            // 运行中会话直接发 {mode:X} 会被 CLI 解释为同时关闭计划模式；
            // 与 Desktop 一样只改草稿，随下一次提交以 {mode:X, planEnabled:true} 生效。
            self.draft_submission_overrides
                .insert(key, json!({"mode": mode, "planEnabled": true}));
        } else if effective.has_session {
            self.switch_mode(mode, cx);
        } else {
            self.ui_mode = Some(mode.to_string());
        }
        cx.notify();
    }

    pub(crate) fn plan_transition_before_frame(&self, topic: &str) -> Option<Option<String>> {
        let sid = topic.strip_prefix("conversation/")?;
        Some(
            self.conversations
                .get(sid)?
                .config
                .plan_transition_id
                .clone(),
        )
    }

    /// Agent-driven Enter/ExitPlanMode wins over a draft override left from
    /// `/plan` or a failed send (Desktop useDraftConfigControl planTransition
    /// rebase). `before` is `None` when the conversation was not mirrored yet,
    /// so the first snapshot of a resumed session never drops a restored draft.
    pub(crate) fn reconcile_plan_override(&mut self, sid: &str, before: Option<Option<String>>) {
        let Some(before) = before else {
            return;
        };
        let after = self
            .conversations
            .get(sid)
            .and_then(|c| c.config.plan_transition_id.clone());
        if after.is_none() || after == before {
            return;
        }
        if let Some(config) = self.draft_submission_overrides.get_mut(sid)
            && let Some(map) = config.as_object_mut()
        {
            map.remove("mode");
            map.remove("planEnabled");
            if map.is_empty() {
                self.draft_submission_overrides.remove(sid);
            }
        }
    }
}

#[cfg(test)]
#[path = "plan_mode_tests.rs"]
mod tests;
