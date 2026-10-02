//! Pending interactions (permission requests, agent questions) mirrored from
//! the V4 conversation `pendingInteractions` region, and the
//! `resolveInteraction` answer shapes.
//!
//! Spec source: packages/shared/src/zcode-protocol-v4/snapshot.ts
//! (`pendingInteractionSchema`), command.ts (`resolveInteraction`), and the
//! answer mapping in apps/zcode-cli/packages/bootstrap/src/zcode-protocol/
//! interaction-broker.ts.
//!
//! Only the V4 command is used to answer. The backend also sends legacy
//! `interaction/requestPermission` / `requestUserInput` stdio requests and
//! races them against the V4 answer; the V4 answer cancels them.

use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// `PERMISSION_FULL_ACCESS_OPTION_ID` in snapshot.ts.
pub const FULL_ACCESS_OPTION_ID: &str = "fullAccess";

#[derive(Clone, Debug, PartialEq)]
pub struct InteractionOption {
    pub option_id: String,
    pub label: String,
    /// allowOnce | allowAlways | deny | custom (permission); empty otherwise.
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    pub question: String,
    pub header: String,
    /// Option labels; the broker matches answers by label.
    pub options: Vec<(String, String)>,
    pub multi_select: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingInteraction {
    pub interaction_id: String,
    /// permission | userInput | workspaceHookReview
    pub kind: String,
    pub tool_name: String,
    /// Permission summary or user-input prompt.
    pub text: String,
    pub options: Vec<InteractionOption>,
    pub full_access: Option<InteractionOption>,
    pub questions: Vec<Question>,
    pub free_text: bool,
}

fn str_of(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn parse_option(o: &Value) -> Option<InteractionOption> {
    Some(InteractionOption {
        option_id: o.get("optionId")?.as_str()?.to_string(),
        label: o.get("label")?.as_str()?.to_string(),
        kind: str_of(o, "kind"),
    })
}

impl PendingInteraction {
    pub fn from_value(v: &Value) -> Option<Self> {
        let interaction_id = v.get("interactionId")?.as_str()?.to_string();
        let payload = v.get("payload")?;
        let kind = v
            .get("kind")
            .or_else(|| payload.get("kind"))
            .and_then(Value::as_str)?
            .to_string();
        let text = match kind.as_str() {
            "permission" => str_of(payload, "summary"),
            _ => str_of(payload, "prompt"),
        };
        let options = payload
            .get("options")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(parse_option).collect())
            .unwrap_or_default();
        let questions = payload
            .get("questions")
            .and_then(Value::as_array)
            .map(|qs| {
                qs.iter()
                    .map(|q| Question {
                        question: str_of(q, "question"),
                        header: str_of(q, "header"),
                        options: q
                            .get("options")
                            .and_then(Value::as_array)
                            .map(|os| {
                                os.iter()
                                    .map(|o| (str_of(o, "label"), str_of(o, "description")))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        multi_select: q
                            .get("multiSelect")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            interaction_id,
            tool_name: str_of(payload, "toolName"),
            text,
            options,
            full_access: payload.get("fullAccessOption").and_then(parse_option),
            questions,
            free_text: payload
                .get("freeText")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            kind,
        })
    }

    /// The option whose answer means "deny" (used for deny-with-feedback).
    pub fn deny_option(&self) -> Option<&InteractionOption> {
        self.options.iter().find(|o| o.kind == "deny")
    }
}

/// Click on a listed option (permission options, simple user-input choices,
/// and Full access).
pub fn option_answer(option_id: &str) -> Value {
    json!({ "optionId": option_id })
}

/// Free text, optionally attached to an option (deny with feedback).
pub fn free_text_answer(option_id: Option<&str>, text: &str) -> Value {
    let mut answer = json!({ "freeText": text });
    if let Some(id) = option_id {
        answer["optionId"] = json!(id);
    }
    answer
}

/// Multi-question AskUserQuestion answer: `content.answers` keyed by the
/// question text, values are the picked option labels (comma-joined, like
/// the broker's `normalizeAnswerValue`).
pub fn questions_answer(questions: &[Question], picks: &HashMap<usize, Vec<String>>) -> Value {
    let mut answers = Map::new();
    for (i, q) in questions.iter().enumerate() {
        if let Some(labels) = picks.get(&i).filter(|l| !l.is_empty()) {
            answers.insert(q.question.clone(), json!(labels.join(", ")));
        }
    }
    json!({ "action": "accept", "content": { "answers": answers } })
}

pub fn decline_answer() -> Value {
    json!({ "action": "decline" })
}

/// Toggle a pick: single-select replaces, multi-select toggles membership.
pub fn toggle_pick(picks: &mut Vec<String>, label: &str, multi: bool) {
    if multi {
        if let Some(pos) = picks.iter().position(|p| p == label) {
            picks.remove(pos);
        } else {
            picks.push(label.to_string());
        }
    } else {
        *picks = vec![label.to_string()];
    }
}

#[cfg(test)]
#[path = "interactions_tests.rs"]
mod tests;
