//! The audio chain: microphone or file -> 16 kHz mono -> Silero VAD -> split where the speaker changes ->
//! a recognizer and CAM++ -> a finished sentence. Any of the recognizers may be missing; SenseVoice, when
//! there, decides which language a sentence is in.
//! It knows nothing about boards; callers receive each sentence through a callback.

/// A meeting's audio kept as it is heard, and read back.
pub use alcyo_keep as keep;
pub mod play;
use anyhow::{anyhow, Context, Result};
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig, OfflineTransducerModelConfig, SileroVadModelConfig, SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig, VadModelConfig, VoiceActivityDetector, Wave};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

pub mod system;
mod listen;
mod turns;
pub use turns::Latency;
mod drafts;
pub use drafts::{Draft, DraftWith, MODEL as DRAFT_MODEL};
pub use listen::{capture, computer_sounds, microphones, Input, Listen, Sources, Warning};

pub const SAMPLE_RATE: i32 = alcyo_keep::SAMPLE_RATE;
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

/// Below this a window of the sentence is quiet: a share of its median loudness. The quiet-room floor stays
/// below speech itself; an absolute floor cuts quiet words in half.
fn quiet_level(loudness: &[f32]) -> f32 {
    if loudness.is_empty() {
        return 0.0;
    }
    let mut sorted = loudness.to_vec();
    sorted.sort_by(f32::total_cmp);
    let median = sorted[loudness.len() / 2];
    (median * QUIET_RATIO).max(0.003_f32.min(median * 0.5))
}

/// What two readings agree on from the start, ending at the last whole word or character they share. A
/// draft built of it never takes back what it showed, unless a later reading disagrees with both.
fn agreed_start(before: &str, after: &str) -> String {
    let shared: String = before.chars().zip(after.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a).collect();
    if shared.chars().count() == after.chars().count() {
        return shared;
    }
    // A Latin word cut in the middle is not shown until the readings agree on all of it.
    match shared.char_indices().rev().find(|(_, c)| !c.is_ascii_alphanumeric()) {
        Some((i, c)) if after.as_bytes().get(shared.len()).is_some_and(u8::is_ascii_alphanumeric) => shared[..i + c.len_utf8()].trim_end().to_string(),
        None if after.as_bytes().get(shared.len()).is_some_and(u8::is_ascii_alphanumeric) => String::new(),
        _ => shared.trim_end().to_string(),
    }
}

/// How many windows at the start are quiet, before speech begins, keeping one of them in front of it. Quiet
/// is 20 dB below the loud part, its 90th percentile: the audio kept from before speech was detected is
/// mostly silence, so a share of its median would be a share of the silence itself.
fn quiet_lead(loudness: &[f32]) -> usize {
    if loudness.is_empty() {
        return 0;
    }
    let mut sorted = loudness.to_vec();
    sorted.sort_by(f32::total_cmp);
    let loud = sorted[(sorted.len() - 1) * 9 / 10];
    let quiet = loud * ONSET_RATIO;
    loudness.iter().take_while(|&&r| r < quiet).count().saturating_sub(1)
}

/// Given each window's loudness, whether the sentence can end at the dip it finishes on, and how many
/// windows at the end belong to the next one.
fn soft_break(loudness: &[f32]) -> Option<usize> {
    let n = loudness.len();
    if n * VAD_WINDOW < (SOFT_MAX * SAMPLE_RATE as f32) as usize {
        return None;
    }
    let quiet = quiet_level(loudness);
    let run = loudness.iter().rev().take_while(|&&r| r < quiet).count();
    // One window less per second past SOFT_MAX.
    let over = (n * VAD_WINDOW) as f32 / SAMPLE_RATE as f32 - SOFT_MAX;
    let need = (SOFT_PAUSE as f32 - over).round().clamp(SOFT_PAUSE_MIN as f32, SOFT_PAUSE as f32) as usize;
    // After a cut only the tail is kept, so this does not fire again on the same dip.
    (run >= need).then_some(run / 2)
}

