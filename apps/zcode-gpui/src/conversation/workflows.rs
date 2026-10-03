//! Workflow runs state mirroring and delta reducer for the V4 protocol.
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/workflow-runs-delta.ts.
//!
//! Invariant: Delta application sequence is strictly header -> removals -> upserts.

pub use crate::conversation::workflows_types::*;
use serde_json::Value;
use std::collections::HashSet;

impl WorkflowRunsState {
    pub fn from_value(v: &Value) -> Self {
        let revision = v.get("revision").and_then(Value::as_u64).unwrap_or(0);
        let runs = v
            .get("runs")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(WorkflowRunState::from_value)
                    .collect()
            })
            .unwrap_or_default();
        Self { revision, runs }
    }

    pub fn apply_updated(&mut self, delta: &Value) {
        if let Some(rev) = delta.get("revision").and_then(Value::as_u64) {
            self.revision = self.revision.max(rev);
        }
        let Some(run_id) = delta.get("runId").and_then(Value::as_str) else {
            return;
        };

        let index = self.runs.iter().position(|r| r.run_id == run_id);
        match index {
            None => {
                if let Some(header) = delta.get("run")
                    && is_complete_header(header)
                {
                    let mut run = WorkflowRunState::from_value_header(header, run_id);
                    if let Some(actors) = delta.get("actors").and_then(Value::as_array) {
                        run.actors = actors
                            .iter()
                            .filter_map(WorkflowRunActor::from_value)
                            .collect();
                    }
                    if let Some(nodes) = delta.get("nodes").and_then(Value::as_array) {
                        run.nodes = nodes
                            .iter()
                            .filter_map(WorkflowRunNode::from_value)
                            .collect();
                    }
                    self.runs.push(run);
                }
            }
            Some(idx) => {
                let run = &mut self.runs[idx];

                // Step 1: header patch and cleared keys.
                if let Some(patch) = delta.get("run") {
                    run.apply_header_patch(patch);
                }
                if let Some(cleared) = delta.get("cleared").and_then(Value::as_array) {
                    for key in cleared.iter().filter_map(Value::as_str) {
                        run.clear_field(key);
                    }
                }

                // Step 2: removals (by (siteId, ordinal)).
                if let Some(rem_actors) = delta.get("removedActors").and_then(Value::as_array) {
                    let set: HashSet<String> = rem_actors
                        .iter()
                        .filter_map(WorkflowRunEntryRef::from_value)
                        .map(|r| r.key())
                        .collect();
                    run.actors
                        .retain(|a| !set.contains(&format!("{}\0{}", a.site_id, a.ordinal)));
                }
                if let Some(rem_nodes) = delta.get("removedNodes").and_then(Value::as_array) {
                    let set: HashSet<String> = rem_nodes
                        .iter()
                        .filter_map(WorkflowRunEntryRef::from_value)
                        .map(|r| r.key())
                        .collect();
                    run.nodes
                        .retain(|n| !set.contains(&format!("{}\0{}", n.site_id, n.ordinal)));
                }

                // Step 3: upserts (in-place update or append).
                if let Some(actors) = delta.get("actors").and_then(Value::as_array) {
                    for val in actors {
                        if let Some(incoming) = WorkflowRunActor::from_value(val) {
                            if let Some(existing) = run.actors.iter_mut().find(|a| {
                                a.site_id == incoming.site_id && a.ordinal == incoming.ordinal
                            }) {
                                *existing = incoming;
                            } else {
                                run.actors.push(incoming);
                            }
                        }
                    }
                }
                if let Some(nodes) = delta.get("nodes").and_then(Value::as_array) {
                    for val in nodes {
                        if let Some(incoming) = WorkflowRunNode::from_value(val) {
                            if let Some(existing) = run.nodes.iter_mut().find(|n| {
                                n.site_id == incoming.site_id && n.ordinal == incoming.ordinal
                            }) {
                                *existing = incoming;
                            } else {
                                run.nodes.push(incoming);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn apply_removed(&mut self, delta: &Value) {
        if let Some(rev) = delta.get("revision").and_then(Value::as_u64) {
            self.revision = self.revision.max(rev);
        }
        let Some(run_id) = delta.get("runId").and_then(Value::as_str) else {
            return;
        };
        self.runs.retain(|r| r.run_id != run_id);
    }
}

fn is_complete_header(v: &Value) -> bool {
    v.get("status").is_some() && v.get("usage").is_some()
}

#[cfg(test)]
#[path = "workflows_tests.rs"]
mod tests;
