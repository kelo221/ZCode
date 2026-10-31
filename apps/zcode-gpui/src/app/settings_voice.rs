//! Settings › Voice (VOICE.md): agents speak to the user on this PC. Desktop
//! only; the phone build hides the section.

use crate::app::root::RootView;
use crate::app::settings_rows::{ControlWidth, settings_card, settings_row};
use crate::shared::i18n::label;
use crate::shared::preferences::{PreferenceChange, Preferences};
use crate::shared::settings::AppSettings;
use crate::shared::theme::active_theme;
use crate::shared::voice_settings::{KNOWN_VOICES, VoiceSettings};
use crate::voice::VoiceStatus;
use ely_gpui_component::buttons::Button;
use ely_gpui_component::forms::{Choice, Select, Slider, Switch};
use gpui::{AnyElement, App, Context, IntoElement, SharedString, div, prelude::*};

const TEST_LINE: &str = "Hi, this is how your agents will sound when they need you.";

/// "af_sky" -> "Sky (American female)".
pub(crate) fn voice_label(id: &str) -> String {
    let mut parts = id.splitn(2, '_');
    let tag = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or(id);
    let accent = match tag.chars().next() {
        Some('a') => "American",
        Some('b') => "British",
        _ => return id.to_string(),
    };
    let gender = match tag.chars().nth(1) {
        Some('f') => " female",
        Some('m') => " male",
        _ => "",
    };
    let mut chars = name.chars();
    let title = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect::<String>())
        .unwrap_or_default();
    format!("{title} ({accent}{gender})")
}

pub(crate) fn status_text(status: &VoiceStatus) -> String {
    match status {
        VoiceStatus::NotStarted => label(
            "Not started (starts on first use)",
            "未启动（首次使用时启动）",
        )
        .into(),
        VoiceStatus::Missing => label(
            "Voice engine not found: build apps/zcode-voice or place zcode-voice next to GCode",
            "未找到语音引擎：请构建 apps/zcode-voice，或将 zcode-voice 放在 GCode 旁边",
        )
        .into(),
        VoiceStatus::Loading => label("Loading voice model…", "正在加载语音模型…").into(),
        VoiceStatus::Downloading { file, percent } => format!(
            "{} {file} {percent}%",
            label("Downloading voice model", "正在下载语音模型")
        ),
        VoiceStatus::Ready => label("Ready", "就绪").into(),
        VoiceStatus::Failed { code, message } if code == "espeak_missing" => format!(
            "{} ({message})",
            label(
                "eSpeak NG data not found. Install it with: winget install eSpeak-NG.eSpeak-NG",
                "未找到 eSpeak NG 数据。安装命令：winget install eSpeak-NG.eSpeak-NG",
            )
        ),
        VoiceStatus::Failed { message, .. } => format!("{} {message}", label("Error:", "错误：")),
    }
}

fn save(voice: VoiceSettings, cx: &mut App) {
    Preferences::enqueue(PreferenceChange::Voice(voice), cx);
}

