//! The audio chain: microphone or file -> 16 kHz mono -> Silero VAD -> a recognizer and CAM++ -> a finished sentence.
//! Any of the recognizers may be missing; SenseVoice, when there, decides which language a sentence is in.
//! It knows nothing about boards; callers receive each sentence through a callback.

use anyhow::{anyhow, Context, Result};
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig, OfflineTransducerModelConfig, SileroVadModelConfig, SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig, VadModelConfig, VoiceActivityDetector, Wave};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

mod helper;
mod language;
pub mod system;

pub const SAMPLE_RATE: i32 = 16_000;
const VAD_WINDOW: usize = 512;
/// Same pause as the desktop app: this long a silence ends a sentence.
const MIN_SILENCE: f32 = 0.45;
const MAX_SPEECH: f32 = 10.0;
/// Once a sentence is this long, in seconds, a short dip in loudness is enough to end it. People who
/// talk continuously rarely pause for `MIN_SILENCE`, and waiting for `MAX_SPEECH` leaves the board behind.
const SOFT_MAX: f32 = 4.0;
/// How many quiet windows of 32 ms make a dip. Right after `SOFT_MAX` it takes a breath, so the gaps
/// between words don't cut a clause in half; the longer the sentence runs, the shorter the dip needed.
const SOFT_PAUSE: usize = 10;
const SOFT_PAUSE_MIN: usize = 4;
/// A window is quiet below this share of the sentence's median loudness.
const QUIET_RATIO: f32 = 0.3;
/// Taken from before speech was detected, or the first word is lost: the detector only reports speech
/// after `min_speech_duration` and its own delay. SenseVoice also reads a first word better after some silence.
const PREROLL: usize = SAMPLE_RATE as usize;
/// The most audio kept for one unbroken stretch of speech. Soft cuts do not empty the detector's own
/// buffer, so past this it is flushed without resetting its clock.
const RETAIN: usize = SAMPLE_RATE as usize * 30;

/// Levels only the detector's copy: quiet speech can be recognizable yet below Silero's input range.
struct VadLevel {
    peak: f32,
    gain: f32,
}

impl Default for VadLevel {
    fn default() -> Self {
        Self { peak: 0.0, gain: 1.0 }
    }
}

impl VadLevel {
    fn apply(&mut self, samples: &mut [f32]) {
        self.peak = samples.iter().fold(self.peak * 0.95, |peak, x| peak.max(x.abs()));
        let target = if self.peak >= 0.08 { 1.0 } else { (0.2 / self.peak.max(0.025)).clamp(1.0, 8.0) };
        // Fast reduction, gradual increase: the quiet onset of normal speech must not be boosted 8x.
        let rise = if rms(samples) >= 0.0005 { 0.125 } else { 0.0 };
        self.gain = target.min(self.gain + rise);
        for sample in samples {
            *sample *= self.gain;
        }
    }
}

fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

/// Given each window's loudness, whether the sentence can end at the dip it finishes on, and how many
/// windows at the end belong to the next one.
fn soft_break(loudness: &[f32]) -> Option<usize> {
    let n = loudness.len();
    if n * VAD_WINDOW < (SOFT_MAX * SAMPLE_RATE as f32) as usize {
        return None;
    }
    let mut sorted = loudness.to_vec();
    sorted.sort_by(f32::total_cmp);
    // The quiet-room floor stays below speech itself; an absolute floor cuts quiet words in half.
    let median = sorted[n / 2];
    let quiet = (median * QUIET_RATIO).max(0.003_f32.min(median * 0.5));
    let run = loudness.iter().rev().take_while(|&&r| r < quiet).count();
    // One window less per second past SOFT_MAX.
    let over = (n * VAD_WINDOW) as f32 / SAMPLE_RATE as f32 - SOFT_MAX;
    let need = (SOFT_PAUSE as f32 - over).round().clamp(SOFT_PAUSE_MIN as f32, SOFT_PAUSE as f32) as usize;
    // After a cut only the tail is kept, so this does not fire again on the same dip.
    (run >= need).then_some(run / 2)
}

/// Shorter than this says too little about a voice, so "yeah" and "right" carry no voiceprint.
const METER_RANGE_DB: f32 = 60.0;
const MIN_VOICEPRINT: usize = SAMPLE_RATE as usize * 4 / 5;

/// How full a level meter should be for a peak sample, from 0 to 1. Hearing is logarithmic: on a linear
/// scale ordinary speech, whose peaks are often below a tenth, barely moves the bar.
pub fn loudness(peak: f32) -> f32 {
    (1.0 + 20.0 * peak.max(1e-5).log10() / METER_RANGE_DB).clamp(0.0, 1.0)
}

pub struct Sentence {
    pub text: String,
    /// At unit length, so a dot product is the cosine similarity. Missing if it was too short to tell.
    pub voiceprint: Option<Vec<f32>>,
    pub asr_ms: u128,
    /// When the sentence was detected as over.
    pub ended: Instant,
    /// Where in the input it began, in seconds.
    pub start: f64,
    pub seconds: f64,
    /// Which recognizer wrote the text.
    pub engine: Engine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    SenseVoice,
    Parakeet,
    /// ReazonSpeech, for Japanese.
    Reazon,
    /// The recognizer built into the system, such as Apple's.
    System,
}