/// The latency the diarization model runs at; see [`Latency`]. `ALCYO_DIARIZATION` set to slow, medium,
/// fast, long or off chooses another without building again: `long` halves the model's work for 0.35 s
/// more on each line, for a slow machine.
const LATENCY: Latency = Latency::Slow;

/// The latency from `ALCYO_DIARIZATION`, or [`LATENCY`]; `None` when it is off.
fn chosen_latency() -> Option<Latency> {
    match std::env::var("ALCYO_DIARIZATION").ok().as_deref().map(str::trim) {
        Some("off") => None,
        Some("slow") => Some(Latency::Slow),
        Some("medium") => Some(Latency::Medium),
        Some("fast") => Some(Latency::Fast),
        Some("long") => Some(Latency::Long),
        Some(other) if !other.is_empty() => {
            eprintln!("ALCYO_DIARIZATION is slow, medium, fast, long or off, not {other}; using the default");
            Some(LATENCY)
        }
        _ => Some(LATENCY),
    }
}

/// How long the diarization model keeps hearing after speech, in samples. Pauses between sentences stay in
/// what it hears: left out, it judged turns worse (F1 0.889 to 0.877 on the speaker eval). Only a longer
/// silence, as in a break, is left out, and it needs at least its latency and a step to give out the frames
/// at the end of a sentence.
const TAIL: usize = SAMPLE_RATE as usize * 10;
/// A window this far below the loud part of the audio before speech, 20 dB, is silence the diarization
/// model need not hear.
const ONSET_RATIO: f32 = 0.1;
/// A piece of a sentence shorter than this, in samples, is too short to read: a quarter second.
const MIN_PIECE: usize = SAMPLE_RATE as usize / 4;

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
    /// The diarization model's speaker slot, which stays with one person while at most eight talk. It counts
    /// on past 7 after the ear was finished and heard again, as the model then starts over and a slot may
    /// be someone else. Missing without the model, or when it heard nobody.
    pub slot: Option<usize>,
    /// Other slots heard talking at the same time, whose words are lost under this one's: speech over
    /// speech cannot be taken apart, but who interrupted can be shown.
    pub also: Vec<usize>,
    /// The stretch of speech it came from, which its drafts share (see [`Draft`]).
    pub utterance: usize,
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

/// The language a meeting is held in, as chosen in Settings. English is heard whatever is chosen, as
/// meetings in Chinese or Japanese are full of it; the choice settles what everything else is heard as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    /// SenseVoice tells each sentence's language and passes it on.
    #[default]
    Auto,
    /// Kana and Hangul are Chinese misheard, and are heard again as Chinese.
    Chinese,
    /// Parakeet hears everything.
    English,
    /// ReazonSpeech hears everything that isn't English, kana or not.
    Japanese,
}

impl Language {
    /// From the setting, which is also the system recognizer's locale: "zh-CN", "en-US", "ja-JP" or "auto".
    pub fn from_setting(setting: &str) -> Self {
        match setting.split(['-', '_']).next() {
            Some("zh") => Self::Chinese,
            Some("en") => Self::English,
            Some("ja") => Self::Japanese,
            _ => Self::Auto,
        }
    }
}

/// Who reads a sentence again after its first reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Again {
    English,
    Japanese,
    Chinese,
}

/// Who reads a sentence again, from what the first reading made of it. `long` is whether it is long
/// enough for ReazonSpeech.
fn read_again(language: Language, engine: Engine, text: &str, long: bool) -> Option<Again> {
    match engine {
        Engine::SenseVoice | Engine::Reazon if is_english(text) => Some(Again::English),
        Engine::SenseVoice if is_korean(text) || (language == Language::Chinese && is_japanese(text)) => Some(Again::Chinese),
        Engine::SenseVoice if long && (language == Language::Japanese || (language == Language::Auto && is_japanese(text))) => Some(Again::Japanese),
        _ => None,
    }
}

