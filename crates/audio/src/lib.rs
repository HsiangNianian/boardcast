//! The audio chain: microphone or file -> 16 kHz mono -> Silero VAD -> SenseVoice and CAM++ -> a finished sentence.
//! It knows nothing about boards; callers receive each sentence through a callback.

use anyhow::{anyhow, Context, Result};
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig, OfflineTransducerModelConfig, SileroVadModelConfig, SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig, VadModelConfig, VoiceActivityDetector, Wave};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

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

/// SenseVoice, from `dir`.
pub fn load_sense_voice(dir: &Path) -> Result<OfflineRecognizer> {
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.sense_voice = OfflineSenseVoiceModelConfig { model: Some(dir.join("model.int8.onnx").to_string_lossy().to_string()), language: Some("auto".into()), use_itn: true };
    config.model_config.tokens = Some(dir.join("tokens.txt").to_string_lossy().to_string());
    config.model_config.num_threads = 2;
    OfflineRecognizer::create(&config).context("loading SenseVoice")
}

/// Recognize a whole recording with one engine, for comparing them on the same audio.
pub fn transcribe(asr: &OfflineRecognizer, samples: &[f32]) -> String {
    recognize(asr, samples)
}

pub struct Ear {
    vad: VoiceActivityDetector,
    asr: OfflineRecognizer,
    /// Recognizes English again, as it does English better. Without its files, SenseVoice does everything.
    english: Option<OfflineRecognizer>,
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
}

impl Ear {
    /// `models` holds `silero_vad.onnx`, `sense-voice/`, `speaker/` and `parakeet-en/`. Without the last two,
    /// speech is still heard, but speakers are not told apart and English is left to SenseVoice.
    pub fn load(models: &Path) -> Result<Self> {
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
        let mut asr_config = OfflineRecognizerConfig::default();
        asr_config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(path("sense-voice/model.int8.onnx")),
            language: Some("auto".into()),
            use_itn: true,
        };
        asr_config.model_config.tokens = Some(path("sense-voice/tokens.txt"));
        asr_config.model_config.num_threads = 2;
        let asr = OfflineRecognizer::create(&asr_config).context("loading SenseVoice")?;
        let english = load_parakeet(&models.join("parakeet-en"));
        let speaker = SpeakerEmbeddingExtractor::create(&SpeakerEmbeddingExtractorConfig { model: Some(path("speaker/campplus_zh_en.onnx")), num_threads: 1, ..Default::default() });
        if speaker.is_none() {
            eprintln!("the speaker model did not load, so speakers will not be told apart");
        }
        Ok(Self {
            vad,
            asr,
            english,
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
        })
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
        let mut text = recognize(&self.asr, samples);
        let mut engine = Engine::SenseVoice;
        if let Some(english) = self.english.as_ref().filter(|_| is_english(&text)) {
            let again = recognize(english, samples);
            if !again.is_empty() {
                (text, engine) = (again, Engine::Parakeet);
            }
        }
        if !text.is_empty() {
            let asr_ms = ended.elapsed().as_millis();
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

/// Live microphone capture, mono at 16 kHz, until `stop` is set.
pub fn capture(stop: &AtomicBool, samples: &Sender<Vec<f32>>) -> Result<()> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    #[cfg(target_os = "linux")]
    let host = cpal::host_from_id(cpal::HostId::PulseAudio).unwrap_or_else(|_| cpal::default_host());
    #[cfg(not(target_os = "linux"))]
    let host = cpal::default_host();
    let device = host.default_input_device().context("no microphone")?;
    let supported = device.default_input_config()?;
    let channels = supported.channels() as usize;
    let ratio = f64::from(supported.sample_rate()) / f64::from(SAMPLE_RATE);
    let mut config = supported.config();
    if let cpal::SupportedBufferSize::Range { min, max } = *supported.buffer_size() {
        config.buffer_size = cpal::BufferSize::Fixed((supported.sample_rate() / 50).clamp(min, max));
    }
    let tx = samples.clone();
    let mut position = 0.0_f64;
    let mut send = move |mono: Vec<f32>| {
        let mut out = Vec::with_capacity((mono.len() as f64 / ratio) as usize + 1);
        while (position as usize) < mono.len() {
            out.push(mono[position as usize]);
            position += ratio;
        }
        position -= mono.len() as f64;
        let _ = tx.send(out);
    };
    let on_error = |e: cpal::Error| eprintln!("microphone: {e}");
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
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(50));
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