/// Shorter than this, ReazonSpeech makes up words, so SenseVoice's reading stands.
const MIN_JAPANESE: usize = SAMPLE_RATE as usize * 3 / 10;

/// Whether recognized text is Japanese: it has kana. Japanese written only in kanji is too rare to matter,
/// and telling it from Chinese would take more than the text.
pub fn is_japanese(text: &str) -> bool {
    text.chars().any(|c| matches!(c as u32, 0x3040..=0x309F | 0x30A0..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9D))
}

/// A recognizer outside this crate, such as the system's own, that takes a sentence and gives its text.
pub trait Recognizer: Send {
    /// Observe VAD-qualified chunks; an empty slice queries readiness or finishes a short capture.
    fn prepare(&mut self, _samples: &[f32], _finished: bool) -> bool { true }
    fn recognize(&mut self, samples: &[f32]) -> Result<String>;
    fn error(&self) -> Option<&str> { None }
    fn identity(&self) -> Option<(&str, &str)> { None }
}

/// Text this short might have been made up from noise, so its sound is checked for a voice.
const NOISE_WORDS: usize = 12;
/// Real sentences measured 0.2 or more voiced on the ASR corpus; keyboard clicks, coughs and hiss 0.
const MIN_VOICED: f32 = 0.05;

/// The share of 30 ms frames that are voiced: loud enough, and periodic at a pitch a voice can have.
/// Speech is full of them; clicks, breath and hum are not.
pub fn voiced_share(samples: &[f32]) -> f32 {
    const FRAME: usize = 480;
    const LAGS: std::ops::RangeInclusive<usize> = 40..=200; // 400 Hz down to 80 Hz
    let frames: Vec<&[f32]> = samples.chunks_exact(FRAME).collect();
    if frames.is_empty() {
        return 0.0;
    }
    let loudest = frames.iter().map(|f| rms(f)).fold(0.0_f32, f32::max);
    let voiced = frames
        .iter()
        .filter(|f| {
            let energy: f32 = f.iter().map(|x| x * x).sum();
            if energy <= 0.0 || rms(f) < loudest * 0.1 {
                return false;
            }
            LAGS.clone().any(|lag| {
                let (a, b) = (&f[..FRAME - lag], &f[lag..]);
                let num: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
                let den = (a.iter().map(|x| x * x).sum::<f32>() * b.iter().map(|x| x * x).sum::<f32>()).sqrt();
                den > 0.0 && num / den > 0.6
            })
        })
        .count();
    voiced as f32 / frames.len() as f32
}

/// Whether recognized text is Korean: it has Hangul. Nobody here speaks Korean, so SenseVoice heard
/// a short Chinese word that way, as with 哦 written 어.
pub fn is_korean(text: &str) -> bool {
    text.chars().any(|c| matches!(c as u32, 0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF))
}

/// Whether recognized text is English rather than another language: letters, and no Chinese, Japanese
/// or Korean. Chinese with English terms stays with SenseVoice, which handles that mix.
pub fn is_english(text: &str) -> bool {
    let cjk = text.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF));
    let letters = text.chars().filter(char::is_ascii_alphabetic).count();
    !cjk && letters >= 2
}

pub trait Heard: FnMut(Sentence) {}
impl<F: FnMut(Sentence)> Heard for F {}

fn recognize(asr: &OfflineRecognizer, samples: &[f32]) -> String {
    let stream = asr.create_stream();
    stream.accept_waveform(SAMPLE_RATE, samples);
    asr.decode(&stream);
    stream.get_result().map(|r| r.text.trim().to_string()).unwrap_or_default()
}

/// Parakeet Unified EN, from `dir`, if its files are there.
pub fn load_parakeet(dir: &Path) -> Option<OfflineRecognizer> {
    let file = |name: &str| dir.join(name).is_file().then(|| dir.join(name).to_string_lossy().to_string());
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.transducer = OfflineTransducerModelConfig { encoder: file("encoder.int8.onnx"), decoder: file("decoder.int8.onnx"), joiner: file("joiner.int8.onnx") };
    config.model_config.tokens = file("tokens.txt");
    config.model_config.model_type = Some("nemo_transducer".into());
    config.model_config.num_threads = 2;
    if config.model_config.transducer.encoder.is_none() || config.model_config.tokens.is_none() {
        return None;
    }
    let recognizer = OfflineRecognizer::create(&config);
    if recognizer.is_none() {
        eprintln!("Parakeet did not load, so SenseVoice recognizes English too");
    }
    recognizer
}