/// Whether text heard again as Chinese still isn't, and is better left out.
fn still_not_chinese(language: Language, text: &str) -> bool {
    is_korean(text) || (language == Language::Chinese && is_japanese(text))
}

/// Shorter than this, ReazonSpeech makes up words, so SenseVoice's reading stands.
const MIN_JAPANESE: usize = SAMPLE_RATE as usize * 3 / 10;

/// Whether recognized text is Chinese: it has Han characters, and no kana or Hangul, and is not mostly
/// English.
pub fn is_chinese(text: &str) -> bool {
    text.chars().any(|c| matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF)) && !is_japanese(text) && !is_korean(text) && !is_english(text)
}

/// Whether recognized text is Japanese: it has kana. Japanese written only in kanji is too rare to matter,
/// and telling it from Chinese would take more than the text.
pub fn is_japanese(text: &str) -> bool {
    text.chars().any(|c| matches!(c as u32, 0x3040..=0x309F | 0x30A0..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9D))
}

/// A recognizer outside this crate, such as the system's own, that takes a sentence and gives its text.
pub trait Recognizer: Send {
    fn recognize(&mut self, samples: &[f32]) -> Result<String>;
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
    /// The language chosen, which settles who hears what.
    pub language: Language,
}

impl Recognizers {
    /// Whatever models are in `models`: `sense-voice/`, `parakeet-en/` and `reazon-ja/`.
    pub fn load(models: &Path) -> Self {
        let dir = models.join("sense-voice");
        let sense_voice = load_sense_voice_if_there(&dir);
        let sense_voice_dir = sense_voice.is_some().then_some(dir);
        Self { system: None, sense_voice, english: load_parakeet(&models.join("parakeet-en")), japanese: load_reazon(&models.join("reazon-ja")), sense_voice_dir, chinese: None, language: Language::Auto }
    }

    /// The same recognizers, hearing `language`.
    pub fn hearing(self, language: Language) -> Self {
        Self { language, ..self }
    }

    /// Only the system's recognizer, for when it was chosen and nothing else needs loading.
    pub fn system(recognizer: Box<dyn Recognizer>) -> Self {
        Self { system: Some(recognizer), sense_voice: None, english: None, japanese: None, sense_voice_dir: None, chinese: None, language: Language::Auto }
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
        if self.language == Language::English {
            if let Some(english) = &self.english {
                return (recognize(english, samples), Engine::Parakeet);
            }
        }
        let (text, engine) = match (&self.sense_voice, &self.japanese, &self.english) {
            (Some(asr), _, _) => (recognize(asr, samples), Engine::SenseVoice),
            (None, Some(japanese), _) if samples.len() >= MIN_JAPANESE => (recognize(japanese, samples), Engine::Reazon),
            (None, Some(_), None) => return (String::new(), Engine::Reazon),
            (None, _, Some(english)) => (recognize(english, samples), Engine::Parakeet),
            (None, _, None) => return (String::new(), Engine::SenseVoice),
        };
        // What the first reading says about the language decides who reads it again.
        let better = match read_again(self.language, engine, &text, samples.len() >= MIN_JAPANESE) {
            Some(Again::Chinese) => return (self.as_chinese(samples), Engine::SenseVoice),
            Some(Again::English) => self.english.as_ref().map(|e| (e, Engine::Parakeet)),
            Some(Again::Japanese) => self.japanese.as_ref().map(|j| (j, Engine::Reazon)),
            None => None,
        };
        if let Some((asr, better)) = better {
            let text = recognize(asr, samples);
            if !text.is_empty() {
                return (text, better);
            }
        }
        (text, engine)
    }
}

impl Recognizers {
    /// Whether a draft can be read: by SenseVoice, or by the one recognizer a package for one language has.
    fn can_draft(&self) -> bool {
        self.sense_voice.is_some() || self.japanese.is_some() || self.english.is_some()
    }

