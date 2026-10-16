//! NDJSON protocol between zcode-gpui and the voice sidecar.
//!
//! Commands arrive one JSON object per stdin line. Events leave one JSON
//! object per stdout line. kokoro-tiny itself prints human-readable progress
//! to stdout, so readers must skip lines that don't parse as an [`Event`].

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::sync::Mutex;

pub const MAX_TEXT_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Command {
    Speak(SpeakRequest),
    Stop,
    Status,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakRequest {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default)]
    pub speed: Option<f32>,
    #[serde(default)]
    pub volume: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum Event {
    Loading,
    Downloading {
        file: String,
        percent: u32,
    },
    Ready {
        voices: Vec<String>,
    },
    Started {
        id: String,
    },
    Spoken {
        id: String,
    },
    Dropped {
        id: String,
    },
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        code: ErrorCode,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadCommand,
    EspeakMissing,
    ModelDownloadFailed,
    ModelLoadFailed,
    SynthFailed,
    AudioFailed,
}

pub fn parse_command(line: &str) -> Result<Command, String> {
    serde_json::from_str(line.trim()).map_err(|e| e.to_string())
}

/// Clamps speech text to [`MAX_TEXT_CHARS`] characters, collapsing whitespace.
pub fn clean_text(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(MAX_TEXT_CHARS).collect()
}

static STDOUT_LOCK: Mutex<()> = Mutex::new(());

pub fn emit(event: &Event) {
    let Ok(line) = serde_json::to_string(event) else {
        return;
    };
    let _guard = STDOUT_LOCK.lock();
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_speak_with_defaults() {
        let cmd = parse_command(r#"{"op":"speak","id":"a","text":"Done"}"#).unwrap();
        assert_eq!(
            cmd,
            Command::Speak(SpeakRequest {
                id: "a".into(),
                text: "Done".into(),
                voice: None,
                speed: None,
                volume: None,
            })
        );
    }

    #[test]
    fn parses_stop_and_status() {
        assert_eq!(parse_command(r#"{"op":"stop"}"#).unwrap(), Command::Stop);
        assert_eq!(
            parse_command(" {\"op\":\"status\"}\r\n").unwrap(),
            Command::Status
        );
    }

    #[test]
    fn rejects_unknown_ops() {
        assert!(parse_command(r#"{"op":"dance"}"#).is_err());
        assert!(parse_command("not json").is_err());
    }

    #[test]
    fn events_serialize_with_tag() {
        let value = serde_json::to_value(Event::Error {
            id: None,
            code: ErrorCode::EspeakMissing,
            message: "x".into(),
        })
        .unwrap();
        assert_eq!(
            value,
            json!({"event":"error","code":"espeak_missing","message":"x"})
        );
        let value = serde_json::to_value(Event::Downloading {
            file: "0.onnx".into(),
            percent: 40,
        })
        .unwrap();
        assert_eq!(
            value,
            json!({"event":"downloading","file":"0.onnx","percent":40})
        );
    }

    #[test]
    fn clean_text_collapses_and_clamps() {
        assert_eq!(clean_text("  Plan\n\n ready  "), "Plan ready");
        let long = "a".repeat(MAX_TEXT_CHARS + 50);
        assert_eq!(clean_text(&long).chars().count(), MAX_TEXT_CHARS);
    }
}