impl RootView {
    pub(crate) fn voice_pane(
        &mut self,
        settings: &AppSettings,
        blocked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = active_theme();
        let voice = settings.gpui_voice.clone().unwrap_or_default();
        let state = self.state.clone();
        let service = state.read(cx).voice.as_ref();
        let status: SharedString = service
            .map(|s| status_text(&s.status))
            .unwrap_or_else(|| label("Unavailable on this device", "此设备不可用").into())
            .into();
        let reported: Vec<String> = service.map(|s| s.voices.clone()).unwrap_or_default();
        let ids: Vec<String> = if reported.is_empty() {
            KNOWN_VOICES.iter().map(|v| v.to_string()).collect()
        } else {
            reported
        };

        let enabled = {
            let (voice, state) = (voice.clone(), state.clone());
            Switch::new("settings-voice-enabled", voice.enabled)
                .disabled(blocked)
                .on_change(move |enabled, _, cx| {
                    if !enabled {
                        state.update(cx, |s, _| s.voice_stop());
                    }
                    save(
                        VoiceSettings {
                            enabled,
                            ..voice.clone()
                        },
                        cx,
                    );
                })
        };
        let picker = {
            let voice = voice.clone();
            Select::new(
                "settings-voice-name",
                ids.iter()
                    .map(|id| Choice::new(id.clone(), voice_label(id))),
            )
            .selected(voice.voice.clone())
            .disabled(blocked)
            .on_change(move |value, _, cx| {
                save(
                    VoiceSettings {
                        voice: value.to_string(),
                        ..voice.clone()
                    },
                    cx,
                )
            })
        };
        let speed = {
            let voice = voice.clone();
            Slider::new("settings-voice-speed", f64::from(voice.speed))
                .range(0.5, 2.0)
                .step(0.1)
                .disabled(blocked)
                .on_change(move |value, _, cx| {
                    let speed = ((value * 10.0).round() / 10.0) as f32;
                    save(
                        VoiceSettings {
                            speed,
                            ..voice.clone()
                        },
                        cx,
                    )
                })
        };
        let volume = {
            let voice = voice.clone();
            Slider::new("settings-voice-volume", f64::from(voice.volume))
                .range(0.0, 1.0)
                .step(0.05)
                .disabled(blocked)
                .on_change(move |value, _, cx| {
                    let volume = ((value * 20.0).round() / 20.0) as f32;
                    save(
                        VoiceSettings {
                            volume,
                            ..voice.clone()
                        },
                        cx,
                    )
                })
        };
        let unfocused = {
            let voice = voice.clone();
            Switch::new("settings-voice-unfocused", voice.only_when_unfocused)
                .disabled(blocked)
                .on_change(move |only_when_unfocused, _, cx| {
                    save(
                        VoiceSettings {
                            only_when_unfocused,
                            ..voice.clone()
                        },
                        cx,
                    )
                })
        };
        let test = {
            let (voice, state) = (voice.clone(), state.clone());
            Button::new("settings-voice-test", label("Test voice", "试听"))
                .disabled(service.is_none())
                .on_click(move |_, _, cx| {
                    state.update(cx, |s, cx| {
                        let _ = s.voice_say(TEST_LINE, &voice, cx);
                        cx.notify();
                    });
                })
        };
        let column = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(settings_card([
                settings_row(
                    label("Let agents speak", "允许智能体语音提醒"),
                    Some(label(
                        "The main agent gets a Speak tool for when it needs you, finishes a task or has a plan ready. Applies to sessions started or resumed after turning it on.",
                        "主智能体获得 Speak 工具，用于需要你处理、完成任务或计划就绪时。开启后对新建或恢复的会话生效。",
                    ).into()),
                    div().child(enabled),
                    ControlWidth::Default,
                ),
                settings_row(
                    label("Only when GCode isn't focused", "仅在 GCode 未聚焦时"),
                    Some(label(
                        "Stay silent while you are looking at the GCode window.",
                        "当你正在查看 GCode 窗口时保持安静。",
                    ).into()),
                    div().child(unfocused),
                    ControlWidth::Default,
                ),
            ]))
            .child(settings_card([
                settings_row(label("Voice", "声音"), None, div().w_full().child(picker), ControlWidth::Default),
                settings_row(
                    label("Speed", "语速"),
                    Some(format!("{:.1}×", voice.speed).into()),
                    div().w_full().child(speed),
                    ControlWidth::Wide,
                ),
                settings_row(
                    label("Volume", "音量"),
                    Some(format!("{:.0}%", voice.volume * 100.0).into()),
                    div().w_full().child(volume),
                    ControlWidth::Wide,
                ),
                settings_row(
                    label("Engine", "引擎"),
                    Some(status),
                    div().child(test),
                    ControlWidth::Wide,
                ),
            ]))
            .child(div().text_color(theme.subtle).child(label(
                "Speech is generated locally with Kokoro. The voice model (about 350 MB) downloads to ~/.zcode/v2/voice on first use. Voice plays only on this PC, never on a paired phone.",
                "语音由 Kokoro 在本地生成。语音模型（约 350 MB）会在首次使用时下载到 ~/.zcode/v2/voice。语音只在这台电脑上播放，不会在已配对的手机上播放。",
            )));
        #[cfg(test)]
        let column = crate::app::test_support::track_children(
            column,
            vec![
                "settings-voice-toggles".into(),
                "settings-voice-engine".into(),
                "settings-voice-note".into(),
            ],
        );
        column.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_labels_read_naturally() {
        assert_eq!(voice_label("af_sky"), "Sky (American female)");
        assert_eq!(voice_label("bm_george"), "George (British male)");
        assert_eq!(voice_label("weird"), "weird");
    }

    #[test]
    fn espeak_missing_points_at_the_install_command() {
        let text = status_text(&VoiceStatus::Failed {
            code: "espeak_missing".into(),
            message: "x".into(),
        });
        assert!(text.contains("winget install eSpeak-NG.eSpeak-NG"));
    }
}