    /// The speech so far read once, for a draft: by SenseVoice when it is there, as it is fast and hears
    /// every language, otherwise by the recognizer there is.
    fn draft(&self, samples: &[f32]) -> String {
        match (&self.sense_voice, &self.japanese, &self.english) {
            (Some(asr), _, _) => recognize(asr, samples),
            // ReazonSpeech fails on audio this short rather than reading nothing.
            (None, Some(_), _) if samples.len() < MIN_JAPANESE => String::new(),
            (None, Some(asr), _) | (None, None, Some(asr)) => recognize(asr, samples),
            _ => String::new(),
        }
    }

    /// A sentence SenseVoice heard as Korean, or as Japanese when Chinese was chosen, is Chinese it
    /// misheard: read it again as Chinese. Should that fail as well, nothing is written rather than the
    /// wrong language.
    fn as_chinese(&mut self, samples: &[f32]) -> String {
        if self.chinese.is_none() {
            self.chinese = self.sense_voice_dir.as_ref().and_then(|dir| load_sense_voice_for(dir, "zh").map_err(|e| eprintln!("{e:#}")).ok());
        }
        let again = self.chinese.as_ref().map(|zh| recognize(zh, samples)).unwrap_or_default();
        if still_not_chinese(self.language, &again) { String::new() } else { again }
    }
}

