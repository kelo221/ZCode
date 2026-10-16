//! Speaking worker: loads Kokoro once, then synthesizes and plays queued jobs.

use crate::models::{self, MODEL, VOICES};
use crate::protocol::{ErrorCode, Event, SpeakRequest, emit};
use crate::queue::SpeechQueue;
use kokoro_tiny::TtsEngine;
use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, Sink};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SAMPLE_RATE: u32 = 24_000;
pub const DEFAULT_VOICE: &str = "af_sky";

/// State shared between the worker and the stdin reader.
#[derive(Default)]
pub struct Shared {
    pub queue: SpeechQueue,
    /// The sink currently playing, so `stop` can cut speech mid-sentence.
    pub current: Mutex<Option<Arc<Sink>>>,
    /// Last lifecycle event (loading/downloading/ready/error) for `status`.
    pub state: Mutex<Option<Event>>,
}

impl Shared {
    pub fn set_state(&self, event: Event) {
        emit(&event);
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = Some(event);
    }

    pub fn stop_current(&self) {
        if let Some(sink) = self
            .current
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            sink.stop();
        }
    }
}

struct Failure {
    code: ErrorCode,
    message: String,
}

pub fn run(shared: Arc<Shared>, model_dir: PathBuf, espeak: Result<PathBuf, String>) {
    let mut engine = match espeak {
        Err(message) => Err(Failure {
            code: ErrorCode::EspeakMissing,
            message,
        }),
        Ok(_) => load(&shared, &model_dir),
    };
    match &engine {
        Ok((_, voices)) => shared.set_state(Event::Ready {
            voices: voices.clone(),
        }),
        Err(f) => shared.set_state(Event::Error {
            id: None,
            code: f.code,
            message: f.message.clone(),
        }),
    }
    while let Some(job) = shared.queue.pop() {
        match &mut engine {
            Ok((tts, voices)) => speak(&shared, tts, voices, job),
            Err(f) => emit(&Event::Error {
                id: Some(job.id),
                code: f.code,
                message: f.message.clone(),
            }),
        }
    }
}

fn load(shared: &Shared, dir: &Path) -> Result<(TtsEngine, Vec<String>), Failure> {
    shared.set_state(Event::Loading);
    for file in [&VOICES, &MODEL] {
        let name = file.name.to_string();
        models::ensure(dir, file, &|percent| {
            shared.set_state(Event::Downloading {
                file: name.clone(),
                percent,
            })
        })
        .map_err(|message| Failure {
            code: ErrorCode::ModelDownloadFailed,
            message,
        })?;
    }
    let model = models::path_in(dir, &MODEL);
    let voices = models::path_in(dir, &VOICES);
    let load_failed = |message: String| Failure {
        code: ErrorCode::ModelLoadFailed,
        message,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| load_failed(e.to_string()))?;
    let tts = runtime
        .block_on(TtsEngine::with_paths(
            &model.to_string_lossy(),
            &voices.to_string_lossy(),
        ))
        .map_err(load_failed)?;
    let all = tts.voices();
    // kokoro-tiny silently falls back to a canned "I lost my voice" clip when
    // it can't load the model; treat that as a load failure.
    if all.is_empty() || all == ["fallback"] {
        return Err(load_failed("Kokoro model files could not be loaded".into()));
    }
    let names = english_voices(all);
    if names.is_empty() {
        return Err(load_failed(
            "Kokoro voices file has no English voices".into(),
        ));
    }
    Ok((tts, names))
}

/// Only American (`a*`) and British (`b*`) voices: phonemes come from espeak's
/// English rules, so other languages' voices would mispronounce everything.
pub fn english_voices(mut all: Vec<String>) -> Vec<String> {
    all.retain(|v| v.starts_with("a") || v.starts_with("b"));
    all.sort();
    all
}

/// espeak language for a Kokoro voice: `b*` voices are British English.
pub fn lang_for_voice(voice: &str) -> &'static str {
    if voice.starts_with('b') {
        "en-gb"
    } else {
        "en-us"
    }
}

pub fn pick_voice<'a>(requested: Option<&'a str>, available: &[String]) -> &'a str {
    match requested {
        Some(v) if available.iter().any(|a| a == v) => v,
        _ => DEFAULT_VOICE,
    }
}

fn speak(shared: &Shared, tts: &mut TtsEngine, voices: &[String], job: SpeakRequest) {
    let voice = pick_voice(job.voice.as_deref(), voices);
    let speed = job.speed.unwrap_or(1.0).clamp(0.5, 2.0);
    let volume = job.volume.unwrap_or(0.8).clamp(0.0, 1.0);
    emit(&Event::Started { id: job.id.clone() });
    let samples = match tts.synthesize(
        &job.text,
        Some(voice),
        Some(speed),
        Some(lang_for_voice(voice)),
    ) {
        Ok(samples) => samples,
        Err(message) => {
            return emit(&Event::Error {
                id: Some(job.id),
                code: ErrorCode::SynthFailed,
                message,
            });
        }
    };
    match play(shared, samples, volume) {
        Ok(()) => emit(&Event::Spoken { id: job.id }),
        Err(message) => emit(&Event::Error {
            id: Some(job.id),
            code: ErrorCode::AudioFailed,
            message,
        }),
    }
}

/// Opens the default output per utterance so device changes (headphones
/// plugged in) are picked up, and blocks until playback ends or is stopped.
fn play(shared: &Shared, samples: Vec<f32>, volume: f32) -> Result<(), String> {
    let (_stream, handle) = OutputStream::try_default().map_err(|e| e.to_string())?;
    let sink = Arc::new(Sink::try_new(&handle).map_err(|e| e.to_string())?);
    sink.set_volume(volume);
    sink.append(SamplesBuffer::new(1, SAMPLE_RATE, samples));
    *shared.current.lock().unwrap_or_else(|e| e.into_inner()) = Some(sink.clone());
    sink.sleep_until_end();
    shared
        .current
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn british_voices_use_en_gb() {
        assert_eq!(lang_for_voice("bf_emma"), "en-gb");
        assert_eq!(lang_for_voice("af_sky"), "en-us");
        assert_eq!(lang_for_voice("am_adam"), "en-us");
    }

    #[test]
    fn keeps_only_english_voices_sorted() {
        let all = ["jf_alpha", "bm_george", "af_sky", "zf_xiaoyi", "am_adam"]
            .map(String::from)
            .to_vec();
        assert_eq!(english_voices(all), ["af_sky", "am_adam", "bm_george"]);
    }

    #[test]
    fn unknown_voices_fall_back_to_default() {
        let available = vec!["af_bella".to_string(), "af_sky".to_string()];
        assert_eq!(pick_voice(Some("af_bella"), &available), "af_bella");
        assert_eq!(pick_voice(Some("zz_nobody"), &available), DEFAULT_VOICE);
        assert_eq!(pick_voice(None, &available), DEFAULT_VOICE);
    }
}
