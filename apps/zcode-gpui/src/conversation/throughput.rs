//! Output tokens per second for the composer meter.
//!
//! The backend reports usage once per main-agent model call
//! (`state.updated` `usage.cumulative.outputTokens` on ModelComplete), so
//! while a response streams the rate is estimated from streamed characters
//! (~4 per token). At completion the exact output-token delta over the
//! call's duration replaces the estimate.
//!
//! 修复原因：窗口曾从本次调用第一个模型输出行起算。经代理的 Claude 模型常先静默 6–16 s
//! （隐藏思考或代理缓冲），再在约 1 s 内吐出整段回答，界面因此显示 200–1200 tok/s，而
//! `model_usage` 表中同一调用按整次请求计时只有 40–90 tok/s。协议没有“请求已发出”事件，
//! 所以起点取以下最晚者：turn 开始（追加 turnHeader/userInput 行）、上一次调用的 usage
//! 事件、以及本次调用首个输出之前结束的工具调用。工具执行时间（长 bash、子代理）因此
//! 不计入窗口；模型仍在流式输出时提前完成的工具也不会把起点后移。

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
    /// When the current call's request went out (see module docs). `None`
    /// until a boundary was observed, e.g. right after load.
    call_start: Option<Instant>,
    /// The current call produced output; later tool completions belong to
    /// it and no longer move `call_start`.
    output_seen: bool,
    stream_start: Option<Instant>,
    streamed_chars: usize,
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

/// Whether a row event means the next model request is about to be sent:
/// a new turn, or a tool call finishing (its result goes into that request).
pub fn is_call_boundary_row(appended: bool, row: &Value) -> bool {
    match row.get("kind").and_then(Value::as_str) {
        Some("turnHeader" | "userInput") => appended,
        Some("toolCall") => matches!(
            row.get("status").and_then(Value::as_str),
            Some("success" | "error" | "cancelled")
        ),
        _ => false,
    }
}

/// Whether a `row.delta` path carries this session's model output. Tool
/// output (`output.text`, e.g. a bash command's stdout) and subagent
/// summaries (`summaryText`) are not the parent model writing.
pub fn is_model_output_path(path: &str) -> bool {
    matches!(path, "text" | "inputText")
}

impl Throughput {
    /// See [`is_call_boundary_row`].
    pub fn on_call_boundary(&mut self, now: Instant) {
        if !self.output_seen {
            self.call_start = Some(now);
        }
    }

    /// A model output row was appended (see [`is_model_output_row`]).
    pub fn on_model_row(&mut self) {
        self.output_seen = true;
    }

    /// A model output `row.delta` append arrived.
    pub fn on_stream(&mut self, appended: &str, now: Instant) {
        if appended.is_empty() {
            return;
        }
        self.output_seen = true;
        self.stream_start.get_or_insert(now);
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
        let Some(prev) = self.cumulative_output else {
            // Baseline from a snapshot: a call may already be running, so its
            // start is unknown and no window opens until the next boundary.
            self.cumulative_output = Some(total);
            return;
        };
        if total == prev {
            // Model or context-window change, not the end of a call.
            return;
        }
        if let Some(start) = self.call_start {
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
        // The next call is sent right after this one, unless tools run first.
        self.call_start = Some(now);
        self.output_seen = false;
        self.stream_start = None;
        self.streamed_chars = 0;
    }

    /// Live estimate while streaming, else the last completed response's rate.
    pub fn rate(&self, now: Instant) -> Option<Rate> {
        if let Some(streaming) = self.stream_start {
            let start = self.call_start.unwrap_or(streaming);
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
#[path = "throughput_tests.rs"]
mod tests;