/// ReazonSpeech k2 v2, for Japanese, from `dir`, if its files are there.
pub fn load_reazon(dir: &Path) -> Option<OfflineRecognizer> {
    let file = |name: &str| dir.join(name).is_file().then(|| dir.join(name).to_string_lossy().to_string());
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.transducer = OfflineTransducerModelConfig { encoder: file("encoder-epoch-99-avg-1.int8.onnx"), decoder: file("decoder-epoch-99-avg-1.int8.onnx"), joiner: file("joiner-epoch-99-avg-1.int8.onnx") };
    config.model_config.tokens = file("tokens.txt");
    config.model_config.num_threads = 2;
    if config.model_config.transducer.encoder.is_none() || config.model_config.tokens.is_none() {
        return None;
    }
    let recognizer = OfflineRecognizer::create(&config);
    if recognizer.is_none() {
        eprintln!("ReazonSpeech did not load, so Japanese is left to the other recognizers");
    }
    recognizer
}

/// SenseVoice, from `dir`, if its files are there.
fn load_sense_voice_if_there(dir: &Path) -> Option<OfflineRecognizer> {
    if !dir.join("model.int8.onnx").is_file() {
        return None;
    }
    let recognizer = load_sense_voice(dir);
    if let Err(e) = &recognizer {
        eprintln!("{e:#}");
    }
    recognizer.ok()
}

/// SenseVoice, from `dir`, telling the language by itself.
pub fn load_sense_voice(dir: &Path) -> Result<OfflineRecognizer> {
    load_sense_voice_for(dir, "auto")
}

/// SenseVoice, from `dir`, hearing one language: "zh", "en", "ja", or "auto" to tell.
pub fn load_sense_voice_for(dir: &Path, language: &str) -> Result<OfflineRecognizer> {
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.sense_voice = OfflineSenseVoiceModelConfig { model: Some(dir.join("model.int8.onnx").to_string_lossy().to_string()), language: Some(language.into()), use_itn: true };
    config.model_config.tokens = Some(dir.join("tokens.txt").to_string_lossy().to_string());
    config.model_config.num_threads = 2;
    OfflineRecognizer::create(&config).context("loading SenseVoice")
}

/// Recognize a whole recording with one engine, for comparing them on the same audio.
pub fn transcribe(asr: &OfflineRecognizer, samples: &[f32]) -> String {
    recognize(asr, samples)
}

/// The recognizers an ear has. Each may be missing, but not all of them.
pub struct Recognizers {
    /// Takes every sentence when given, as the person chose it over the models.
    pub system: Option<Box<dyn Recognizer>>,
    /// Hears every language and says which one it was, so it routes the others.
    pub sense_voice: Option<OfflineRecognizer>,
    /// Recognizes English again, as it does English better.
    pub english: Option<OfflineRecognizer>,
    /// Recognizes Japanese again, as it does Japanese better.
    pub japanese: Option<OfflineRecognizer>,
    /// Where SenseVoice came from, so it can be loaded again told to hear Chinese.
    pub sense_voice_dir: Option<PathBuf>,
    /// SenseVoice for Chinese only, loaded the first time a sentence comes out as Korean.
    pub chinese: Option<OfflineRecognizer>,
}

impl Recognizers {
    /// Whatever models are in `models`: `sense-voice/`, `parakeet-en/` and `reazon-ja/`.
    pub fn load(models: &Path) -> Self {
        let dir = models.join("sense-voice");
        let sense_voice = load_sense_voice_if_there(&dir);
        let sense_voice_dir = sense_voice.is_some().then_some(dir);
        Self { system: None, sense_voice, english: load_parakeet(&models.join("parakeet-en")), japanese: load_reazon(&models.join("reazon-ja")), sense_voice_dir, chinese: None }
    }

    /// Only the system's recognizer, for when it was chosen and nothing else needs loading.
    pub fn system(recognizer: Box<dyn Recognizer>) -> Self {
        Self { system: Some(recognizer), sense_voice: None, english: None, japanese: None, sense_voice_dir: None, chinese: None }
    }

    /// The models there, by name, for saying what hears.
    pub fn names(&self) -> Vec<&'static str> {
        [(self.sense_voice.is_some(), "SenseVoice"), (self.english.is_some(), "Parakeet"), (self.japanese.is_some(), "ReazonSpeech")].into_iter().filter_map(|(there, name)| there.then_some(name)).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.system.is_none() && self.sense_voice.is_none() && self.english.is_none() && self.japanese.is_none()
    }

    /// The text of a sentence and what wrote it, empty if nothing was understood.
    fn hear(&mut self, samples: &[f32]) -> (String, Engine) {
        if let Some(system) = self.system.as_mut() {
            return match system.recognize(samples) {
                Ok(text) => (text, Engine::System),
                Err(e) => {
                    eprintln!("the system recognizer failed: {e:#}");
                    (String::new(), Engine::System)
                }
            };
        }
        let (text, engine) = match (&self.sense_voice, &self.japanese, &self.english) {
            (Some(asr), _, _) => (self.not_korean(recognize(asr, samples), samples), Engine::SenseVoice),
            (None, Some(japanese), _) if samples.len() >= MIN_JAPANESE => (recognize(japanese, samples), Engine::Reazon),
            (None, Some(_), None) => return (String::new(), Engine::Reazon),
            (None, _, Some(english)) => (recognize(english, samples), Engine::Parakeet),
            (None, _, None) => return (String::new(), Engine::SenseVoice),
        };
        // What the first reading says about the language decides who reads it again.
        let again = match engine {
            Engine::SenseVoice if is_japanese(&text) && samples.len() >= MIN_JAPANESE => self.japanese.as_ref().map(|j| (j, Engine::Reazon)),
            Engine::SenseVoice | Engine::Reazon if is_english(&text) => self.english.as_ref().map(|e| (e, Engine::Parakeet)),
            _ => None,
        };
        if let Some((asr, better)) = again {
            let text = recognize(asr, samples);
            if !text.is_empty() {
                return (text, better);
            }
        }
        (text, engine)
    }
}