/// How an ear drafts.
enum Drafter {
    Reread,
    Stream(drafts::Stream),
    /// The stream for Chinese, reading again otherwise.
    ByLanguage(drafts::Stream),
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
    /// Who is talking, when the diarization model is there.
    turns: Option<turns::Turns>,
    /// The diarization model to load, with the folder it is in, once the first audio comes: loading waits
    /// until then so that choosing another latency first never loads two.
    turns_wanted: Option<(PathBuf, Latency)>,
    /// When speech last ended, counted like `fed`: the diarization model hears on for `TAIL` after it.
    quiet_since: usize,
    /// Wait for the diarization model at every step, rather than take what it has given so far.
    wait_for_turns: bool,
    /// How drafts are made while speech goes on, and who is told of them.
    drafter: Option<Drafter>,
    on_draft: Option<Box<dyn FnMut(Draft) + Send>>,
    /// The stretch of speech being heard now, counting from 0: a sentence ends it.
    utterance: usize,
    /// Where it began to be loud, counted like `fed`.
    onset: usize,
    /// The last draft given and how much speech it read, so the next is given only when it says more.
    last_draft: (usize, usize, String),
    /// Time spent drafting, to see what it costs.
    drafting: Duration,
    /// Whether the last sentence heard was Chinese, which in automatic mode settles how the next is drafted;
    /// before any, it is read again, which drafts every language.
    last_chinese: bool,
    /// The stretch being drafted and whether it is drafted by the stream.
    draft_streams: (usize, bool),
    /// The last reading of the speech so far, when drafts are read again: only what two readings in a row
    /// agree on is shown, as a recognizer reading a sentence whole often changes its start once it hears
    /// more.
    last_reading: (usize, String),
    /// Sentences waiting for `turns` to know who said them, with where they start.
    waiting: VecDeque<(Vec<f32>, usize, usize)>,
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
        let turns_wanted = chosen_latency().map(|latency| (models.to_path_buf(), latency));
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
            turns: None,
            turns_wanted,
            quiet_since: 0,
            drafter: None,
            on_draft: None,
            utterance: 0,
            onset: 0,
            last_draft: (usize::MAX, 0, String::new()),
            drafting: Duration::ZERO,
            last_reading: (usize::MAX, String::new()),
            last_chinese: false,
            draft_streams: (usize::MAX, false),
            wait_for_turns: false,
            waiting: VecDeque::new(),
        })
    }

    /// Tell speakers apart at this latency instead, or not split sentences at all with `None`.
    pub fn turns(mut self, models: &Path, latency: Option<Latency>) -> Self {
        self.turns = None;
        self.turns_wanted = latency.map(|l| (models.to_path_buf(), l));
        self
    }

    /// Draft what is being said while it is said, `with` the way given, and tell `on_draft` of each draft
    /// that says more than the last. Without a recognizer able to draft, nothing is drafted.
    pub fn drafts(mut self, with: DraftWith, on_draft: Box<dyn FnMut(Draft) + Send>) -> Self {
        self.drafter = match with {
            DraftWith::Reread => self.asr.can_draft().then_some(Drafter::Reread),
            DraftWith::Stream(dir) => drafts::Stream::load(&dir).map(Drafter::Stream).or_else(|| {
                eprintln!("the streaming model in {} did not load, so nothing is drafted", dir.display());
                None
            }),
            // A package without the streaming model, such as one for English or Japanese only, reads again.
            DraftWith::ByLanguage(dir) => match drafts::Stream::load(&dir) {
                Some(stream) if self.asr.can_draft() => Some(Drafter::ByLanguage(stream)),
                _ => self.asr.can_draft().then_some(Drafter::Reread),
            },
        };
        // The system's recognizer drafts on its own, once it can (#291).
        if self.asr.system.is_some() {
            self.drafter = None;
        }
        self.on_draft = Some(on_draft);
        self
    }

    /// The time spent drafting so far, in seconds.
    pub fn drafting_seconds(&self) -> f64 {
        self.drafting.as_secs_f64()
    }

    /// Wait for the diarization model at every step, which runs on its own thread, as a recording heard
    /// faster than real time needs: lines then come out when they would live, on a machine fast enough,
    /// and the results do not depend on how busy this one is. Without it, input given faster than real
    /// time piles up for the model and soon counts as a machine too slow for it.
    pub fn wait_for_turns(mut self) -> Self {
        self.wait_for_turns = true;
        self
    }

    pub fn feed(&mut self, samples: &[f32], heard: &mut impl Heard) {
        if let Some((models, latency)) = self.turns_wanted.take() {
            self.turns = turns::Turns::load(&models, latency);
            if self.turns.is_none() {
                eprintln!("the diarization model did not load, so a sentence is not split between speakers");
            }
        }
        self.pending.extend_from_slice(samples);
        while self.pending.len() >= VAD_WINDOW {
            let chunk: Vec<f32> = self.pending.drain(..VAD_WINDOW).collect();
            self.fed += VAD_WINDOW;
            self.audio.extend(&chunk);
            let was_speaking = self.in_speech;
            let mut leveled = chunk.clone();
            self.level.apply(&mut leveled);
            self.vad.accept_waveform(&leveled);

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
                self.onset = self.speech_start + quiet_lead(&self.loudness) * VAD_WINDOW;
            }
            if self.in_speech {
                self.draft();
            }
            if self.in_speech && self.fed - self.speech_start >= RETAIN - PREROLL {
                self.vad.flush();
            }
            self.drain(heard);
            if was_speaking && !self.in_speech {
                self.quiet_since = self.fed;
            }
            self.feed_turns();
            self.release(false, heard);
            let keep_from = if self.in_speech { self.sent_until } else { self.fed.saturating_sub(PREROLL) };
            let discard = keep_from.saturating_sub(self.fed - self.audio.len()).min(self.audio.len());
            self.audio.drain(..discard);
            let oldest = self.waiting.front().map_or(keep_from, |w| w.1.min(keep_from));
            if let Some(turns) = self.turns.as_mut() {
                turns.forget_before(oldest);
            }
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
        self.onset = self.sent_until;
    }

    /// Give the diarization model what it should hear of the input so far: speech, from the first window
    /// loud enough to belong to it, and up to `TAIL` of quiet after it. A longer silence, as in a break, is
    /// left out, as the model costs about half of real time, so a meeting can be left listening.
    fn feed_turns(&mut self) {
        let Some(turns) = self.turns.as_mut() else { return };
        if !self.in_speech && self.fed >= self.quiet_since + TAIL {
            return;
        }
        let kept_from = self.fed - self.audio.len();
        let mut from = turns.heard_until().unwrap_or(0).max(self.fed - VAD_WINDOW).max(kept_from);
        if self.in_speech {
            // The audio before the detector heard speech, from its first loud window.
            let onset = self.speech_start + quiet_lead(&self.loudness) * VAD_WINDOW;
            from = turns.heard_until().unwrap_or(0).max(onset).max(kept_from).min(from);
        }
        if from >= self.fed {
            return;
        }
        let samples: Vec<f32> = self.audio.iter().skip(from - kept_from).copied().collect();
        turns.accept(from, &samples);
    }

    /// Draft the speech heard so far, when a draft is wanted and there is more to say.
    fn draft(&mut self) {
        if self.on_draft.is_none() || self.drafter.is_none() {
            return;
        }
        let kept_from = self.fed - self.speech.len();
        let from = self.onset.saturating_sub(kept_from).min(self.speech.len());
        let spoken = self.speech.len() - from;
        let (utterance, read, _) = self.last_draft;
        // How this stretch is drafted is settled when it begins, so a draft never changes hands midway.
        if self.draft_streams.0 != self.utterance {
            let chinese = match self.asr.language {
                Language::Chinese => true,
                Language::English | Language::Japanese => false,
                Language::Auto => self.last_chinese,
            };
            self.draft_streams = (self.utterance, chinese);
        }
        let began = Instant::now();
        let text = match self.drafter.as_mut() {
            Some(Drafter::Stream(stream)) => stream.hear(self.utterance, &self.speech[from..]),
            Some(Drafter::ByLanguage(stream)) if self.draft_streams.1 => stream.hear(self.utterance, &self.speech[from..]),
            Some(Drafter::Reread | Drafter::ByLanguage(_)) => {
                // Reading again costs as much as all the speech so far, so the longer it runs, the more must
                // come before the next read: a quarter more, and never less than a step.
                // The first read waits for a step of speech too: a recognizer reading a few windows reads
                // nothing, and ReazonSpeech fails outright on them.
                if spoken < drafts::REREAD_STEP || (utterance == self.utterance && spoken < read + drafts::REREAD_STEP.max(read / 4)) {
                    return;
                }
                let reading = self.asr.draft(&self.speech[from..]);
                let agreed = if self.last_reading.0 == self.utterance { agreed_start(&self.last_reading.1, &reading) } else { String::new() };
                self.last_reading = (self.utterance, reading);
                agreed
            }
            None => return,
        };
        self.drafting += began.elapsed();
        let same = utterance == self.utterance && text == self.last_draft.2;
        self.last_draft = (self.utterance, spoken, text.clone());
        if text.is_empty() || same {
            return;
        }
        let seconds = |at: usize| at as f64 / f64::from(SAMPLE_RATE);
        let draft = Draft { text, utterance: self.utterance, spoken_from: seconds(self.onset), at: seconds(self.fed) };
        if let Some(tell) = self.on_draft.as_mut() {
            tell(draft);
        }
    }

    /// A sentence the detector cut out: heard now, or once the diarization model knows who said it. It
    /// ends the stretch of speech its drafts were of.
    fn hear(&mut self, samples: &[f32], start: usize, heard: &mut impl Heard) {
        let utterance = self.utterance;
        self.utterance += 1;
        if samples.len() < VAD_WINDOW {
            return;
        }
        if self.turns.is_some() {
            self.waiting.push_back((samples.to_vec(), start, utterance));
            self.release(false, heard);
        } else {
            self.hear_piece(samples, start, None, vec![], utterance, heard);
        }
    }

    /// Hear the waiting sentences whose speakers are known, or all of them at the end, each split where
    /// the speaker changes.
    fn release(&mut self, all: bool, heard: &mut impl Heard) {
        let Some(turns) = self.turns.as_mut() else {
            // Turned off while sentences waited: they are heard whole.
            while let Some((samples, start, utterance)) = self.waiting.pop_front() {
                self.hear_piece(&samples, start, None, vec![], utterance, heard);
            }
            return;
        };
        turns.take(self.wait_for_turns);
        // Waiting at every step is for measuring, where the machine's load must not change the result.
        if !self.wait_for_turns && turns.too_slow() {
            eprintln!("the diarization model cannot keep up on this machine, so sentences are no longer split between speakers");
            self.turns = None;
            return self.release(all, heard);
        }
        if turns.stalled() {
            eprintln!("the diarization model stopped answering, so sentences are no longer split between speakers");
            self.turns = None;
            return self.release(all, heard);
        }
        while let Some((samples, start, _)) = self.waiting.front() {
            let Some(turns) = self.turns.as_ref() else { break };
            if !all && start + samples.len() > turns.known_until(self.fed) {
                break;
            }
            let pieces = turns.split(*start, samples.len());
            let (samples, start, utterance) = self.waiting.pop_front().expect("a waiting sentence");
            for piece in pieces {
                if piece.len >= MIN_PIECE {
                    self.hear_piece(&samples[piece.from..piece.from + piece.len], start + piece.from, piece.slot, piece.also, utterance, heard);
                }
            }
        }
    }

    fn hear_piece(&mut self, samples: &[f32], start: usize, slot: Option<usize>, also: Vec<usize>, utterance: usize, heard: &mut impl Heard) {
        let ended = Instant::now();
        let (text, engine) = self.asr.hear(samples);
        // A word or two out of clicks, breath or hum: the recognizers find words in anything, but
        // speech is voiced, and these are not.
        if text.chars().count() <= NOISE_WORDS && voiced_share(samples) < MIN_VOICED {
            return;
        }
        if !text.is_empty() {
            self.last_chinese = is_chinese(&text);
            let asr_ms = ended.elapsed().as_millis();
            heard(Sentence { text, voiceprint: self.voiceprint(samples), asr_ms, ended, start: start as f64 / f64::from(SAMPLE_RATE), seconds: samples.len() as f64 / f64::from(SAMPLE_RATE), engine, slot, also, utterance });
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
        if let Some(turns) = self.turns.as_mut() {
            turns.finish();
        }
        self.release(true, heard);
        // The ear may hear again, as when listening resumes: that is a new stream for the model.
        if let Some(turns) = self.turns.as_mut() {
            turns.restart();
        }
    }
}

