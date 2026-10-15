//! Context-window usage for the composer ring (`usage.contextWindow`).
//! Desktop parity: packages/ui/src/chat-input-toolbar/contextUsage.tsx.

use serde_json::Value;

/// Production shows the cache hit rate only when caching clearly pays off.
pub const CACHE_HIT_RATE_DISPLAY_THRESHOLD: f64 = 0.78;

/// Breakdown sources in Desktop's tie-break order, with their labels.
const SOURCES: [(&str, &str, &str); 7] = [
    ("messages", "Messages", "消息"),
    ("system_prompt", "System prompt", "系统提示词"),
    ("meta_user_context", "Meta context", "其他"),
    ("skills", "Skills", "技能"),
    ("tool_prompt", "Tool prompt", "工具提示词"),
    ("system_tool_schemas", "System tools", "系统工具"),
    ("mcp_tool_schemas", "MCP tools", "MCP 工具"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub source: &'static str,
    pub label_en: &'static str,
    pub label_zh: &'static str,
    pub chars: f64,
    /// Share of all breakdown characters, 0..=1.
    pub share: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContextWindow {
    pub used: f64,
    pub max: f64,
    pub cache_hit_rate: Option<f64>,
    pub segments: Vec<Segment>,
}

impl ContextWindow {
    /// The renderable context window of a `usage` region. `None` when it is
    /// null, or zero or invalid (used=0 is an initial value, not 0% usage).
    pub fn from_usage(usage: &Value) -> Option<Self> {
        let cw = usage.get("contextWindow")?;
        let used = cw.get("usedTokens")?.as_f64()?;
        let max = cw.get("maxTokens")?.as_f64()?;
        if !used.is_finite() || !max.is_finite() || used <= 0. || max <= 0. {
            return None;
        }
        Some(Self {
            used,
            max,
            cache_hit_rate: cw
                .pointer("/cache/hitRate")
                .and_then(Value::as_f64)
                .filter(|r| r.is_finite()),
            segments: segments(cw.get("breakdown")),
        })
    }

    pub fn fraction(&self) -> f64 {
        (self.used / self.max).clamp(0., 1.)
    }

    /// "12.3K/200K (6.2%)"; Chinese uses 万/亿 like Intl's compact notation.
    pub fn summary(&self, zh: bool) -> String {
        format!(
            "{}/{} ({})",
            compact(self.used, 1, zh),
            compact(self.max, 0, zh),
            percent(self.fraction())
        )
    }

    pub fn cache_label(&self) -> Option<String> {
        self.cache_hit_rate
            .filter(|r| *r >= CACHE_HIT_RATE_DISPLAY_THRESHOLD)
            .map(|r| percent(r.max(0.)))
    }
}

/// Merged per-source characters, largest first, ties in Desktop's order.
fn segments(breakdown: Option<&Value>) -> Vec<Segment> {
    let mut chars = [0f64; SOURCES.len()];
    for item in breakdown.and_then(Value::as_array).into_iter().flatten() {
        let Some(c) = item.get("chars").and_then(Value::as_f64) else {
            continue;
        };
        let source = item.get("source").and_then(Value::as_str);
        if let Some(i) = SOURCES.iter().position(|(s, ..)| Some(*s) == source)
            && c.is_finite()
            && c > 0.
        {
            chars[i] += c;
        }
    }
    let total: f64 = chars.iter().sum();
    if total <= 0. {
        return Vec::new();
    }
    let mut out: Vec<Segment> = SOURCES
        .iter()
        .zip(chars)
        .filter(|(_, c)| *c > 0.)
        .map(|(&(source, label_en, label_zh), chars)| Segment {
            source,
            label_en,
            label_zh,
            chars,
            share: chars / total,
        })
        .collect();
    // Stable sort keeps the source order for equal sizes.
    out.sort_by(|a, b| b.chars.total_cmp(&a.chars));
    out
}

/// Intl-style percent with at most one decimal: 0.0625 -> "6.3%", 0.5 -> "50%".
pub fn percent(fraction: f64) -> String {
    trim(fraction * 100., 1) + "%"
}

/// Intl compact notation: K/M/B in English, 万/亿 in Chinese, plain below 1000.
pub fn compact(value: f64, max_fraction: usize, zh: bool) -> String {
    let abs = value.abs();
    let units: &[(f64, &str)] = if zh {
        &[(1e8, "亿"), (1e4, "万")]
    } else {
        &[(1e9, "B"), (1e6, "M"), (1e3, "K")]
    };
    if abs >= 1e3 {
        let scale = 10f64.powi(max_fraction as i32);
        for &(size, unit) in units {
            // Rounded, so 999_950 reads "1M" rather than "1000K".
            if (abs / size * scale).round() >= scale {
                return trim(value / size, max_fraction) + unit;
            }
        }
    }
    trim(value, max_fraction)
}

fn trim(value: f64, max_fraction: usize) -> String {
    // Intl rounds halves away from zero; `format!` alone rounds 2.5 to "2".
    let scale = 10f64.powi(max_fraction as i32);
    let value = (value * scale).round() / scale;
    let s = format!("{value:.max_fraction$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

#[cfg(test)]
#[path = "context_usage_tests.rs"]
mod tests;
