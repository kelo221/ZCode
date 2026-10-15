//! Output tokens per second for the composer meter.
//!
//! The backend reports usage once per main-agent model call
//! (`state.updated` `usage.cumulative.outputTokens` on ModelComplete), so
//! while a response streams the rate is estimated from streamed characters
//! (~4 per token). At completion the exact output-token delta over the
//! generation time replaces the estimate.
//!
//! 修复原因：输出 token 包含不流出文本的思考（Claude 只发签名）与工具参数。旧窗口从第一个
//! 可见增量（常是调用末尾的工具参数）或推理行的 `durationMs` 起算，而 CLI 从不下发
//! `durationMs`，几秒的调用被当成零点几秒（界面显示 1473 tok/s，同一会话 model-io 日志里
//! 真实速率为 52–125 tok/s）。现在窗口从本次调用第一个模型输出行出现时起算：CLI 在
//! `reasoning_start` 就追加 streaming 推理行（即使没有文本），在 `tool_input_start` 追加
//! `inputStreaming` 工具行，在 `text_start` 追加 streaming 正文行。

use serde_json::Value;
use std::time::Instant;

const CHARS_PER_TOKEN: f32 = 4.0;
/// Shorter windows give meaningless rates (cached or tool-only responses).
const MIN_WINDOW_SECS: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rate {
    pub tokens_per_sec: f32,
    /// `false` while it is a character-based estimate.
    pub exact: bool,
}

#[derive(Debug, Default)]
pub struct Throughput {
    stream_start: Option<Instant>,
    streamed_chars: usize,
    /// Start of the exact-rate window: the first model output of the current
    /// call. `None` until one appeared.
    window_start: Option<Instant>,
    cumulative_output: Option<u64>,
    last: Option<Rate>,
}

/// Whether an appended row is the model starting to produce output: a
/// streaming reasoning or text row, or a tool call whose input streams.
pub fn is_model_output_row(row: &Value) -> bool {
    match row.get("kind").and_then(Value::as_str) {
        Some("reasoning" | "assistantText") => {
            row.get("state").and_then(Value::as_str) == Some("streaming")
        }
        Some("toolCall") => row.get("status").and_then(Value::as_str) == Some("inputStreaming"),
        _ => false,
    }
}

/// Whether a `row.delta` path carries this session's model output. Subagent
/// summaries (`summaryText`) stream child output the parent's usage omits.
pub fn is_model_output_path(path: &str) -> bool {
    matches!(path, "text" | "inputText")
}

impl Throughput {
    /// A model output row was appended (see [`is_model_output_row`]).
    pub fn on_model_row(&mut self, now: Instant) {
        self.window_start.get_or_insert(now);
    }

    /// A model output `row.delta` append arrived.
    pub fn on_stream(&mut self, appended: &str, now: Instant) {
        if appended.is_empty() {
            return;
        }
        self.stream_start.get_or_insert(now);
        self.window_start.get_or_insert(now);
        self.streamed_chars += appended.chars().count();
    }

    /// A `usage` region arrived (snapshot or patch).
    pub fn on_usage(&mut self, usage: &Value, now: Instant) {
        let Some(total) = usage
            .pointer("/cumulative/outputTokens")
            .and_then(Value::as_f64)
            .map(|t| t as u64)
        else {
            return;
        };
        if let (Some(prev), Some(start)) = (self.cumulative_output, self.window_start) {
            let secs = now.duration_since(start).as_secs_f32();
            let tokens = total.saturating_sub(prev);
            if secs >= MIN_WINDOW_SECS && tokens > 0 {
                self.last = Some(Rate {
                    tokens_per_sec: tokens as f32 / secs,
                    exact: true,
                });
            }
        }
        self.cumulative_output = Some(total);
        self.stream_start = None;
        self.window_start = None;
        self.streamed_chars = 0;
    }

    /// Live estimate while streaming, else the last completed response's rate.
    pub fn rate(&self, now: Instant) -> Option<Rate> {
        if let Some(start) = self.stream_start {
            let secs = now.duration_since(start).as_secs_f32();
            if secs >= MIN_WINDOW_SECS {
                return Some(Rate {
                    tokens_per_sec: self.streamed_chars as f32 / CHARS_PER_TOKEN / secs,
                    exact: false,
                });
            }
        }
        self.last
    }
}

