//! Conversation model parsed from V4 snapshots/deltas.
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/ (rows.ts, snapshot.ts,
//! delta.ts, sessions-index.ts). Parsing is deliberately lenient: unknown row
//! kinds and delta ops are ignored so additive protocol changes don't break
//! the client (backend may be newer than this frontend).

use crate::composer::catalog::SessionConfig;
use serde_json::Value;
use std::collections::BTreeMap;

/// Single-line preview with ellipsis, for list rendering.
pub(crate) fn format_preview(s: &str, max: usize) -> String {
    let s = s.replace('\n', " ");
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max).collect::<String>())
    } else {
        s
    }
}

#[derive(Clone, Debug)]
pub struct SessionEntry {
    pub session_id: String,
    pub title: String,
    pub phase: String,
    pub last_activity_at: Option<i64>,
    pub preview: String,
}

impl SessionEntry {
    pub fn from_value(v: &Value) -> Option<Self> {
        let session_id = v.get("sessionId")?.as_str()?.to_string();
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
            last_activity_at: v.get("lastActivityAt").and_then(Value::as_i64),
            preview: v
                .get("lastAssistantPreview")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        })
    }
}

pub use crate::conversation::rows::Row;

/// Client mirror of one conversation topic. State patches arrive as
/// whole-key replacement (never deep-merge), per snapshot.ts/delta.ts.
#[derive(Default)]
pub struct ConversationState {
    pub rows: BTreeMap<u64, Row>,
    pub phase: String,
    pub(crate) can_stop: bool,
    pub(crate) input_routing: Option<crate::composer::delivery::InputRouting>,
    pub log_epoch: Option<String>,
    pub seq: u64,
    /// Mirrored CAS revision (`snapshot.revision`, then `state.updated`
    /// patches). Only meaningful once `revision_known` is set.
    pub revision: u64,
    /// A snapshot carrying `revision` has been applied. Until then CAS
    /// commands are unavailable: a defaulted 0 would be a guessed base.
    pub revision_known: bool,
    pub config: SessionConfig,
    pub subscribed: bool,
    /// rowId of the first row in the loaded window; >0 means older history
    /// exists and `v4/conversation/rowsRange` can page it in.
    pub first_row_id: u64,
    pub total_count: u64,
    /// Mirror seq seen by the previous freshness probe (`Pending::PollRows`).
    /// Movement between probes means our own deltas are flowing, which
    /// suppresses the probe's seq comparison (in-flight frames would
    /// false-positive while we stream our own turn).
    pub prev_poll_seq: u64,
    /// Backend-reported failure for the current turn (control.lastError).
    pub last_error: Option<(String, String)>,
    /// Backend automatic retry state (control.apiRetry), shown verbatim.
    pub api_retry: Option<Value>,
    /// Pending interactions (permission, AskUserQuestion elicitation)
    pub pending_interactions: Vec<crate::conversation::interactions::PendingInteraction>,
    /// Answers in flight; filters re-announced cards (idempotent by id).
    pub resolving: crate::conversation::interactions::ResolvingInteractions,
    /// Active plan execution progress
    pub plan: Option<crate::conversation::turn_meta::PlanState>,
    pub(crate) goal: Option<crate::conversation::goal::GoalState>,
    pub(crate) goal_availability: crate::conversation::goal::GoalAvailability,
    pub(crate) queue_edit_allowed: bool,
    /// Follow-up input queue
    pub queue: Option<crate::conversation::queue::QueueState>,
    /// Active foreground execution id for target-scoped stop
    pub active_foreground_execution_id: Option<String>,
    /// Workflow runs state and subagents progress
    pub workflow_runs: crate::conversation::workflows::WorkflowRunsState,
    /// Mirrored subagents projection (subagentProjectionStateSchema).
    pub subagents: Option<crate::conversation::subagents::SubagentsState>,
    /// Background work items (backgroundWorkSummarySchema).
    pub background_works: Vec<crate::conversation::subagents::BackgroundWork>,
}

/// V4 `sessionPhaseSchema` phases during which a turn is live.
pub fn phase_is_active(phase: &str) -> bool {
    matches!(phase, "running" | "prewarming")
}

impl ConversationState {
    #[allow(dead_code)]
    pub fn running_subagents(&self) -> Vec<crate::conversation::subagents::JoinedRunningSubagent> {
        crate::conversation::subagents::join_running_subagents(
            self.subagents.as_ref(),
            &self.background_works,
        )
    }