/// Feed a mono WAV in real time, so timings resemble a live meeting.
pub fn play_wav(path: &Path, samples: &Sender<Vec<f32>>) -> Result<()> {
    let wave = Wave::read(&path.to_string_lossy()).with_context(|| format!("cannot read {}", path.display()))?;
    for chunk in wave.samples().chunks(320) {
        let _ = samples.send(chunk.to_vec());
        thread::sleep(Duration::from_millis(20));
    }
    let _ = samples.send(vec![0.0; SAMPLE_RATE as usize]);
    Ok(())
}

#[cfg(test)]
mod onset_tests {
    use super::*;

    #[test]
    fn chinese_is_told_from_japanese_korean_and_english() {
        assert!(is_chinese("我们下周二再发布"));
        assert!(is_chinese("这个 API 要改一下"));
        assert!(!is_chinese("会議を始めます"));
        assert!(!is_chinese("회의를 시작합니다"));
        assert!(!is_chinese("Let's ship it on Friday."));
    }

    #[test]
    fn a_draft_shows_what_two_readings_agree_on() {
        assert_eq!(agreed_start("今天天气", "今天天气很好"), "今天天气");
        assert_eq!(agreed_start("今天天汽很", "今天天气很好"), "今天天");
        assert_eq!(agreed_start("we will ship on fri", "we will ship on friday"), "we will ship on");
        assert_eq!(agreed_start("hello", "help me"), "");
        assert_eq!(agreed_start("", "今天"), "");
    }

