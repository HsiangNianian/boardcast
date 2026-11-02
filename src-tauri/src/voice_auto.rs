//! Identify the opening speech only; replay the retained PCM to Apple, then release this model.
use super::{VadLevel, SAMPLE_RATE, WINDOW};
use crate::models::voice_files;
use serde::Deserialize;
use sherpa_onnx::{SileroVadModelConfig, VadModelConfig, VoiceActivityDetector};
use sherpa_onnx_sys as sys;
use std::ffi::{CStr, CString};
use std::path::Path;

pub const UNCERTAIN: &str = "APPLE_AUTO_LANGUAGE_UNCERTAIN";
const RATE: usize = SAMPLE_RATE as usize;

struct Probe(*const sys::OfflineRecognizer);
// SAFETY: Like sherpa-onnx's safe wrapper, this recognizer has one owner and is used serially.
unsafe impl Send for Probe {}

#[derive(Deserialize)]
struct ResultText {
    text: String,
    #[serde(default, alias = "language")]
    lang: String,
    #[serde(default)]
    event: String,
}

fn language(result: &ResultText) -> Option<&str> {
    let lang = result.lang.trim_start_matches("<|").trim_end_matches("|>");
    let event = result.event.trim_start_matches("<|").trim_end_matches("|>");
    (!result.text.trim().is_empty()
        && event.eq_ignore_ascii_case("speech")
        && ["zh", "ja", "en"].contains(&lang))
    .then_some(lang)
}

impl Probe {
    fn load(models: &Path) -> Result<Self, String> {
        let path = |name| {
            CString::new(models.join(name).to_string_lossy().as_bytes()).map_err(|e| e.to_string())
        };
        let model = path(voice_files::SENSE_VOICE)?;
        let tokens = path(voice_files::TOKENS)?;
        // SAFETY: This C config contains only numbers and nullable pointers, as in the upstream C example.
        let mut config: sys::OfflineRecognizerConfig = unsafe { std::mem::zeroed() };
        config.feat_config.sample_rate = SAMPLE_RATE;
        config.feat_config.feature_dim = 80;
        config.model_config.sense_voice.model = model.as_ptr();
        config.model_config.sense_voice.language = c"auto".as_ptr();
        config.model_config.tokens = tokens.as_ptr();
        config.model_config.num_threads = 2;
        config.model_config.provider = c"cpu".as_ptr();
        config.decoding_method = c"greedy_search".as_ptr();
        config.max_active_paths = 4;
        // SAFETY: The factory copies the config strings before returning.
        let ptr = unsafe { sys::SherpaOnnxCreateOfflineRecognizer(&config) };
        if ptr.is_null() {
            Err("Couldn't load the Auto language detector".into())
        } else {
            Ok(Self(ptr))
        }
    }

    fn detect(&self, samples: &[f32]) -> Result<Option<String>, String> {
        if samples.is_empty() || samples.len() > RATE * 5 || samples.iter().any(|s| !s.is_finite())
        {
            return Err("Invalid audio for language detection".into());
        }
        // SAFETY: All handles belong to this call; samples remain valid throughout synchronous decoding.
        let json = unsafe {
            let stream = sys::SherpaOnnxCreateOfflineStream(self.0);
            if stream.is_null() {
                return Err("Couldn't create the language detection stream".into());
            }
            sys::SherpaOnnxAcceptWaveformOffline(
                stream,
                SAMPLE_RATE,
                samples.as_ptr(),
                samples.len() as i32,
            );
            sys::SherpaOnnxDecodeOfflineStream(self.0, stream);
            let result = sys::SherpaOnnxGetOfflineStreamResultAsJson(stream);
            let json =
                (!result.is_null()).then(|| CStr::from_ptr(result).to_string_lossy().into_owned());
            if !result.is_null() {
                sys::SherpaOnnxDestroyOfflineStreamResultJson(result);
            }
            sys::SherpaOnnxDestroyOfflineStream(stream);
            json.ok_or("Language detection returned no result")?
        };
        let result: ResultText = serde_json::from_str(&json)
            .map_err(|e| format!("Invalid language detection result: {e}"))?;
        Ok(language(&result).map(str::to_owned))
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        // SAFETY: This is the unique owning handle created above.
        unsafe { sys::SherpaOnnxDestroyOfflineRecognizer(self.0) }
    }
}

pub struct Route {
    pub language: String,
    pub samples: Vec<f32>,
    pub offset: f64,
}

pub struct Detector {
    probe: Probe,
    vad: VoiceActivityDetector,
    level: VadLevel,
    pending: Vec<f32>,
    audio: Vec<f32>,
    seen: usize,
    offset: usize,
    speech: Option<usize>,
    attempted: usize,
}