    pub fn phase_running(&self) -> bool {
        phase_is_active(&self.phase)
    }

    fn set_pending_interactions(&mut self, pis: &[Value]) {
        let incoming = pis
            .iter()
            .filter_map(crate::conversation::interactions::PendingInteraction::from_value)
            .collect();
        self.pending_interactions = self.resolving.reconcile(incoming);
    }

    pub fn has_more_history(&self) -> bool {
        self.first_row_id > 0
    }

    pub fn apply_snapshot(&mut self, snap: &Value) {
        if let Some(epoch) = snap.get("logEpoch").and_then(Value::as_str) {
            self.log_epoch = Some(epoch.to_string());
        }
        self.seq = snap.get("seq").and_then(Value::as_u64).unwrap_or(self.seq);
        if let Some(rev) = snap.get("revision").and_then(Value::as_u64) {
            self.revision = rev;
            self.revision_known = true;
        }
        self.input_routing = snap
            .get("inputRouting")
            .and_then(crate::composer::delivery::InputRouting::from_value);
        self.can_stop = snap.pointer("/control/canStop").and_then(Value::as_bool) == Some(true);
        if let Some(control) = snap.get("control") {
            self.apply_control(control);
        }
        if let Some(config) = snap.get("config") {
            self.apply_config(config);
        }
        if let Some(pis) = snap.get("pendingInteractions").and_then(Value::as_array) {
            self.set_pending_interactions(pis);
        }
        self.queue_edit_allowed = snap
            .pointer("/availability/queueEdit/allowed")
            .and_then(Value::as_bool)
            == Some(true);
        self.goal = snap
            .get("goal")
            .and_then(crate::conversation::goal::GoalState::from_value);
        self.goal_availability = snap
            .get("availability")
            .map(crate::conversation::goal::GoalAvailability::from_value)
            .unwrap_or_default();
        if let Some(p) = snap.get("plan") {
            self.plan = crate::conversation::turn_meta::PlanState::from_value(p);
        }
        if let Some(q) = snap.get("queue") {
            self.queue = crate::conversation::queue::QueueState::from_value(q);
        }
        if let Some(wf) = snap.get("workflowRuns") {
            self.workflow_runs = crate::conversation::workflows::WorkflowRunsState::from_value(wf);
        }
        if let Some(s) = snap.get("subagents") {
            self.subagents = crate::conversation::subagents::SubagentsState::from_value(s);
        }
        if let Some(bw) = snap.get("backgroundWorks").and_then(Value::as_array) {
            self.background_works = bw
                .iter()
                .filter_map(crate::conversation::subagents::BackgroundWork::from_value)
                .collect();
        }
        if let Some(rows) = snap.get("rows") {
            self.first_row_id = rows
                .get("firstRowId")
                .and_then(Value::as_u64)
                .unwrap_or(self.first_row_id);
            self.total_count = rows
                .get("totalCount")
                .and_then(Value::as_u64)
                .unwrap_or(self.total_count);
            if let Some(window) = rows.get("window").and_then(Value::as_array) {
                self.rows.clear();
                for r in window {
                    let id = r.get("rowId").and_then(Value::as_u64).unwrap_or(0);
                    self.rows.insert(id, Row::from_value(r));
                }
            }
        }
    }

    /// Merge a `v4/conversation/rowsRange` result (rows ascending by rowId).
    pub fn apply_older_rows(&mut self, rows: &[Value]) {
        for r in rows {
            let id = r.get("rowId").and_then(Value::as_u64).unwrap_or(0);
            self.rows.entry(id).or_insert_with(|| Row::from_value(r));
        }
        if let Some(min) = self.rows.keys().next() {
            self.first_row_id = (*min).min(self.first_row_id);
        }
    }

