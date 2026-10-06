use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    ok: bool,
    path: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Moved {
    ok: bool,
    from: String,
    to: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Failure {
    ok: bool,
    reason: String,
    path: Option<String>,
    detail: Option<String>,
}

pub(crate) fn validate(v: &Value, moving: bool, original: &str) -> Result<(), String> {
    if v["ok"] == false {
        let f: Failure =
            serde_json::from_value(v.clone()).map_err(|_| "Malformed workflow failure")?;
        let reasons: &[&str] = if moving {
            &[
                "invalid_name",
                "not_found",
                "target_exists",
                "read_error",
                "write_error",
            ]
        } else {
            &["invalid_name", "not_found", "parse_error", "read_error"]
        };
        if f.ok
            || !reasons.contains(&f.reason.as_str())
            || !moving && v.get("path").is_some()
            || ["path", "detail"]
                .iter()
                .any(|k| v.get(k).is_some_and(|x| !x.is_string()))
            || f.path.as_ref().is_some_and(|s| s.trim().is_empty())
        {
            return Err("Malformed workflow failure".into());
        }
        return Err(crate::shared::redact::scrub(&format!(
            "{}{}{}",
            f.reason,
            f.path.map(|p| format!(": {p}")).unwrap_or_default(),
            f.detail.map(|d| format!(": {d}")).unwrap_or_default()
        )));
    }
    if moving {
        let r: Moved =
            serde_json::from_value(v.clone()).map_err(|_| "Workflow move outcome is unknown")?;
        if !r.ok || r.from != original || r.to.trim().is_empty() || r.to == r.from {
            return Err("Workflow move outcome is unknown".into());
        }
    } else {
        let r: Written =
            serde_json::from_value(v.clone()).map_err(|_| "Workflow write outcome is unknown")?;
        if !r.ok || r.path != original || r.path.trim().is_empty() {
            return Err("Workflow write outcome is unknown".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn mutations_require_strict_matching_receipts_and_redact_failure() {
        assert!(validate(&json!({"ok":true,"path":"/a"}), false, "/a").is_ok());
        assert!(validate(&json!({"ok":true,"path":"/b"}), false, "/a").is_err());
        assert!(validate(&json!({"ok":true,"path":"/a","extra":true}), false, "/a").is_err());
        assert!(validate(&json!({"ok":true,"from":"/a","to":"/b"}), true, "/a").is_ok());
        assert!(validate(&json!({"ok":true,"from":"/a","to":"/a"}), true, "/a").is_err());
        assert!(
            !validate(
                &json!({"ok":false,"reason":"target_exists","path":"/b","detail":"token=secret"}),
                true,
                "/a"
            )
            .unwrap_err()
            .contains("secret")
        );
        assert!(validate(&json!({"ok":false,"reason":"target_exists"}), false, "/a").is_err());
        assert!(
            validate(
                &json!({"ok":false,"reason":"read_error","detail":null}),
                false,
                "/a"
            )
            .is_err()
        );
    }
}
