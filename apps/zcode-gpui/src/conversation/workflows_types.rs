//! Workflow run data models and deserialization schemas for the V4 protocol.
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/workflow-runs.ts.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunsState {
    pub revision: u64,
    pub runs: Vec<WorkflowRunState>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunState {
    pub run_id: String,
    pub tool_call_id: Option<String>,
    pub status: String,
    pub stop_reason: Option<String>,
    pub resumed_from: Option<String>,
    pub superseded_by: Option<String>,
    pub usage: WorkflowRunUsage,
    pub error: Option<String>,
    pub resumable: Option<bool>,
    pub result_preview: Option<String>,
    pub actors: Vec<WorkflowRunActor>,
    pub nodes: Vec<WorkflowRunNode>,
    pub reports: Option<Vec<WorkflowRunReport>>,
    pub pending_questions: Option<Vec<WorkflowRunPendingQuestion>>,
    pub concurrency: Option<WorkflowRunConcurrency>,
    pub concurrency_ceiling: Option<u64>,
    pub subagent_model: Option<String>,
    pub phases: Option<Vec<WorkflowRunPhase>>,
    pub phase_names: Option<Vec<String>>,
    pub truncated: Option<bool>,
    pub last_event_sequence: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunUsage {
    pub spent_tokens: u64,
    pub nodes_used: u64,
    pub nodes_unlisted: Option<u64>,
    pub nodes_unlisted_settled: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunActor {
    pub site_id: String,
    pub ordinal: u64,
    pub name: Option<String>,
    pub session_id: Option<String>,
    pub status: String,
    pub phase_name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunNode {
    pub site_id: String,
    pub ordinal: u64,
    pub kind: Option<String>,
    pub phase: String,
    pub outcome: Option<String>,
    pub cached: Option<bool>,
    pub actor_site_id: Option<String>,
    pub actor_ordinal: Option<u64>,
    pub phase_name: Option<String>,
    pub instructions_head: Option<String>,
    pub turn: Option<u64>,
    pub tool_calls: Option<u64>,
    pub last_tool: Option<WorkflowRunNodeLastTool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunNodeLastTool {
    pub name: String,
    pub target: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunReport {
    pub site_id: String,
    pub ordinal: u64,
    pub preview: String,
    pub artifact_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunPendingQuestion {
    pub qid: String,
    pub actor_site_id: Option<String>,
    pub actor_ordinal: Option<u64>,
    pub actor_name: Option<String>,
    pub question: String,
    pub context: Option<String>,
    pub asked_at: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunConcurrency {
    pub key: Option<String>,
    pub cap: u64,
    pub ceiling: u64,
    pub limit: Option<u64>,
    pub cooldown_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowRunPhase {
    pub name: String,
    pub rounds: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct WorkflowRunEntryRef {
    pub site_id: String,
    pub ordinal: u64,
}

impl WorkflowRunEntryRef {
    pub fn key(&self) -> String {
        format!("{}\0{}", self.site_id, self.ordinal)
    }

    pub fn from_value(v: &Value) -> Option<Self> {
        let site_id = v.get("siteId").and_then(Value::as_str)?.to_string();
        let ordinal = v.get("ordinal").and_then(Value::as_u64)?;
        Some(Self { site_id, ordinal })
    }
}

impl WorkflowRunState {
    pub fn from_value(v: &Value) -> Option<Self> {
        let run_id = v.get("runId").and_then(Value::as_str)?.to_string();
        let mut run = Self::from_value_header(v, &run_id);
        if let Some(actors) = v.get("actors").and_then(Value::as_array) {
            run.actors = actors
                .iter()
                .filter_map(WorkflowRunActor::from_value)
                .collect();
        }
        if let Some(nodes) = v.get("nodes").and_then(Value::as_array) {
            run.nodes = nodes
                .iter()
                .filter_map(WorkflowRunNode::from_value)
                .collect();
        }
        Some(run)
    }

    pub fn from_value_header(v: &Value, run_id: &str) -> Self {
        let mut run = Self {
            run_id: run_id.to_string(),
            ..Default::default()
        };
        run.apply_header_patch(v);
        run
    }

    pub fn apply_header_patch(&mut self, v: &Value) {
        if let Some(s) = v.get("toolCallId").and_then(Value::as_str) {
            self.tool_call_id = Some(s.to_string());
        }
        if let Some(s) = v.get("status").and_then(Value::as_str) {
            self.status = s.to_string();
        }
        if let Some(s) = v.get("stopReason").and_then(Value::as_str) {
            self.stop_reason = Some(s.to_string());
        }
        if let Some(s) = v.get("resumedFrom").and_then(Value::as_str) {
            self.resumed_from = Some(s.to_string());
        }
        if let Some(s) = v.get("supersededBy").and_then(Value::as_str) {
            self.superseded_by = Some(s.to_string());
        }
        if let Some(u) = v.get("usage") {
            self.usage = WorkflowRunUsage::from_value(u);
        }
        if let Some(s) = v.get("error").and_then(Value::as_str) {
            self.error = Some(s.to_string());
        }
        if let Some(b) = v.get("resumable").and_then(Value::as_bool) {
            self.resumable = Some(b);
        }
        if let Some(s) = v.get("resultPreview").and_then(Value::as_str) {
            self.result_preview = Some(s.to_string());
        }
        if let Some(sub) = v.get("subagentModel").and_then(Value::as_str) {
            self.subagent_model = Some(sub.to_string());
        }
        if let Some(seq) = v.get("lastEventSequence").and_then(Value::as_u64) {
            self.last_event_sequence = seq;
        }
        if let Some(b) = v.get("truncated").and_then(Value::as_bool) {
            self.truncated = Some(b);
        }
    }

    pub fn clear_field(&mut self, key: &str) {
        match key {
            "reports" => self.reports = None,
            "pendingQuestions" => self.pending_questions = None,
            "concurrency" => self.concurrency = None,
            "subagentModel" => self.subagent_model = None,
            "error" => self.error = None,
            "resultPreview" => self.result_preview = None,
            _ => {}
        }
    }
}

impl WorkflowRunUsage {
    pub fn from_value(v: &Value) -> Self {
        Self {
            spent_tokens: v.get("spentTokens").and_then(Value::as_u64).unwrap_or(0),
            nodes_used: v.get("nodesUsed").and_then(Value::as_u64).unwrap_or(0),
            nodes_unlisted: v.get("nodesUnlisted").and_then(Value::as_u64),
            nodes_unlisted_settled: v.get("nodesUnlistedSettled").and_then(Value::as_u64),
        }
    }
}

impl WorkflowRunActor {
    pub fn from_value(v: &Value) -> Option<Self> {
        let site_id = v.get("siteId").and_then(Value::as_str)?.to_string();
        let ordinal = v.get("ordinal").and_then(Value::as_u64)?;
        let name = v.get("name").and_then(Value::as_str).map(str::to_string);
        let session_id = v
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let status = v
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("waiting")
            .to_string();
        let phase_name = v
            .get("phaseName")
            .and_then(Value::as_str)
            .map(str::to_string);
        Some(Self {
            site_id,
            ordinal,
            name,
            session_id,
            status,
            phase_name,
        })
    }
}

impl WorkflowRunNode {
    pub fn from_value(v: &Value) -> Option<Self> {
        let site_id = v.get("siteId").and_then(Value::as_str)?.to_string();
        let ordinal = v.get("ordinal").and_then(Value::as_u64)?;
        let kind = v.get("kind").and_then(Value::as_str).map(str::to_string);
        let phase = v
            .get("phase")
            .and_then(Value::as_str)
            .unwrap_or("queued")
            .to_string();
        let outcome = v.get("outcome").and_then(Value::as_str).map(str::to_string);
        let cached = v.get("cached").and_then(Value::as_bool);
        let actor_site_id = v
            .get("actorSiteId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let actor_ordinal = v.get("actorOrdinal").and_then(Value::as_u64);
        let phase_name = v
            .get("phaseName")
            .and_then(Value::as_str)
            .map(str::to_string);
        let instructions_head = v
            .get("instructionsHead")
            .and_then(Value::as_str)
            .map(str::to_string);
        let turn = v.get("turn").and_then(Value::as_u64);
        let tool_calls = v.get("toolCalls").and_then(Value::as_u64);
        let last_tool = v
            .get("lastTool")
            .and_then(WorkflowRunNodeLastTool::from_value);
        Some(Self {
            site_id,
            ordinal,
            kind,
            phase,
            outcome,
            cached,
            actor_site_id,
            actor_ordinal,
            phase_name,
            instructions_head,
            turn,
            tool_calls,
            last_tool,
        })
    }
}

impl WorkflowRunNodeLastTool {
    pub fn from_value(v: &Value) -> Option<Self> {
        let name = v.get("name").and_then(Value::as_str)?.to_string();
        let target = v.get("target").and_then(Value::as_str).map(str::to_string);
        Some(Self { name, target })
    }
}
