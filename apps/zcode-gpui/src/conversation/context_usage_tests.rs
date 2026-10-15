use super::*;
use serde_json::json;

fn usage(cw: Value) -> Value {
    json!({ "contextWindow": cw, "cumulative": { "inputTokens": 0, "outputTokens": 0,
            "cacheReadTokens": 0, "cacheWriteTokens": 0 } })
}

#[test]
fn hidden_without_a_usable_window() {
    assert_eq!(ContextWindow::from_usage(&usage(Value::Null)), None);
    assert_eq!(ContextWindow::from_usage(&json!({})), None);
    for (used, max) in [(0, 200_000), (100, 0), (-5, 200_000)] {
        let cw = json!({ "usedTokens": used, "maxTokens": max,
                         "autoCompactThresholdTokens": null });
        assert_eq!(ContextWindow::from_usage(&usage(cw)), None, "{used}/{max}");
    }
}

#[test]
fn summary_matches_desktop_format() {
    let cw = ContextWindow::from_usage(&usage(json!({
        "usedTokens": 12_345, "maxTokens": 200_000, "autoCompactThresholdTokens": 180_000
    })))
    .unwrap();
    assert_eq!(cw.summary(false), "12.3K/200K (6.2%)");
    assert_eq!(cw.summary(true), "1.2万/20万 (6.2%)");
    assert!((cw.fraction() - 0.061725).abs() < 1e-9);

    let over = ContextWindow {
        used: 300.,
        max: 200.,
        cache_hit_rate: None,
        segments: vec![],
    };
    assert_eq!(over.fraction(), 1.);
    assert_eq!(over.summary(false), "300/200 (100%)");
}

#[test]
fn compact_numbers() {
    assert_eq!(compact(999., 1, false), "999");
    assert_eq!(compact(1_000., 1, false), "1K");
    assert_eq!(compact(1_234_567., 1, false), "1.2M");
    assert_eq!(compact(999_950., 1, false), "1M");
    assert_eq!(compact(1_000_000., 0, false), "1M");
    assert_eq!(compact(5_000., 1, true), "5000");
    assert_eq!(compact(250_000_000., 0, true), "3亿");
}

#[test]
fn breakdown_merges_sorts_and_drops_invalid() {
    let cw = ContextWindow::from_usage(&usage(json!({
        "usedTokens": 1000, "maxTokens": 2000, "autoCompactThresholdTokens": null,
        "breakdown": [
            { "source": "skills", "chars": 100 },
            { "source": "messages", "chars": 500 },
            { "source": "system_prompt", "chars": 100 },
            { "source": "messages", "chars": 200 },
            { "source": "tool_prompt", "chars": 0 },
            { "source": "unknown", "chars": 50 },
        ]
    })))
    .unwrap();
    let order: Vec<_> = cw.segments.iter().map(|s| s.source).collect();
    // Equal sizes keep Desktop's source order (system prompt before skills).
    assert_eq!(order, ["messages", "system_prompt", "skills"]);
    assert_eq!(cw.segments[0].chars, 700.);
    assert!((cw.segments[0].share - 0.7778).abs() < 1e-3);
    assert_eq!(percent(cw.segments[0].share), "77.8%");
}

#[test]
fn cache_rate_only_above_threshold() {
    let with_rate = |rate: f64| {
        ContextWindow::from_usage(&usage(json!({
            "usedTokens": 1, "maxTokens": 2, "autoCompactThresholdTokens": null,
            "cache": { "hitRate": rate }
        })))
        .unwrap()
    };
    assert_eq!(with_rate(0.5).cache_label(), None);
    assert_eq!(with_rate(0.78).cache_label().as_deref(), Some("78%"));
    assert_eq!(with_rate(0.9234).cache_label().as_deref(), Some("92.3%"));
}

#[test]
fn usage_patches_replace_the_conversation_window() {
    let mut conv = crate::conversation::model::ConversationState::default();
    let patch = |cw: Value| json!([{ "op": "state.updated", "patch": { "usage": usage(cw) } }]);
    conv.apply_deltas(
        patch(json!({ "usedTokens": 50_000, "maxTokens": 200_000,
                      "autoCompactThresholdTokens": null }))
        .as_array()
        .unwrap(),
    );
    assert_eq!(conv.context_window.as_ref().map(|c| c.used), Some(50_000.));
    conv.apply_deltas(patch(Value::Null).as_array().unwrap());
    assert_eq!(conv.context_window, None);
}
