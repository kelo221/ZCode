//! `setting.json` `gpuiVoice`: the GPUI-only voice preferences (VOICE.md).

use serde::{Deserialize, Serialize};

pub const DEFAULT_VOICE: &str = "af_sky";
pub const SPEED_RANGE: std::ops::RangeInclusive<f32> = 0.5..=2.0;
pub const VOLUME_RANGE: std::ops::RangeInclusive<f32> = 0.0..=1.0;

/// English Kokoro voices (`a*` American, `b*` British), offered before the
/// sidecar has reported its own list.
pub const KNOWN_VOICES: &[&str] = &[
    "af_alloy",
    "af_aoede",
    "af_bella",
    "af_heart",
    "af_jessica",
    "af_kore",
    "af_nicole",
    "af_nova",
    "af_river",
    "af_sarah",
    "af_sky",
    "am_adam",
    "am_echo",
    "am_eric",
    "am_fenrir",
    "am_liam",
    "am_michael",
    "am_onyx",
    "am_puck",
    "am_santa",
    "bf_alice",
    "bf_emma",
    "bf_isabella",
    "bf_lily",
    "bm_daniel",
    "bm_fable",
    "bm_george",
    "bm_lewis",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct VoiceSettings {
    pub enabled: bool,
    pub voice: String,
    pub speed: f32,
    pub volume: f32,
    pub only_when_unfocused: bool,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            voice: DEFAULT_VOICE.into(),
            speed: 1.0,
            volume: 0.8,
            only_when_unfocused: false,
        }
    }
}

impl VoiceSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !self.speed.is_finite() || !SPEED_RANGE.contains(&self.speed) {
            return Err("Voice speed must be between 0.5 and 2".into());
        }
        if !self.volume.is_finite() || !VOLUME_RANGE.contains(&self.volume) {
            return Err("Voice volume must be between 0 and 1".into());
        }
        let name_ok = (self.voice.starts_with('a') || self.voice.starts_with('b'))
            && self.voice.len() <= 32
            && self
                .voice
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !name_ok {
            return Err("Unknown voice".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_off_and_valid() {
        let settings = VoiceSettings::default();
        assert!(!settings.enabled);
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn partial_json_fills_defaults() {
        let settings: VoiceSettings = serde_json::from_str(r#"{"enabled":true}"#).unwrap();
        assert!(settings.enabled);
        assert_eq!(settings.voice, DEFAULT_VOICE);
        assert_eq!(
            serde_json::to_value(&settings).unwrap()["onlyWhenUnfocused"],
            serde_json::json!(false)
        );
    }

    #[test]
    fn rejects_out_of_range_and_foreign_voices() {
        let base = VoiceSettings::default();
        assert!(
            VoiceSettings {
                speed: 3.0,
                ..base.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            VoiceSettings {
                volume: -0.1,
                ..base.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            VoiceSettings {
                voice: "jf_alpha".into(),
                ..base.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            VoiceSettings {
                voice: "a/../x".into(),
                ..base
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn known_voices_are_valid() {
        for voice in KNOWN_VOICES {
            let settings = VoiceSettings {
                voice: (*voice).into(),
                ..VoiceSettings::default()
            };
            assert!(settings.validate().is_ok(), "{voice}");
        }
    }
}
