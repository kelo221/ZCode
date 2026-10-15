//! Output tokens per second for the composer meter.
//!
//! The backend only reports usage when a model response completes
//! (`state.updated` `usage.cumulative.outputTokens` on ModelComplete), so
//! while a response streams the rate is estimated from streamed characters
//! (~4 per token). At completion the exact output-token delta over the
//! generation time replaces the estimate.
//!
//! Output tokens include thinking, which often streams no text (Claude sends
//! signature-only thinking), so the generation window starts at the earliest
//! of the first visible delta and the start of the response's reasoning
//! (reasoning row `durationMs` back from when the row reported it).

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
    /// Start of the exact-rate window; `None` until this response produced
    /// visible output or a timed reasoning row.
    window_start: Option<Instant>,
    /// Highest reasoning row already counted, so a later upsert of an older
    /// row can't stretch the window back over tool time.
    last_reasoning_row: Option<u64>,
    cumulative_output: Option<u64>,
    last: Option<Rate>,
}

impl Throughput {
    /// A `row.delta` append arrived.
    pub fn on_stream(&mut self, appended: &str, now: Instant) {
        if appended.is_empty() {
            return;
        }
        self.stream_start.get_or_insert(now);
        self.widen_window(now);
        self.streamed_chars += appended.chars().count();
    }

    /// A reasoning row with a measured duration arrived (append or upsert).
    pub fn on_reasoning(&mut self, row_id: u64, duration_ms: u64, now: Instant) {
        if self.last_reasoning_row.is_some_and(|last| row_id <= last) {
            return;
        }
        self.last_reasoning_row = Some(row_id);
        let started = now
            .checked_sub(std::time::Duration::from_millis(duration_ms))
            .unwrap_or(now);
        self.widen_window(started);
    }

    fn widen_window(&mut self, start: Instant) {
        self.window_start = Some(self.window_start.map_or(start, |s| s.min(start)));
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
    fn hidden_thinking_time_counts_toward_the_exact_rate() {
        // 20 s of signature-only thinking, then a short sentence: 1500 tokens
        // over 0.5 s of visible text read as 3000 tok/s before.
        let t0 = Instant::now();
        let mut t = Throughput::default();
        t.on_usage(&usage(0), t0);
        let thought = t0 + Duration::from_secs(20);
        t.on_reasoning(7, 20_000, thought);
        t.on_stream("Done.", thought);
        t.on_usage(&usage(1500), thought + Duration::from_millis(500));
        let done = t.rate(thought + Duration::from_secs(1)).unwrap();
        assert!((done.tokens_per_sec - 1500. / 20.5).abs() < 0.01);

        // An older reasoning row upserted again later doesn't stretch the next window.
        let t1 = thought + Duration::from_secs(60);
        t.on_reasoning(7, 20_000, t1);
        t.on_stream(&"x".repeat(40), t1);
        t.on_usage(&usage(1600), t1 + Duration::from_secs(1));
        assert_eq!(
            t.rate(t1 + Duration::from_secs(1)).unwrap().tokens_per_sec,
            100.
        );
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