    fn apply_control(&mut self, control: &Value) {
        self.can_stop = control.get("canStop").and_then(Value::as_bool) == Some(true);
        self.phase = control
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        self.last_error = match control.get("lastError") {
            Some(e) if !e.is_null() => Some((
                e.get("code")
                    .and_then(Value::as_str)
                    .unwrap_or("error")
                    .to_string(),
                // Provider errors may quote the rejected key verbatim.
                crate::shared::redact::scrub(
                    e.get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error"),
                ),
            )),
            _ => None,
        };
        self.api_retry = match control.get("apiRetry") {
            Some(r) if !r.is_null() => Some(r.clone()),
            _ => None,
        };
        self.active_foreground_execution_id = control
            .get("activeWorks")
            .and_then(Value::as_array)
            .and_then(|works| {
                works.iter().find_map(|w| {
                    w.get("foregroundExecutionId")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
            });
    }

    /// Whole-key replacement of the `config` region (sessionConfigStateSchema).
    pub fn apply_config(&mut self, config: &Value) {
        let str_field = |k: &str| {
            config
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        if config.get("provider").is_some() {
            self.config.provider = str_field("provider");
        }
        if config.get("model").is_some() {
            self.config.model = str_field("model");
        }
        if config.get("thought").is_some() {
            self.config.thought = str_field("thought");
        }
        if let Some(levels) = config.get("thoughtLevels").and_then(Value::as_array) {
            self.config.thought_levels = levels
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
        }
        if config.get("mode").is_some() {
            self.config.mode = str_field("mode");
        }
        if config.get("followupMode").is_some() {
            self.config.followup_mode = str_field("followupMode");
        }
    }

    pub fn apply_deltas(&mut self, deltas: &[Value]) {
        for d in deltas {
            match d.get("op").and_then(Value::as_str).unwrap_or("") {
                "row.appended" | "row.upserted" => {
                    if let Some(r) = d.get("row") {
                        let id = r.get("rowId").and_then(Value::as_u64).unwrap_or(0);
                        self.rows.insert(id, Row::from_value(r));
                    }
                }
                "row.removed" => {
                    let from_id = d
                        .get("fromRowId")
                        .or_else(|| d.get("rowId"))
                        .and_then(Value::as_u64);
                    if let Some(from_id) = from_id {
                        self.rows.retain(|&id, _| id < from_id);
                    }
                }
                "row.delta" => {
                    let path = d.get("path").and_then(Value::as_str).unwrap_or("text");
                    let append = d.get("append").and_then(Value::as_str).unwrap_or("");
                    if append.is_empty() {
                        continue;
                    }
                    if let Some(row) = d
                        .get("rowId")
                        .and_then(Value::as_u64)
                        .and_then(|id| self.rows.get_mut(&id))
                        && let Some(field) = row.stream_field_mut(path)
                    {
                        field.push_str(append);
                    }
                }
                "state.updated" => {
                    if let Some(patch) = d.get("patch") {
                        if let Some(routing) = patch.get("inputRouting") {
                            self.input_routing =
                                crate::composer::delivery::InputRouting::from_value(routing);
                        }
                        if let Some(control) = patch.get("control") {
                            self.apply_control(control);
                        }
                        if let Some(config) = patch.get("config") {
                            self.apply_config(config);
                        }
                        if let Some(rev) = patch.get("revision").and_then(Value::as_u64) {
                            self.revision = rev;
                        }
                        if let Some(pis) =
                            patch.get("pendingInteractions").and_then(Value::as_array)
                        {
                            self.set_pending_interactions(pis);
                        }
                        if let Some(goal) = patch.get("goal") {
                            self.goal = crate::conversation::goal::GoalState::from_value(goal);
                        }
                        if let Some(availability) = patch.get("availability") {
                            self.queue_edit_allowed = availability
                                .pointer("/queueEdit/allowed")
                                .and_then(Value::as_bool)
                                == Some(true);
                            self.goal_availability =
                                crate::conversation::goal::GoalAvailability::from_value(
                                    availability,
                                );
                        }
                        if let Some(p) = patch.get("plan") {
                            self.plan = crate::conversation::turn_meta::PlanState::from_value(p);
                        }
                        if let Some(q) = patch.get("queue") {
                            self.queue = crate::conversation::queue::QueueState::from_value(q);
                        }
                        if let Some(wf) = patch.get("workflowRuns") {
                            self.workflow_runs =
                                crate::conversation::workflows::WorkflowRunsState::from_value(wf);
                        }
                        if let Some(s) = patch.get("subagents") {
                            self.subagents =
                                crate::conversation::subagents::SubagentsState::from_value(s);
                        }
                        if let Some(bw_val) = patch.get("backgroundWorks") {
                            self.background_works = bw_val
                                .as_array()
                                .map(|bw| {
                                    bw.iter()
                                        .filter_map(
                                            crate::conversation::subagents::BackgroundWork::from_value,
                                        )
                                        .collect()
                                })
                                .unwrap_or_default();
                        }
                    }
                }
                "workflowRun.updated" => {
                    self.workflow_runs.apply_updated(d);
                }
                "workflowRun.removed" => {
                    self.workflow_runs.apply_removed(d);
                }
                _ => {} // queue.*, ... intentionally ignored
            }
        }
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
