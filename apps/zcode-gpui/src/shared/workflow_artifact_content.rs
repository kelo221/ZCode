use base64::{Engine, prelude::BASE64_STANDARD};
use serde::Deserialize;
use serde_json::Value;

pub(crate) const MARKDOWN_MAX_BYTES: usize = 256 * 1024;
pub(crate) const CONTENT_ERROR: &str = "Markdown artifact could not be read";

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ContentResult {
    data_base64: String,
    media_type: String,
    total_bytes: u64,
    next_offset: Option<u64>,
}

pub(crate) fn decode_markdown(value: &Value) -> Result<String, &'static str> {
    if value.get("nextOffset") != Some(&Value::Null) {
        return Err(CONTENT_ERROR);
    }
    let result: ContentResult = serde_json::from_value(value.clone()).map_err(|_| CONTENT_ERROR)?;
    if result.media_type != "text/markdown"
        || result.total_bytes > MARKDOWN_MAX_BYTES as u64
        || result.next_offset.is_some()
        || result.data_base64.len() > MARKDOWN_MAX_BYTES.div_ceil(3) * 4
    {
        return Err(CONTENT_ERROR);
    }
    let bytes = BASE64_STANDARD
        .decode(&result.data_base64)
        .map_err(|_| CONTENT_ERROR)?;
    if bytes.len() > MARKDOWN_MAX_BYTES
        || bytes.len() as u64 != result.total_bytes
        || BASE64_STANDARD.encode(&bytes) != result.data_base64
    {
        return Err(CONTENT_ERROR);
    }
    String::from_utf8(bytes).map_err(|_| CONTENT_ERROR)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn markdown_read_requires_complete_exact_mime_canonical_utf8_bytes() {
        let good = json!({"dataBase64":BASE64_STANDARD.encode("# Report"),"mediaType":"text/markdown","totalBytes":8,"nextOffset":null});
        assert_eq!(decode_markdown(&good).unwrap(), "# Report");
        assert_eq!(decode_markdown(&json!({"dataBase64":"","mediaType":"text/markdown","totalBytes":0,"nextOffset":null})).unwrap(), "");
        for (key, value) in [
            ("mediaType", json!("text/html")),
            ("totalBytes", json!(7)),
            ("nextOffset", json!(8)),
            ("unknown", json!(true)),
            ("dataBase64", json!("@@@=")),
            ("dataBase64", json!("/w==")),
            ("totalBytes", json!(MARKDOWN_MAX_BYTES + 1)),
        ] {
            let mut bad = good.clone();
            bad[key] = value;
            assert_eq!(decode_markdown(&bad).unwrap_err(), CONTENT_ERROR);
        }
        let mut bad = good.clone();
        bad.as_object_mut().unwrap().remove("nextOffset");
        assert!(decode_markdown(&bad).is_err());
        bad = json!({"dataBase64":BASE64_STANDARD.encode(vec![b'a';MARKDOWN_MAX_BYTES+1]),"mediaType":"text/markdown","totalBytes":MARKDOWN_MAX_BYTES+1,"nextOffset":null});
        assert!(decode_markdown(&bad).is_err());
    }
}