impl Detector {
    pub fn load(models: &Path) -> Result<Self, String> {
        let vad = VoiceActivityDetector::create(
            &VadModelConfig {
                silero_vad: SileroVadModelConfig {
                    model: Some(models.join(voice_files::VAD).to_string_lossy().into_owned()),
                    threshold: 0.5,
                    min_silence_duration: 0.45,
                    min_speech_duration: 0.25,
                    window_size: WINDOW as i32,
                    max_speech_duration: 3.0,
                },
                sample_rate: SAMPLE_RATE,
                num_threads: 1,
                ..Default::default()
            },
            6.0,
        )
        .ok_or("Couldn't load voice detection for Auto")?;
        Ok(Self {
            probe: Probe::load(models)?,
            vad,
            level: VadLevel::default(),
            pending: vec![],
            audio: vec![],
            seen: 0,
            offset: 0,
            speech: None,
            attempted: 0,
        })
    }

    pub fn feed(&mut self, samples: &[f32], finish: bool) -> Result<Option<Route>, String> {
        self.audio.extend_from_slice(samples);
        self.pending.extend_from_slice(samples);
        let mut ended = false;
        while self.pending.len() >= WINDOW {
            let mut window: Vec<_> = self.pending.drain(..WINDOW).collect();
            self.seen += WINDOW;
            self.level.apply(&mut window);
            self.vad.accept_waveform(&window);
            if self.vad.detected() {
                self.speech.get_or_insert(self.seen);
            }
            while !self.vad.is_empty() {
                self.speech.get_or_insert(self.seen);
                ended = true;
                self.vad.pop();
            }
        }
        if self.speech.is_none() {
            // Retain a second of pre-roll, but never grow memory while waiting in a quiet room.
            let discard = self.audio.len().saturating_sub(RATE);
            self.audio.drain(..discard);
            self.offset += discard;
            return Ok(None);
        }
        let elapsed = self.seen.saturating_sub(self.speech.unwrap());
        if self.audio.len() > RATE * 5 {
            return Err(UNCERTAIN.into());
        }
        if finish
            || (self.seen.saturating_sub(self.attempted) >= RATE && (ended || elapsed >= RATE * 2))
        {
            self.attempted = self.seen;
            if let Some(language) = self.probe.detect(&self.audio)? {
                return Ok(Some(Route {
                    language,
                    samples: std::mem::take(&mut self.audio),
                    offset: self.offset as f64 / RATE as f64,
                }));
            }
            if finish || elapsed >= RATE * 3 {
                return Err(UNCERTAIN.into());
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_requires_spoken_supported_language_metadata() {
        for lang in ["zh", "ja", "en", "<|ja|>"] {
            assert!(language(&ResultText {
                lang: lang.into(),
                event: "<|Speech|>".into(),
                text: "word".into()
            })
            .is_some());
        }
        for (lang, event, text) in [
            ("", "Speech", "hello"),
            ("ko", "Speech", "hello"),
            ("en", "Music", "hello"),
            ("en", "Speech", ""),
        ] {
            assert!(language(&ResultText {
                lang: lang.into(),
                event: event.into(),
                text: text.into()
            })
            .is_none());
        }
    }

    #[test]
    #[ignore = "needs the verified ASR corpus and bundled models; run by Eval"]
    fn auto_language_real_audio() {
        let models = std::env::var("BOARDCAST_AUTO_MODELS").expect("model directory");
        let audio = std::env::var("BOARDCAST_AUTO_AUDIO").expect("audio directory");
        for (file, expected) in [
            ("zh-alimeeting-00.wav", "zh"),
            ("ja-fleurs-00.wav", "ja"),
            ("en-ami-00.wav", "en"),
        ] {
            let mut detector = Detector::load(Path::new(&models)).unwrap();
            for _ in 0..200 {
                assert!(detector.feed(&vec![0.0; 1600], false).unwrap().is_none());
            }
            assert!(detector.audio.len() <= RATE);
            assert!(detector.feed(&[], true).unwrap().is_none());
            let wave =
                sherpa_onnx::Wave::read(&Path::new(&audio).join(file).to_string_lossy()).unwrap();
            let mut fed = vec![0.0; RATE * 20];
            let started = std::time::Instant::now();
            let mut routed = None;
            for chunk in wave.samples().chunks(1600) {
                fed.extend_from_slice(chunk);
                if let Some(route) = detector.feed(chunk, false).unwrap() {
                    routed = Some(route);
                    break;
                }
            }
            let route = routed
                .or_else(|| detector.feed(&[], true).unwrap())
                .expect("language detected");
            assert_eq!(route.language, expected, "{file}");
            let offset = (route.offset * RATE as f64).round() as usize;
            assert_eq!(
                route.samples,
                fed[offset..],
                "opening PCM must be replayed exactly once"
            );
            assert!(route.samples.len() <= RATE * 5);
            eprintln!(
                "Auto {file}: {} at {:.2}s, probe wall {:?}, retained {:.2}s",
                route.language,
                fed.len() as f64 / RATE as f64 - 20.0,
                started.elapsed(),
                route.samples.len() as f64 / RATE as f64
            );
        }
    }
}