impl Recognizers {
    /// Korean from SenseVoice is Chinese it misheard: read the sentence again as Chinese. Should that
    /// fail as well, nothing is written rather than Korean.
    fn not_korean(&mut self, text: String, samples: &[f32]) -> String {
        if !is_korean(&text) {
            return text;
        }
        if self.chinese.is_none() {
            self.chinese = self.sense_voice_dir.as_ref().and_then(|dir| load_sense_voice_for(dir, "zh").map_err(|e| eprintln!("{e:#}")).ok());
        }
        let again = self.chinese.as_ref().map(|zh| recognize(zh, samples)).unwrap_or_default();
        if is_korean(&again) { String::new() } else { again }
    }
}

pub struct Ear {
    vad: VoiceActivityDetector,
    asr: Recognizers,
    speaker: Option<SpeakerEmbeddingExtractor>,
    pending: Vec<f32>,
    /// The sentence being said, from the moment speech was detected and with some audio before it.
    speech: Vec<f32>,
    /// The loudness of each window of `speech`.
    loudness: Vec<f32>,
    /// Everything before this sample, counted like `fed`, was already heard.
    sent_until: usize,
    recent: Vec<f32>,
    in_speech: bool,
    speech_start: usize,
    fed: usize,
    level: VadLevel,
    /// The original samples: recognition must not hear the detector's gain.
    audio: VecDeque<f32>,
    waiting: Vec<(Vec<f32>, usize, Instant)>,
}

impl Ear {
    /// `models` holds `silero_vad.onnx`, `speaker/` and the recognizers' folders (see [`Recognizers::load`]).
    /// Without the speaker model, speech is still heard, but speakers are not told apart.
    pub fn load(models: &Path) -> Result<Self> {
        Self::with(models, Recognizers::load(models))
    }