    #[test]
    fn the_silence_before_speech_is_left_out_but_one_window() {
        // A second of quiet room, then a third of a second of speech.
        let mut loudness = vec![0.002_f32; 31];
        loudness.extend([0.05, 0.08, 0.1, 0.09, 0.07, 0.1, 0.08, 0.06, 0.09, 0.1]);
        assert_eq!(quiet_lead(&loudness), 30);
    }

    #[test]
    fn a_soft_start_is_kept() {
        // Speech that rises slowly from the room: its quiet beginning is still well above the room.
        let mut loudness = vec![0.002_f32; 10];
        loudness.extend([0.012, 0.03, 0.06, 0.1, 0.1, 0.1]);
        assert_eq!(quiet_lead(&loudness), 9);
    }

    #[test]
    fn nothing_is_left_out_of_a_loud_room() {
        assert_eq!(quiet_lead(&[0.05; 20]), 0);
        assert_eq!(quiet_lead(&[]), 0);
    }
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
    fn the_setting_names_the_language() {
        assert_eq!(Language::from_setting("zh-CN"), Language::Chinese);
        assert_eq!(Language::from_setting("en-US"), Language::English);
        assert_eq!(Language::from_setting("ja_JP"), Language::Japanese);
        assert_eq!(Language::from_setting("auto"), Language::Auto);
        assert_eq!(Language::from_setting(""), Language::Auto);
    }

