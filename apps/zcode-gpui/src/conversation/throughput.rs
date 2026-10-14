//! Output tokens per second for the composer meter.
//!
//! The backend only reports usage when a model response completes
//! (`state.updated` `usage.cumulative.outputTokens` on ModelComplete), so
//! while a response streams the rate is estimated from streamed characters
//! (~4 per token). At completion the exact output-token delta over the
//! streaming time replaces the estimate.

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
        if let (Some(prev), Some(start)) = (self.cumulative_output, self.stream_start) {
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