    /// An ear with the recognizers given, and the detector and speaker model from `models`.
    pub fn with(models: &Path, asr: Recognizers) -> Result<Self> {
        if asr.is_empty() {
            return Err(anyhow!("no speech recognizer: neither a speech model nor the system's recognizer is there"));
        }
        let path = |p: &str| models.join(p).to_string_lossy().to_string();
        let vad_config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(path("silero_vad.onnx")),
                threshold: 0.5,
                min_silence_duration: MIN_SILENCE,
                min_speech_duration: 0.25,
                window_size: VAD_WINDOW as i32,
                max_speech_duration: MAX_SPEECH,
            },
            sample_rate: SAMPLE_RATE,
            num_threads: 1,
            ..Default::default()
        };
        let vad = VoiceActivityDetector::create(&vad_config, 60.0).context("loading the speech detector")?;
        let speaker = SpeakerEmbeddingExtractor::create(&SpeakerEmbeddingExtractorConfig { model: Some(path("speaker/campplus_zh_en.onnx")), num_threads: 1, ..Default::default() });
        if speaker.is_none() {
            eprintln!("the speaker model did not load, so speakers will not be told apart");
        }
        Ok(Self {
            vad,
            asr,
            speaker,
            pending: vec![],
            speech: vec![],
            loudness: vec![],
            sent_until: 0,
            recent: vec![],
            in_speech: false,
            speech_start: 0,
            fed: 0,
            level: VadLevel::default(),
            audio: VecDeque::new(),
            waiting: vec![],
        })
    }

    pub fn error(&self) -> Option<&str> {
        self.asr.system.as_ref().and_then(|s| s.error())
    }

    /// The actual system backend and locale, once an automatic language has been chosen.
    pub fn identity(&self) -> Option<(&str, &str)> {
        self.asr.system.as_ref().and_then(|s| s.identity())
    }

    pub fn feed(&mut self, samples: &[f32], heard: &mut impl Heard) {
        self.pending.extend_from_slice(samples);
        while self.pending.len() >= VAD_WINDOW {
            let chunk: Vec<f32> = self.pending.drain(..VAD_WINDOW).collect();
            self.fed += VAD_WINDOW;
            self.audio.extend(&chunk);
            let mut leveled = chunk.clone();
            self.level.apply(&mut leveled);
            self.vad.accept_waveform(&leveled);
            if self.vad.detected() && self.asr.system.as_mut().is_some_and(|s| s.prepare(&chunk, false)) {
                self.hear_waiting(heard);
            }

            if self.in_speech {
                self.speech.extend_from_slice(&chunk);
                self.loudness.push(rms(&chunk));
                if let Some(back) = soft_break(&self.loudness) {
                    self.soft_cut(back, heard);
                }
            } else {
                self.recent.extend_from_slice(&chunk);
                let extra = self.recent.len().saturating_sub(PREROLL);
                self.recent.drain(..extra);
            }
            if self.vad.detected() && !self.in_speech {
                self.in_speech = true;
                self.speech = std::mem::take(&mut self.recent);
                self.speech_start = self.fed - self.speech.len();
                self.loudness = self.speech.chunks(VAD_WINDOW).map(rms).collect();
            }
            if self.in_speech && self.fed - self.speech_start >= RETAIN - PREROLL {
                self.vad.flush();
            }
            self.drain(heard);
            let keep_from = if self.in_speech { self.sent_until } else { self.fed.saturating_sub(PREROLL) };
            let discard = keep_from.saturating_sub(self.fed - self.audio.len()).min(self.audio.len());
            self.audio.drain(..discard);
        }
    }

    /// End a sentence at a dip. What was sent stays recorded in `sent_until`, so the detector's own
    /// segment, which still holds it, is not heard twice.
    fn soft_cut(&mut self, back: usize, heard: &mut impl Heard) {
        let keep = (back * VAD_WINDOW).min(self.speech.len());
        let head: Vec<f32> = self.speech.drain(..self.speech.len() - keep).collect();
        let windows = self.loudness.len();
        self.loudness.drain(..windows - back.min(windows));
        self.sent_until = self.fed - keep;
        self.hear(&head, self.sent_until - head.len(), heard);
    }

    fn hear(&mut self, samples: &[f32], start: usize, heard: &mut impl Heard) {
        if samples.len() < VAD_WINDOW {
            return;
        }
        let ended = Instant::now();
        if self.asr.system.as_mut().is_some_and(|s| !s.prepare(&[], false)) {
            self.waiting.push((samples.to_vec(), start, ended));
            return;
        }
        self.hear_waiting(heard);
        self.transcribe(samples, start, ended, heard);
    }

    fn hear_waiting(&mut self, heard: &mut impl Heard) {
        for (samples, start, ended) in std::mem::take(&mut self.waiting) {
            self.transcribe(&samples, start, ended, heard);
        }
    }

    fn transcribe(&mut self, samples: &[f32], start: usize, ended: Instant, heard: &mut impl Heard) {
        let started = Instant::now();
        let (text, engine) = self.asr.hear(samples);
        // A word or two out of clicks, breath or hum: the recognizers find words in anything, but
        // speech is voiced, and these are not.
        if text.chars().count() <= NOISE_WORDS && voiced_share(samples) < MIN_VOICED {
            return;
        }
        if !text.is_empty() {
            let asr_ms = started.elapsed().as_millis();
            heard(Sentence { text, voiceprint: self.voiceprint(samples), asr_ms, ended, start: start as f64 / f64::from(SAMPLE_RATE), seconds: samples.len() as f64 / f64::from(SAMPLE_RATE), engine });
        }
    }

    fn drain(&mut self, heard: &mut impl Heard) {
        while let Some(segment) = self.vad.front() {
            let start = usize::try_from(segment.start()).unwrap_or(0);
            let length = segment.samples().len();
            let skip = self.sent_until.saturating_sub(start).min(length);
            let offset = (start + skip).saturating_sub(self.fed - self.audio.len());
            let samples: Vec<f32> = self.audio.iter().skip(offset).take(length - skip).copied().collect();
            self.hear(&samples, start + skip, heard);
            self.sent_until = start + length;
            self.vad.pop();
            self.in_speech = self.vad.detected();
            self.speech.clear();
            self.loudness.clear();
        }
        // The detector no longer hears speech but gave no segment, as it was too short to keep.
        if !self.vad.detected() && self.in_speech && self.vad.is_empty() {
            self.in_speech = false;
            self.speech.clear();
            self.loudness.clear();
        }
    }

    fn voiceprint(&self, samples: &[f32]) -> Option<Vec<f32>> {
        let extractor = self.speaker.as_ref()?;
        if samples.len() < MIN_VOICEPRINT {
            return None;
        }
        let stream = extractor.create_stream()?;
        stream.accept_waveform(SAMPLE_RATE, samples);
        stream.input_finished();
        if !extractor.is_ready(&stream) {
            return None;
        }
        let mut print = extractor.compute(&stream)?;
        let length = print.iter().map(|x| x * x).sum::<f32>().sqrt();
        if !length.is_finite() || length == 0.0 {
            return None;
        }
        print.iter_mut().for_each(|x| *x /= length);
        Some(print)
    }

    /// The input ended: whatever is still buffered counts as a sentence.
    pub fn finish(&mut self, heard: &mut impl Heard) {
        if !self.pending.is_empty() {
            self.feed(&vec![0.0; VAD_WINDOW - self.pending.len()], heard);
        }
        self.vad.flush();
        self.drain(heard);
        if !self.waiting.is_empty() && self.asr.system.as_mut().is_some_and(|s| s.prepare(&[], true)) {
            self.hear_waiting(heard);
        }
    }
}

