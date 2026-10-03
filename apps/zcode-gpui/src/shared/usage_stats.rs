//! Usage statistics data models and deserialization for v4/usage/stats.
//!
//! Spec source: packages/shared/src/usage-stats.ts (appUsageSnapshotSchema).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AppUsageSnapshot {
    pub range: String,
    pub summary: AppUsageSummary,
    pub daily: Vec<AppUsageDailyItem>,
    pub models: Vec<AppUsageModelItem>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AppUsageSummary {
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
    pub cache_hit_rate: f64,
    pub total_sessions: u64,
    pub total_turns: u64,
    pub tool_call_count: u64,
    pub favorite_model: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AppUsageDailyItem {
    pub date: String,
    pub total_tokens: u64,
    pub turn_count: u64,
    pub tool_call_count: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AppUsageModelItem {
    pub model_id: String,
    pub total_tokens: u64,
    pub share: f64,
}

impl AppUsageSnapshot {
    pub fn from_value(v: &Value, range: &str) -> Self {
        let summary = v
            .get("summary")
            .map(AppUsageSummary::from_value)
            .unwrap_or_default();
        let daily = v
            .get("daily")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(AppUsageDailyItem::from_value)
                    .collect()
            })
            .unwrap_or_default();
        let models = v
            .get("models")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(AppUsageModelItem::from_value)
                    .collect()
            })
            .unwrap_or_default();

        Self {
            range: range.to_string(),
            summary,
            daily,
            models,
        }
    }
}

impl AppUsageSummary {
    pub fn from_value(v: &Value) -> Self {
        let favorite_model = v
            .get("favoriteModel")
            .and_then(|m| m.get("modelId"))
            .and_then(Value::as_str)
            .map(str::to_string);

        Self {
            total_tokens: v.get("totalTokens").and_then(Value::as_u64).unwrap_or(0),
            input_tokens: v.get("inputTokens").and_then(Value::as_u64).unwrap_or(0),
            output_tokens: v.get("outputTokens").and_then(Value::as_u64).unwrap_or(0),
            reasoning_tokens: v
                .get("reasoningTokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            cache_hit_rate: v.get("cacheHitRate").and_then(Value::as_f64).unwrap_or(0.0),
            total_sessions: v.get("totalSessions").and_then(Value::as_u64).unwrap_or(0),
            total_turns: v.get("totalTurns").and_then(Value::as_u64).unwrap_or(0),
            tool_call_count: v.get("toolCallCount").and_then(Value::as_u64).unwrap_or(0),
            favorite_model,
        }
    }
}

impl AppUsageDailyItem {
    pub fn from_value(v: &Value) -> Option<Self> {
        let date = v.get("date").and_then(Value::as_str)?.to_string();
        let total_tokens = v.get("totalTokens").and_then(Value::as_u64).unwrap_or(0);
        let turn_count = v.get("turnCount").and_then(Value::as_u64).unwrap_or(0);
        let tool_call_count = v.get("toolCallCount").and_then(Value::as_u64).unwrap_or(0);
        Some(Self {
            date,
            total_tokens,
            turn_count,
            tool_call_count,
        })
    }
}

impl AppUsageModelItem {
    pub fn from_value(v: &Value) -> Option<Self> {
        let model_id = v
            .get("modelId")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let total_tokens = v.get("totalTokens").and_then(Value::as_u64).unwrap_or(0);
        let share = v.get("share").and_then(Value::as_f64).unwrap_or(0.0);
        Some(Self {
            model_id,
            total_tokens,
            share,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_app_usage_snapshot_parsing() {
        let val = json!({
            "summary": {
                "totalTokens": 150000,
                "inputTokens": 100000,
                "outputTokens": 50000,
                "reasoningTokens": 10000,
                "cacheHitRate": 0.35,
                "totalSessions": 12,
                "totalTurns": 45,
                "toolCallCount": 88,
                "favoriteModel": {
                    "modelId": "claude-3-7-sonnet",
                    "totalTokens": 120000,
                    "share": 0.8
                }
            },
            "daily": [
                {
                    "date": "2026-10-01",
                    "totalTokens": 80000,
                    "turnCount": 20,
                    "toolCallCount": 40
                }
            ],
            "models": [
                {
                    "modelId": "claude-3-7-sonnet",
                    "totalTokens": 120000,
                    "share": 0.8
                }
            ]
        });

        let snapshot = AppUsageSnapshot::from_value(&val, "7d");
        assert_eq!(snapshot.range, "7d");
        assert_eq!(snapshot.summary.total_tokens, 150000);
        assert_eq!(snapshot.summary.cache_hit_rate, 0.35);
        assert_eq!(
            snapshot.summary.favorite_model.as_deref(),
            Some("claude-3-7-sonnet")
        );
        assert_eq!(snapshot.daily.len(), 1);
        assert_eq!(snapshot.daily[0].date, "2026-10-01");
        assert_eq!(snapshot.models.len(), 1);
    }
}
