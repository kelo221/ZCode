use super::*;
use serde_json::json;
use std::time::Duration;

fn usage(out: u64) -> Value {
    json!({ "cumulative": { "inputTokens": 0, "outputTokens": out,
                            "cacheReadTokens": 0, "cacheWriteTokens": 0 } })
}

fn secs(s: f32) -> Duration {
    Duration::from_secs_f32(s)
}

#[test]
fn estimate_while_streaming_then_exact_on_completion() {
    let t0 = Instant::now();
    let mut t = Throughput::default();
    t.on_usage(&usage(1000), t0);
    assert_eq!(t.rate(t0), None, "baseline only");

    t.on_call_boundary(t0); // the user's message starts the turn
    t.on_stream(&"x".repeat(400), t0 + secs(1.));
    let live = t.rate(t0 + secs(2.)).unwrap();
    assert!(!live.exact);
    assert_eq!(live.tokens_per_sec, 50.);
    assert_eq!(format_rate(live), "~50 tok/s");

    t.on_usage(&usage(1300), t0 + secs(3.));
    let done = t.rate(t0 + secs(10.)).unwrap();
    assert!(done.exact);
    assert_eq!(done.tokens_per_sec, 100.);
    assert_eq!(format_rate(done), "100 tok/s");
}

#[test]
fn a_silent_wait_then_a_burst_counts_the_whole_call() {
    // From this session's `model_usage`: 8.6 s before the first token, then
    // 1157 output tokens in 4.3 s. Timed from the first token it read 272
    // tok/s; the whole request ran at about 90.
    let t0 = Instant::now();
    let mut t = Throughput::default();
    t.on_usage(&usage(0), t0);
    t.on_call_boundary(t0);
    let first = t0 + secs(8.6);
    t.on_model_row();
    t.on_stream(&"x".repeat(4_000), first);
    t.on_usage(&usage(1157), first + secs(4.3));
    let done = t.rate(first + secs(5.)).unwrap();
    assert!((done.tokens_per_sec - 1157. / 12.9).abs() < 0.1, "{done:?}");
}

#[test]
fn tool_runtime_stays_out_and_early_tool_results_dont_shorten_the_call() {
    let t0 = Instant::now();
    let mut t = Throughput::default();
    t.on_usage(&usage(0), t0);
    t.on_call_boundary(t0);
    t.on_model_row();
    // A read finishing while the model still streams belongs to this call.
    t.on_call_boundary(t0 + secs(3.));
    t.on_usage(&usage(400), t0 + secs(4.));
    assert_eq!(t.rate(t0 + secs(4.)).unwrap().tokens_per_sec, 100.);

    // A 47 s bash command runs after that call; the next request goes out
    // when it finishes.
    let done = t0 + secs(51.);
    t.on_call_boundary(done);
    t.on_model_row();
    t.on_usage(&usage(600), done + secs(5.));
    assert_eq!(t.rate(done + secs(5.)).unwrap().tokens_per_sec, 40.);

    // No tools: the next call starts at the previous usage event.
    t.on_model_row();
    t.on_usage(&usage(700), done + secs(7.));
    assert_eq!(t.rate(done + secs(7.)).unwrap().tokens_per_sec, 50.);
}

#[test]
fn unchanged_usage_is_not_a_call_boundary() {
    let t0 = Instant::now();
    let mut t = Throughput::default();
    t.on_usage(&usage(100), t0);
    t.on_call_boundary(t0);
    t.on_model_row();
    // Model switch or context-window patch mid-call.
    t.on_usage(&usage(100), t0 + secs(2.));
    t.on_usage(&usage(300), t0 + secs(4.));
    assert_eq!(t.rate(t0 + secs(4.)).unwrap().tokens_per_sec, 50.);
}

#[test]
fn the_first_usage_after_load_opens_no_window() {
    let t0 = Instant::now();
    let mut t = Throughput::default();
    // Snapshot taken mid-call: its start is unknown.
    t.on_usage(&usage(10), t0);
    t.on_model_row();
    t.on_usage(&usage(50), t0 + secs(1.));
    assert_eq!(t.rate(t0 + secs(1.)), None);
    t.on_stream("", t0);
    assert_eq!(t.rate(t0 + secs(5.)), None, "empty append ignored");
}

#[test]
fn model_output_rows_boundaries_and_paths() {
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
    assert!(is_call_boundary_row(true, &json!({ "kind": "turnHeader" })));
    assert!(is_call_boundary_row(true, &json!({ "kind": "userInput" })));
    assert!(!is_call_boundary_row(
        false,
        &json!({ "kind": "turnHeader" })
    ));
    for status in ["success", "error", "cancelled"] {
        let row = json!({ "kind": "toolCall", "status": status });
        assert!(is_call_boundary_row(false, &row), "{status}");
    }
    for status in ["inputStreaming", "pendingApproval", "running"] {
        let row = json!({ "kind": "toolCall", "status": status });
        assert!(!is_call_boundary_row(false, &row), "{status}");
    }
    assert!(is_model_output_path("text"));
    assert!(is_model_output_path("inputText"));
    assert!(!is_model_output_path("output.text"), "bash stdout");
    assert!(!is_model_output_path("summaryText"));
}