/// Feed a mono WAV in real time, so timings resemble a live meeting.
pub fn play_wav(path: &Path, mut samples: impl FnMut(Vec<f32>) -> Result<()>) -> Result<()> {
    let wave = Wave::read(&path.to_string_lossy()).with_context(|| format!("cannot read {}", path.display()))?;
    anyhow::ensure!(wave.sample_rate() == SAMPLE_RATE, "recording must be 16 kHz");
    for chunk in wave.samples().chunks(320) {
        samples(chunk.to_vec())?;
        thread::sleep(Duration::from_millis(20));
    }
    samples(vec![0.0; SAMPLE_RATE as usize])?;
    Ok(())
}

fn input_host() -> cpal::Host {
    #[cfg(target_os = "linux")]
    return cpal::host_from_id(cpal::HostId::PulseAudio).unwrap_or_else(|_| cpal::default_host());
    #[cfg(not(target_os = "linux"))]
    cpal::default_host()
}

/// Something to listen to: a microphone, or on Linux the sound the computer is playing.
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    /// Stable across runs, for saving the choice.
    pub id: String,
    pub name: String,
}

/// What can be listened to now. On Linux, PulseAudio and PipeWire list each output's monitor here,
/// which is how the other side of an online meeting is heard.
pub fn inputs() -> Vec<Input> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let Ok(devices) = input_host().input_devices() else { return vec![] };
    let mut found: Vec<Input> = devices.filter_map(|d| Some(Input { id: d.id().ok()?.to_string(), name: d.to_string() })).collect();
    // Microphones first, then what the computer plays.
    found.sort_by_key(|i| i.id.ends_with(".monitor"));
    found
}