pub fn format_rate(rate: Rate) -> String {
    let prefix = if rate.exact { "" } else { "~" };
    format!("{prefix}{:.0} tok/s", rate.tokens_per_sec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn usage(out: u64) -> Value {
        json!({ "cumulative": { "inputTokens": 0, "outputTokens": out,
                                "cacheReadTokens": 0, "cacheWriteTokens": 0 } })
    }

    #[test]
    fn estimate_while_streaming_then_exact_on_completion() {
        let t0 = Instant::now();
        let mut t = Throughput::default();
        t.on_usage(&usage(1000), t0);
        assert_eq!(t.rate(t0), None, "baseline only");

        t.on_stream(&"x".repeat(400), t0);
        let live = t.rate(t0 + Duration::from_secs(2)).unwrap();
        assert!(!live.exact);
        assert_eq!(live.tokens_per_sec, 50.);
        assert_eq!(format_rate(live), "~50 tok/s");

        t.on_usage(&usage(1300), t0 + Duration::from_secs(3));
        let done = t.rate(t0 + Duration::from_secs(10)).unwrap();
        assert!(done.exact);
        assert_eq!(done.tokens_per_sec, 100.);
        assert_eq!(format_rate(done), "100 tok/s");
    }

    #[test]
    fn hidden_thinking_and_tool_input_use_the_whole_call() {
        // The pattern from this session's model-io log: 6 s of signature-only
        // thinking, then 400 tokens of tool input in the last 0.3 s. Timed
        // from the first tool-input delta it read as ~1300 tok/s.
        let t0 = Instant::now();
        let mut t = Throughput::default();
        t.on_usage(&usage(0), t0);
        t.on_model_row(t0); // streaming reasoning row, no text
        let tool = t0 + Duration::from_millis(5_700);
        t.on_model_row(tool); // inputStreaming tool row
        t.on_stream(&"x".repeat(1_200), tool);
        t.on_usage(&usage(400), t0 + Duration::from_secs(6));
        let done = t.rate(t0 + Duration::from_secs(7)).unwrap();
        assert!(done.exact);
        assert!((done.tokens_per_sec - 400. / 6.).abs() < 0.01);

        // The tool runs between calls; the next call's window starts at its
        // own first output row, not at the previous usage event.
        let next = t0 + Duration::from_secs(30);
        t.on_model_row(next);
        t.on_usage(&usage(500), next + Duration::from_secs(2));
        assert_eq!(
            t.rate(next + Duration::from_secs(2))
                .unwrap()
                .tokens_per_sec,
            50.
        );
    }

    #[test]
    fn model_output_rows_and_paths() {
        for row in [
            json!({ "kind": "reasoning", "state": "streaming", "text": "" }),
            json!({ "kind": "toolCall", "status": "inputStreaming" }),
            json!({ "kind": "assistantText", "state": "streaming" }),
        ] {
            assert!(is_model_output_row(&row), "{row}");
        }
        for row in [
            json!({ "kind": "reasoning", "state": "complete" }),
            json!({ "kind": "toolCall", "status": "running" }),
            json!({ "kind": "subagent" }),
            json!({ "kind": "userInput" }),
        ] {
            assert!(!is_model_output_row(&row), "{row}");
        }
        assert!(is_model_output_path("text"));
        assert!(is_model_output_path("inputText"));
        assert!(!is_model_output_path("summaryText"));
    }

    #[test]
    fn usage_without_streaming_keeps_previous_rate() {
        let t0 = Instant::now();
        let mut t = Throughput::default();
        t.on_usage(&usage(10), t0);
        t.on_usage(&usage(50), t0 + Duration::from_secs(1));
        assert_eq!(t.rate(t0 + Duration::from_secs(1)), None);
        t.on_stream("", t0);
        assert_eq!(
            t.rate(t0 + Duration::from_secs(5)),
            None,
            "empty append ignored"
        );
    }
}