    #[test]
    fn the_language_chosen_decides_who_reads_again() {
        use Language::*;
        let sv = Engine::SenseVoice;
        // English is heard whatever was chosen.
        for language in [Auto, Chinese, Japanese] {
            assert_eq!(read_again(language, sv, "The gateway forwards it.", true), Some(Again::English), "{language:?}");
        }
        // Kana goes to ReazonSpeech when told apart, and is misheard Chinese when Chinese was chosen.
        assert_eq!(read_again(Auto, sv, "はい、そうです", true), Some(Again::Japanese));
        assert_eq!(read_again(Chinese, sv, "はい、そうです", true), Some(Again::Chinese));
        // Japanese chosen: ReazonSpeech hears what SenseVoice took for Chinese too, but not a short sound.
        assert_eq!(read_again(Japanese, sv, "会議を始めます", true), Some(Again::Japanese));
        assert_eq!(read_again(Japanese, sv, "会議", false), None);
        assert_eq!(read_again(Auto, sv, "会议开始", true), None);
        // Hangul is misheard Chinese, as before.
        assert_eq!(read_again(Auto, sv, "네", true), Some(Again::Chinese));
        // Chinese with English terms stays with SenseVoice.
        assert_eq!(read_again(Chinese, sv, "Account 下面有 User API。", true), None);
        assert!(still_not_chinese(Chinese, "はい") && !still_not_chinese(Auto, "はい") && still_not_chinese(Auto, "네"));
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
        let mut asr = Recognizers { system: Some(Box::new(Fixed("こんにちは"))), sense_voice: None, english: None, japanese: None, sense_voice_dir: None, chinese: None, language: Language::Auto };
        assert_eq!(asr.hear(&[0.0; 16_000]), ("こんにちは".to_string(), Engine::System));
        let none = Recognizers { system: None, sense_voice: None, english: None, japanese: None, sense_voice_dir: None, chinese: None, language: Language::Auto };
        assert!(none.is_empty());
        assert!(Ear::with(Path::new("/nowhere"), none).is_err());
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

    /// Models and speech, from `ALCYO_VOICE_DIR`. CI runs these with the packaged models.
    fn models() -> Option<std::path::PathBuf> {
        std::env::var_os("ALCYO_VOICE_DIR").map(std::path::PathBuf::from)
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