/// Live capture, mono at 16 kHz, until `stop` is set. `chosen` is an input's id; when it is gone,
/// the default input is used and `fell_back` hears about it.
pub fn capture(stop: &AtomicBool, mut samples: impl FnMut(Vec<f32>) -> Result<()> + Send + 'static, chosen: Option<&str>, fell_back: impl FnOnce(&str), ready: impl FnOnce()) -> Result<()> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = input_host();
    let picked = chosen.and_then(|id| host.input_devices().ok()?.find(|d| d.id().is_ok_and(|d| d.to_string() == id)));
    if let (Some(id), None) = (chosen, &picked) {
        fell_back(id);
    }
    let device = match picked {
        Some(device) => device,
        None => host.default_input_device().context("no microphone")?,
    };
    let supported = device.default_input_config()?;
    let channels = supported.channels() as usize;
    let ratio = f64::from(supported.sample_rate()) / f64::from(SAMPLE_RATE);
    let mut config = supported.config();
    if let cpal::SupportedBufferSize::Range { min, max } = *supported.buffer_size() {
        config.buffer_size = cpal::BufferSize::Fixed((supported.sample_rate() / 50).clamp(min, max));
    }
    let (errors, failure) = channel();
    let sending_errors = errors.clone();
    let mut position = 0.0_f64;
    let mut send = move |mono: Vec<f32>| {
        let mut out = Vec::with_capacity((mono.len() as f64 / ratio) as usize + 1);
        while (position as usize) < mono.len() {
            out.push(mono[position as usize]);
            position += ratio;
        }
        position -= mono.len() as f64;
        if !out.is_empty() {
            if let Err(e) = samples(out) { let _ = sending_errors.send(e.to_string()); }
        }
    };
    let on_error = move |e: cpal::Error| { let _ = errors.send(format!("microphone: {e}")); };
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream::<f32, _, _>(
            config,
            move |data: &[f32], _| send(data.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect()),
            on_error,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream::<i16, _, _>(
            config,
            move |data: &[i16], _| {
                send(data.chunks(channels).map(|f| f.iter().map(|&s| f32::from(s) / 32768.0).sum::<f32>() / channels as f32).collect());
            },
            on_error,
            None,
        )?,
        cpal::SampleFormat::I32 => device.build_input_stream::<i32, _, _>(
            config,
            move |data: &[i32], _| {
                send(data.chunks(channels).map(|f| f.iter().map(|&s| s as f32 / 2_147_483_648.0).sum::<f32>() / channels as f32).collect());
            },
            on_error,
            None,
        )?,
        other => return Err(anyhow!("unsupported sample format {other:?}")),
    };
    stream.play()?;
    ready();
    while !stop.load(Ordering::Relaxed) {
        match failure.recv_timeout(Duration::from_millis(50)) {
            Ok(error) => return Err(anyhow!(error)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_goes_to_parakeet_and_chinese_with_english_terms_does_not() {
        assert!(is_english("The gateway forwards the request."));
        assert!(!is_english("小程序把请求发给后端的异步 API。"));
        assert!(!is_english("Account 下面有 User API。"));
        assert!(!is_english("はい、そうです"));
        assert!(!is_english("1."));
        assert!(!is_english(""));
    }

    #[test]
    fn sense_voice_can_be_told_to_hear_chinese() {
        let Some(models) = models() else { return };
        let zh = load_sense_voice_for(&models.join("sense-voice"), "zh").unwrap();
        // Silence comes out as nothing, Korean least of all.
        assert!(!is_korean(&recognize(&zh, &vec![0.0; SAMPLE_RATE as usize])));
    }

    #[test]
    fn hangul_is_korean_and_nothing_else_is() {
        assert!(is_korean("어") && is_korean("네, 알겠습니다"));
        assert!(!is_korean("哦") && !is_korean("こんにちは") && !is_korean("Release notes") && !is_korean(""));
    }

    #[test]
    fn kana_goes_to_reazon_and_chinese_does_not() {
        assert!(is_japanese("はい、そうです"));
        assert!(is_japanese("サーバーを再起動します"));
        assert!(is_japanese("ｻｰﾊﾞｰ"));
        assert!(!is_japanese("小程序把请求发给后端"));
        assert!(!is_japanese("The gateway"));
    }

    struct Fixed(&'static str);

    impl Recognizer for Fixed {
        fn recognize(&mut self, _: &[f32]) -> Result<String> {
            Ok(self.0.to_string())
        }
    }

    #[test]
    fn the_system_recognizer_takes_every_sentence_and_nothing_is_no_ear() {
        let mut asr = Recognizers { system: Some(Box::new(Fixed("こんにちは"))), sense_voice: None, english: None, japanese: None, sense_voice_dir: None, chinese: None };
        assert_eq!(asr.hear(&[0.0; 16_000]), ("こんにちは".to_string(), Engine::System));
        let none = Recognizers { system: None, sense_voice: None, english: None, japanese: None, sense_voice_dir: None, chinese: None };
        assert!(none.is_empty());
        assert!(Ear::with(Path::new("/nowhere"), none).is_err());
    }

    #[test]
    fn opening_context_wait_keeps_sentence_audio_and_positions_and_flushes_on_stop() {
        use std::sync::{Arc, Mutex};
        struct Waiting { ready: Arc<AtomicBool>, seen: Arc<Mutex<Vec<Vec<f32>>>> }
        impl Recognizer for Waiting {
            fn prepare(&mut self, _: &[f32], finished: bool) -> bool {
                finished || self.ready.load(Ordering::Relaxed)
            }
            fn recognize(&mut self, samples: &[f32]) -> Result<String> {
                self.seen.lock().unwrap().push(samples.to_vec());
                Ok("The original sentence was retained.".into())
            }
        }
        let Some(models) = models() else { return };
        let first = vec![0.25; VAD_WINDOW * 2];
        let second = vec![-0.5; VAD_WINDOW * 2];
        for finish in [false, true] {
            let seen = Arc::new(Mutex::new(vec![]));
            let ready = Arc::new(AtomicBool::new(false));
            let mut ear = Ear::with(&models, Recognizers::system(Box::new(Waiting { ready: ready.clone(), seen: seen.clone() }))).unwrap();
            let mut sentences = vec![];
            ear.hear(&first, 1600, &mut |s| sentences.push(s));
            assert!(sentences.is_empty() && seen.lock().unwrap().is_empty());
            if finish { ear.finish(&mut |s| sentences.push(s)); }
            else { ready.store(true, Ordering::Relaxed); ear.hear(&second, 16000, &mut |s| sentences.push(s)); }
            let expected = if finish { vec![first.clone()] } else { vec![first.clone(), second.clone()] };
            assert_eq!(*seen.lock().unwrap(), expected);
            assert_eq!(sentences[0].start, 0.1);
            assert_eq!(sentences[0].seconds, first.len() as f64 / f64::from(SAMPLE_RATE));
            if !finish {
                assert_eq!(sentences[1].start, 1.0);
                assert!(sentences[0].ended <= sentences[1].ended);
            }
            assert!(ear.waiting.is_empty());
            ear.finish(&mut |_| panic!("deferred sentences must not be repeated"));
        }
    }

    #[test]
    fn the_meter_shows_quiet_speech_and_nothing_for_silence() {
        assert_eq!(loudness(0.0), 0.0);
        assert_eq!(loudness(1.0), 1.0);
        assert!(loudness(0.03) > 0.45, "{}", loudness(0.03));
        assert!(loudness(0.3) > loudness(0.03));
    }

    #[test]
    fn the_detector_gain_is_bounded_and_leaves_silence_and_normal_levels_alone() {
        let mut level = VadLevel::default();
        let mut silent = [0.0; VAD_WINDOW];
        level.apply(&mut silent);
        assert_eq!(silent, [0.0; VAD_WINDOW]);
        let mut quiet = [0.002; VAD_WINDOW];
        level.apply(&mut quiet);
        assert!(quiet[0] > 0.002 && quiet[0] < 0.003, "the gain must rise gradually");
        for _ in 0..100 {
            quiet.fill(0.002);
            level.apply(&mut quiet);
        }
        assert!(quiet.iter().all(|&x| (x - 0.016).abs() < 1e-6));
        let mut normal = [0.5, -0.8, 1.0, -1.0];
        level.apply(&mut normal);
        assert_eq!(normal, [0.5, -0.8, 1.0, -1.0]);
        // A brief dip after a loud frame must not immediately amplify the noise floor.
        let mut dip = [0.001; VAD_WINDOW];
        level.apply(&mut dip);
        assert_eq!(dip, [0.001; VAD_WINDOW]);
    }

    #[test]
    fn a_soft_break_needs_a_long_sentence_and_a_dip() {
        let loud = vec![0.1_f32; 200];
        assert_eq!(soft_break(&loud), None);
        let quiet_speech: Vec<f32> = loud.iter().map(|r| r * 0.01).collect();
        assert_eq!(soft_break(&quiet_speech), None, "quiet speech is not a pause");
        let mut short = vec![0.1_f32; 60];
        short.extend([0.001; SOFT_PAUSE]);
        assert_eq!(soft_break(&short), None);
        // Just past SOFT_MAX a gap between words is not enough, and a breath is.
        let mut just = vec![0.1_f32; 120];
        just.extend([0.001; 6]);
        assert_eq!(soft_break(&just), None);
        just.extend([0.001; SOFT_PAUSE - 6]);
        assert_eq!(soft_break(&just), Some(SOFT_PAUSE / 2));
        let quiet_pause: Vec<f32> = just.iter().map(|r| r * 0.01).collect();
        assert_eq!(soft_break(&quiet_pause), soft_break(&just));
        // Near MAX_SPEECH a short dip is enough.
        let mut late = vec![0.1_f32; 310];
        late.extend([0.001; SOFT_PAUSE_MIN]);
        assert_eq!(soft_break(&late), Some(SOFT_PAUSE_MIN / 2));
        // A dip relative to the speaker's level counts even in a loud room.
        let mut loud_room = vec![0.4_f32; 200];
        loud_room.extend([0.05; SOFT_PAUSE]);
        assert!(soft_break(&loud_room).is_some());
    }

    /// Models and speech, from `BOARDCAST_VOICE_DIR`. CI runs these with the packaged models.
    fn models() -> Option<std::path::PathBuf> {
        std::env::var_os("BOARDCAST_VOICE_DIR").map(std::path::PathBuf::from)
    }

    fn hear_all(ear: &mut Ear, samples: &[f32]) -> Vec<String> {
        let mut said = vec![];
        let mut heard = |sentence: Sentence| said.push(sentence.text);
        for chunk in samples.chunks(SAMPLE_RATE as usize / 10) {
            ear.feed(chunk, &mut heard);
        }
        ear.finish(&mut heard);
        said
    }

    fn clicks(amplitude: f32) -> Vec<f32> {
        let mut seed = 11_u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (f64::from(seed) / f64::from(u32::MAX) * 2.0 - 1.0) as f32
        };
        let mut v = vec![0.0; SAMPLE_RATE as usize * 4];
        let mut at = 1000;
        while at + 400 < v.len() {
            for k in 0..300 {
                v[at + k] += noise() * amplitude * (-(k as f32) / 40.0).exp();
            }
            at += 2000 + (noise().abs() * 2000.0) as usize;
        }
        v
    }

    #[test]
    fn a_voice_is_voiced_and_clicks_are_not() {
        let voice: Vec<f32> = (0..SAMPLE_RATE as usize)
            .map(|i| (1..=5).map(|h| (i as f32 * std::f32::consts::TAU * 150.0 * h as f32 / SAMPLE_RATE as f32).sin() / h as f32).sum::<f32>() * 0.2)
            .collect();
        assert!(voiced_share(&voice) > 0.9);
        assert!(voiced_share(&clicks(0.5)) < MIN_VOICED);
        assert_eq!(voiced_share(&[]), 0.0);
    }

    #[test]
    fn typing_on_a_keyboard_writes_nothing() {
        let Some(dir) = models() else { return };
        for amplitude in [0.05, 0.5] {
            let mut ear = Ear::load(&dir).unwrap();
            assert_eq!(hear_all(&mut ear, &clicks(amplitude)), Vec::<String>::new(), "clicks at {amplitude}");
        }
    }

    #[test]
    fn silence_and_noise_produce_nothing() {
        let Some(dir) = models() else { return };
        let mut ear = Ear::load(&dir).unwrap();
        assert_eq!(hear_all(&mut ear, &vec![0.0; SAMPLE_RATE as usize * 3]), Vec::<String>::new());
        // Broadband noise and hum, at levels the detector's gain will boost.
        for amplitude in [0.001, 0.01] {
            let mut seed = 7_u32;
            let noise: Vec<f32> = (0..SAMPLE_RATE as usize * 4)
                .map(|i| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let white = f64::from(seed) / f64::from(u32::MAX) * 2.0 - 1.0;
                    let hum = (i as f64 * std::f64::consts::TAU * 50.0 / f64::from(SAMPLE_RATE)).sin();
                    ((white + hum) * amplitude) as f32
                })
                .collect();
            let mut ear = Ear::load(&dir).unwrap();
            assert_eq!(hear_all(&mut ear, &noise), Vec::<String>::new());
        }
    }
}
