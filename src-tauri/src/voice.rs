//! 语音输入：麦克风 → 16 kHz → Silero VAD 断句 → SenseVoice 识别 → 事件推给前端。
//!
//! - 说话过程中，每隔 `PARTIAL_EVERY` 把这句已经说了的部分重新识别一遍，作为临时字幕（`voice-partial`）；
//!   SenseVoice 不是流式模型，但几秒钟的音频识别只要几十毫秒，这样做代价很小。
//! - VAD 判断一句话说完（停顿 `MIN_SILENCE` 秒），就用这一整段识别出定稿（`voice-final`），前端拿去出板书。
//! - 音量每 100 ms 推一次（`voice-level`），界面上画一个电平条，让人知道麦克风在收音。
//! - 采集在 Rust 里用 cpal 做，不走浏览器的 getUserMedia：各平台 WebView 的麦克风权限处理不一致。
//! - 线上会议可以同时收系统声音（别人的声音从扬声器出来）：Linux 用默认输出的 monitor 源，
//!   Windows 用 WASAPI loopback，macOS 用 ScreenCaptureKit；两路转成 16 kHz 后混音再识别。
//!
//! - 每句定稿时顺带算一个声纹向量（CAM++，`voiceprint`），前端拿它和这场会已经出现过的声音比，判断是谁在说。
//!   比对和起名放在前端：声纹跟着会议存，中途关麦再开、过几天接着开都认得人。
//!
//! 识别部分（`Engine`）不依赖 Tauri 和声卡，可以直接喂 wav 测试。

use crate::apple_speech::{self, Provider, Selection};
use crate::llm;
use crate::models::voice_files;
use serde::Serialize;
use sherpa_onnx::{
    LinearResampler, OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig,
    SileroVadModelConfig, SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig,
    VadModelConfig, VoiceActivityDetector,
};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[path = "voice_auto.rs"]
pub(crate) mod auto;
#[path = "voice_policy.rs"]
mod policy;
#[path = "voice_preview.rs"]
pub(crate) mod preview;
use policy::{Quality, Retry, Review, SourceAudio};
static CAPTURE: AtomicU64 = AtomicU64::new(0);
fn capture_id() -> String {
    format!(
        "{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        CAPTURE.fetch_add(1, Ordering::Relaxed)
    )
}

pub const SAMPLE_RATE: i32 = 16_000;
/// Silero VAD 在 16 kHz 下一次吃 512 个样本
const WINDOW: usize = 512;
/// 停顿多久算一句话说完。越短出字越快，但一句话中间换气也可能被切开
const MIN_SILENCE: f32 = 0.45;
/// Silero 超过这个长度会更积极地找停顿；真正的内存上限由下面的 flush 保证。
const MAX_SPEECH: f32 = 10.0;
/// Soft sentence break: once a sentence is this long (seconds), a short dip in loudness is enough
/// to end it. People who talk continuously rarely pause for `MIN_SILENCE`, and waiting for
/// `MAX_SPEECH` left the board seconds behind.
const SOFT_MAX: f32 = 4.0;
/// How many quiet windows (32 ms each) make a dip. Right after `SOFT_MAX` it takes a breath
/// (about 0.32 s), so the small gaps between words don't cut a clause in half; the longer the
/// sentence runs, the shorter the dip needed, down to about 0.13 s just before `MAX_SPEECH`.
const SOFT_PAUSE: usize = 10;
const SOFT_PAUSE_MIN: usize = 4;
/// A window is quiet when its loudness is below this share of the sentence's median loudness
const QUIET_RATIO: f32 = 0.3;
/// 说话过程中多久刷新一次临时字幕
const PARTIAL_EVERY: Duration = Duration::from_millis(600);
/// 检测到开口时往前多带一点，免得吃掉第一个字. The VAD only reports speech after
/// `min_speech_duration` (0.25 s) plus its own delay, and a soft break sends this buffer rather than
/// the VAD's segment. SenseVoice also reads the first word better with some silence before it
/// (with 0.25–0.8 s the test clip's 「开饭」 came out as 「开放」), so take a full second.
const PREROLL: usize = SAMPLE_RATE as usize;
/// 短于这个长度（样本数）的句子不算声纹：太短的「嗯」「对」声纹不准，前端会归给上一个说话的人
const MIN_VOICEPRINT: usize = SAMPLE_RATE as usize * 4 / 5;

/// Level only the VAD's copy: quiet speech can be recognizable but below Silero's input range.
struct VadLevel {
    peak: f32,
    gain: f32,
}

impl Default for VadLevel {
    fn default() -> Self {
        Self {
            peak: 0.0,
            gain: 1.0,
        }
    }
}

impl VadLevel {
    fn apply(&mut self, samples: &mut [f32]) {
        self.peak = samples
            .iter()
            .fold(self.peak * 0.95, |peak, x| peak.max(x.abs()));
        let target = if self.peak >= 0.08 {
            1.0
        } else {
            (0.2 / self.peak.max(0.025)).clamp(1.0, 8.0)
        };
        // Fast reduction, gradual increase: don't boost the quiet onset of normal speech by 8x.
        let rise = if rms(samples) >= 0.0005 { 0.125 } else { 0.0 };
        self.gain = target.min(self.gain + rise);
        for sample in samples {
            *sample *= self.gain;
        }
    }
}

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatus {
    /// 语音模型下好了没有
    pub installed: bool,
    pub listening: bool,
    pub device: Option<String>,
    pub message: Option<String>,
    pub language: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heard {
    pub id: String,
    pub revision: u32,
    pub sources: Vec<String>,
    pub quality: Quality,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<Box<Review>>,
    pub text: String,
    /// 这句话在本次收音里的起止时间（秒）
    pub start: f32,
    pub end: f32,
    /// 这句话的声纹（单位长度）；没有声纹模型或者句子太短时为 None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voiceprint: Option<Vec<f32>>,
    /// Where the time went before this sentence reached the app
    pub timing: Timing,
}

/// Per-sentence timings, shown in the transcript and exported with diagnostics
#[derive(Clone, Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Timing {
    /// Audio that had to arrive after the sentence ended before it was cut (the pause we waited for)
    pub wait_ms: u32,
    /// Recognizing the sentence
    pub asr_ms: u32,
    /// Computing its voiceprint
    pub voiceprint_ms: u32,
    /// Ended by a short dip in a long sentence, not by a real pause
    pub soft: bool,
}

/// Loudness of one window
fn rms(samples: &[f32]) -> f32 {
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32).sqrt()
}

/// Where to softly end a long sentence. `rms` is the loudness of each window of the current
/// sentence, oldest first. Returns how many windows from the end to cut at (the middle of the
/// dip that just happened), or None while the sentence is still short or there is no dip.
pub fn soft_break(rms: &[f32]) -> Option<usize> {
    let n = rms.len();
    if n * WINDOW < (SOFT_MAX * SAMPLE_RATE as f32) as usize {
        return None;
    }
    let mut sorted = rms.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    // Cap the quiet-room floor below speech itself; an absolute floor cuts quiet words in half.
    let median = sorted[n / 2];
    let quiet = (median * QUIET_RATIO).max(0.003_f32.min(median * 0.5));
    let run = rms.iter().rev().take_while(|&&r| r < quiet).count();
    // One window less per second past SOFT_MAX
    let over = (n * WINDOW) as f32 / SAMPLE_RATE as f32 - SOFT_MAX;
    let need = (SOFT_PAUSE as f32 - over)
        .round()
        .clamp(SOFT_PAUSE_MIN as f32, SOFT_PAUSE as f32) as usize;
    // After a cut the caller keeps only the tail, so this doesn't fire again on the same dip
    (run >= need).then_some(run / 2)
}

/// 识别核心：喂 16 kHz 的单声道样本，吐出临时结果和定稿
pub struct Engine {
    vad: VoiceActivityDetector,
    asr: OfflineRecognizer,
    /// 声纹模型；没带就不区分说话人
    speaker: Option<SpeakerEmbeddingExtractor>,
    pending: Vec<f32>,
    /// 当前这句话（从检测到开口开始，带一点前置）
    speech: Vec<f32>,
    /// Loudness of each window in `speech`
    loudness: Vec<f32>,
    /// Everything before this sample (counted like `fed`) was already finalized
    sent_until: usize,
    recent: Vec<f32>,
    in_speech: bool,
    vad_start: usize,
    last_partial: Instant,
    partial_text: String,
    /// 已经喂进来的样本总数，用来算时间
    fed: usize,
    level: VadLevel,
    /// Original samples for VAD boundaries; recognition and voiceprints must not use VAD gain.
    audio: std::collections::VecDeque<f32>,
    dynamic: bool,
    capture: String,
    sequence: u64,
    sources: SourceAudio,
    retries: std::collections::VecDeque<Retry>,
    #[cfg(test)]
    vad_segments: usize,
    #[cfg(test)]
    vad_samples: usize,
}

pub enum Event {
    Partial(String),
    Final(Heard),
}

impl Engine {
    pub fn load(models: &Path) -> Result<Self, String> {
        let path = |p: &str| models.join(p).to_string_lossy().to_string();
        let vad_config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(path(voice_files::VAD)),
                threshold: 0.5,
                min_silence_duration: MIN_SILENCE,
                min_speech_duration: 0.25,
                window_size: WINDOW as i32,
                max_speech_duration: MAX_SPEECH,
            },
            sample_rate: SAMPLE_RATE,
            num_threads: 1,
            ..Default::default()
        };
        let vad = VoiceActivityDetector::create(&vad_config, 60.0)
            .ok_or("Couldn't load the speech detection model")?;

        let mut asr_config = OfflineRecognizerConfig::default();
        asr_config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(path(voice_files::SENSE_VOICE)),
            language: Some("auto".into()),
            // 数字、日期规整成阿拉伯数字，带标点
            use_itn: true,
        };
        asr_config.model_config.tokens = Some(path(voice_files::TOKENS));
        asr_config.model_config.num_threads = 2;
        let asr = OfflineRecognizer::create(&asr_config)
            .ok_or("Couldn't load the speech recognition model")?;

        let speaker_model = models.join(voice_files::SPEAKER);
        let speaker = if speaker_model.exists() {
            let s = SpeakerEmbeddingExtractor::create(&SpeakerEmbeddingExtractorConfig {
                model: Some(speaker_model.to_string_lossy().to_string()),
                num_threads: 1,
                ..Default::default()
            });
            if s.is_none() {
                eprintln!("声纹模型加载失败，不区分说话人");
            }
            s
        } else {
            None
        };

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
            vad_start: 0,
            last_partial: Instant::now(),
            partial_text: String::new(),
            fed: 0,
            level: VadLevel::default(),
            audio: Default::default(),
            dynamic: true,
            capture: capture_id(),
            sequence: 0,
            sources: SourceAudio::default(),
            retries: Default::default(),
            #[cfg(test)]
            vad_segments: 0,
            #[cfg(test)]
            vad_samples: 0,
        })
    }

    fn recognize(&self, samples: &[f32]) -> String {
        let stream = self.asr.create_stream();
        stream.accept_waveform(SAMPLE_RATE, samples);
        self.asr.decode(&stream);
        stream
            .get_result()
            .map(|r| r.text.trim().to_string())
            .unwrap_or_default()
    }

    /// 一句话的声纹，归一化成单位长度（前端直接点积就是余弦相似度）
    pub fn voiceprint(&self, samples: &[f32]) -> Option<Vec<f32>> {
        let ex = self.speaker.as_ref()?;
        if samples.len() < MIN_VOICEPRINT {
            return None;
        }
        let stream = ex.create_stream()?;
        stream.accept_waveform(SAMPLE_RATE, samples);
        stream.input_finished();
        if !ex.is_ready(&stream) {
            return None;
        }
        let mut v = ex.compute(&stream)?;
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if !norm.is_finite() || norm == 0.0 {
            return None;
        }
        v.iter_mut().for_each(|x| *x /= norm);
        Some(v)
    }

    /// Keep each aligned source before mixing; the ordinary VAD/ASR path stays identical.
    fn feed_tracks(&mut self, tracks: &[Vec<f32>], out: &mut Vec<Event>) -> Vec<f32> {
        self.sources.push(tracks);
        let mixed = mix(tracks);
        self.feed(&mixed, out);
        mixed
    }

    /// 喂一段 16 kHz 单声道样本
    pub fn feed(&mut self, samples: &[f32], out: &mut Vec<Event>) {
        self.pending.extend_from_slice(samples);
        while self.pending.len() >= WINDOW {
            let chunk: Vec<f32> = self.pending.drain(..WINDOW).collect();
            self.fed += WINDOW;
            self.audio.extend(&chunk);
            let mut vad_chunk = chunk.clone();
            self.level.apply(&mut vad_chunk);
            self.vad.accept_waveform(&vad_chunk);

            if self.in_speech {
                self.speech.extend_from_slice(&chunk);
                self.loudness.push(rms(&chunk));
                if let Some(back) = soft_break(&self.loudness) {
                    self.soft_cut(back, out);
                }
            } else {
                self.recent.extend_from_slice(&chunk);
                let n = self.recent.len();
                if n > PREROLL {
                    self.recent.drain(..n - PREROLL);
                }
            }
            if self.vad.detected() && !self.in_speech {
                self.in_speech = true;
                self.speech = std::mem::take(&mut self.recent);
                self.vad_start = self.fed - self.speech.len();
                self.loudness = self.speech.chunks(WINDOW).map(rms).collect();
                self.last_partial = Instant::now();
            }
            // Soft cuts do not release Silero's own continuous-speech buffer; flush without resetting its clock.
            if self.in_speech && self.fed - self.vad_start >= policy::RETAIN - PREROLL {
                self.vad.flush();
            }
            self.drain_segments(out);
            let before = if self.in_speech {
                self.sent_until
            } else {
                self.fed.saturating_sub(PREROLL)
            };
            let discard = before
                .saturating_sub(self.fed - self.audio.len())
                .min(self.audio.len());
            self.audio.drain(..discard);
        }
        // 正在说：定时刷新临时字幕
        if self.in_speech
            && self.last_partial.elapsed() >= PARTIAL_EVERY
            && self.speech.len() > SAMPLE_RATE as usize / 2
        {
            self.last_partial = Instant::now();
            let text = self.recognize(&self.speech);
            if !text.is_empty() && text != self.partial_text {
                self.partial_text = text.clone();
                out.push(Event::Partial(text));
            }
        }
    }

    /// 停止收音时调用：没说完的那句也定稿
    pub fn finish(&mut self, out: &mut Vec<Event>) {
        if !self.pending.is_empty() {
            self.feed(&vec![0.0; WINDOW - self.pending.len()], out);
        }
        self.vad.flush();
        self.drain_segments(out);
    }

    /// 只清掉上次收音的状态，模型留着给下一次用。
    fn reset(&mut self) {
        self.vad.reset();
        self.pending.clear();
        self.speech.clear();
        self.loudness.clear();
        self.sent_until = 0;
        self.recent.clear();
        self.in_speech = false;
        self.vad_start = 0;
        self.last_partial = Instant::now();
        self.partial_text.clear();
        self.fed = 0;
        self.level = VadLevel::default();
        self.audio.clear();
        self.capture = capture_id();
        self.sequence = 0;
        self.sources = SourceAudio::default();
        self.retries.clear();
        #[cfg(test)]
        {
            self.vad_segments = 0;
            self.vad_samples = 0;
        }
    }

    /// Recognize one finished sentence starting at sample `start` (counted like `fed`) and send it
    fn finalize(&mut self, samples: &[f32], start: usize, soft: bool, out: &mut Vec<Event>) {
        if samples.len() < WINDOW {
            return;
        }
        let t = Instant::now();
        let text = self.recognize(samples);
        let asr_ms = t.elapsed().as_millis() as u32;
        let t = Instant::now();
        let voiceprint = self.voiceprint(samples);
        let voiceprint_ms = t.elapsed().as_millis() as u32;
        let end = start + samples.len();
        let tracks = self.sources.clip(start, samples.len());
        let quality = Quality::measure(samples, &tracks);
        self.sequence += 1;
        let mut heard = Heard {
            id: format!("{}:{}", self.capture, self.sequence),
            revision: 0,
            sources: self.sources.names.clone(),
            quality,
            review: None,
            text,
            start: start as f32 / SAMPLE_RATE as f32,
            end: end as f32 / SAMPLE_RATE as f32,
            voiceprint,
            timing: Timing {
                wait_ms: (self.fed.saturating_sub(end) * 1000 / SAMPLE_RATE as usize) as u32,
                asr_ms,
                voiceprint_ms,
                soft,
            },
        };
        if self.dynamic {
            if let Some((reason, action, retry)) =
                policy::plan(samples, &tracks, &heard.quality, &heard.text)
            {
                let decision = if samples.len() > policy::RETRY_MAX {
                    "too_long"
                } else if self.retries.len() >= policy::RETRY_QUEUE {
                    "queue_full"
                } else {
                    "pending"
                };
                heard.review = Some(Box::new(Review {
                    reason,
                    action,
                    original: heard.text.clone(),
                    candidate: String::new(),
                    decision: decision.into(),
                    asr_ms: 0,
                }));
                if decision == "pending" {
                    self.retries.push_back(Retry {
                        heard: heard.clone(),
                        samples: retry,
                        queued_at: self.fed,
                    });
                }
            }
        }
        // Empty initial results remain in the eval trace, but never become blank transcript rows.
        out.push(Event::Final(heard));
    }

    /// Run only after publishing primary results. Capture/stop reuse this worker, no extra model.
    fn review_one(&mut self, out: &mut Vec<Event>) {
        let Some(mut retry) = self.retries.pop_front() else {
            return;
        };
        let review = retry.heard.review.as_mut().unwrap();
        if self.fed.saturating_sub(retry.queued_at) > policy::RETAIN {
            review.decision = "expired".into();
        } else {
            let t = Instant::now();
            review.candidate = self.recognize(&retry.samples);
            review.asr_ms = t.elapsed().as_millis() as u32;
            if policy::accept(&review.original, &review.candidate) {
                retry.heard.text = review.candidate.clone();
                review.decision = "accepted".into();
            } else {
                review.decision = if review.original == review.candidate {
                    "unchanged"
                } else {
                    "retained"
                }
                .into();
            }
        }
        // An empty primary was never shown: its recovery is the first visible version.
        retry.heard.revision = u32::from(!review.original.is_empty());
        out.push(Event::Final(retry.heard));
    }

    /// End the sentence `back` windows before now, in the middle of a short dip, and keep the rest
    /// as the start of the next one. The VAD still sees one long segment; `sent_until` makes
    /// `drain_segments` skip the part already sent.
    fn soft_cut(&mut self, back: usize, out: &mut Vec<Event>) {
        let keep = (back * WINDOW).min(self.speech.len());
        let head: Vec<f32> = self.speech.drain(..self.speech.len() - keep).collect();
        let n = self.loudness.len();
        self.loudness.drain(..n - back.min(n));
        let start = self.fed - keep - head.len();
        self.sent_until = self.fed - keep;
        self.finalize(&head, start, true, out);
        self.partial_text.clear();
        self.last_partial = Instant::now();
    }

    fn drain_segments(&mut self, out: &mut Vec<Event>) {
        while let Some(seg) = self.vad.front() {
            #[cfg(test)]
            {
                self.vad_segments += 1;
                self.vad_samples += seg.samples().len();
            }
            let seg_start = seg.start().max(0) as usize;
            let skip = self
                .sent_until
                .saturating_sub(seg_start)
                .min(seg.samples().len());
            let start = seg_start + skip;
            debug_assert!(skip == seg.samples().len() || start >= self.fed - self.audio.len());
            let offset = start.saturating_sub(self.fed - self.audio.len());
            let samples: Vec<f32> = self
                .audio
                .iter()
                .skip(offset)
                .take(seg.samples().len() - skip)
                .copied()
                .collect();
            debug_assert_eq!(samples.len(), seg.samples().len() - skip);
            self.finalize(&samples, start, false, out);
            self.sent_until = seg_start + seg.samples().len();
            self.vad.pop();
            self.in_speech = self.vad.detected();
            self.speech.clear();
            self.loudness.clear();
            self.partial_text.clear();
        }
        if !self.vad.detected() && self.in_speech && self.vad.is_empty() {
            // VAD 已经不认为在说话，但还没给出段落（太短被丢弃了），收尾
            self.in_speech = false;
            self.speech.clear();
            self.loudness.clear();
            self.partial_text.clear();
        }
    }
}

// —— 混音：麦克风和系统声音混成一路 ——

/// 某一路落后这么多（样本数）还没来数据，就当它这段是静音，不再等它
const MAX_LAG: usize = SAMPLE_RATE as usize * 3 / 10;

/// 把几路 16 kHz 单声道的声音对齐了加起来。
/// Windows 的回采（loopback）在没有声音播放时根本不给数据，所以不能死等每一路都到齐：
/// 有一路积压超过 `MAX_LAG`，就把落后的那几路补零，照常往下走。
pub struct Mixer {
    queues: Vec<std::collections::VecDeque<f32>>,
}

impl Mixer {
    pub fn new(sources: usize) -> Self {
        Self {
            queues: (0..sources.max(1)).map(|_| Default::default()).collect(),
        }
    }

    pub fn push(&mut self, src: usize, samples: &[f32]) {
        if let Some(q) = self.queues.get_mut(src) {
            q.extend(samples);
        }
    }

    /// 取出已经能混的部分
    #[cfg(test)]
    pub fn pop(&mut self) -> Vec<f32> {
        mix(&self.pop_tracks())
    }

    fn pop_tracks(&mut self) -> Vec<Vec<f32>> {
        let longest = self.queues.iter().map(|q| q.len()).max().unwrap_or(0);
        let shortest = self.queues.iter().map(|q| q.len()).min().unwrap_or(0);
        let n = if longest >= MAX_LAG {
            longest
        } else {
            shortest
        };
        self.queues
            .iter_mut()
            .map(|q| {
                let mut samples: Vec<f32> = q.drain(..n.min(q.len())).collect();
                samples.resize(n, 0.0);
                samples
            })
            .collect()
    }
}

fn mix(tracks: &[Vec<f32>]) -> Vec<f32> {
    let mut out = vec![0.0_f32; tracks.first().map_or(0, Vec::len)];
    for track in tracks {
        for (o, s) in out.iter_mut().zip(track) {
            *o += s;
        }
    }
    out.iter_mut().for_each(|s| *s = s.clamp(-1.0, 1.0));
    out
}

// —— 采集和生命周期 ——

struct Running {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

#[derive(Default)]
pub struct Voice {
    running: Mutex<Option<Running>>,
    preview: Mutex<preview::Preview>,
    status: Mutex<VoiceStatus>,
    engine: Mutex<Option<Engine>>,
    apple: Mutex<Option<apple_speech::Session>>,
    warmup: Mutex<Option<JoinHandle<()>>>,
    shutting_down: AtomicBool,
    selection: Mutex<Selection>,
}

pub fn init(app: &AppHandle) {
    *app.state::<Voice>().selection.lock().unwrap() = Selection::load(app);
    preload(app);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechModels {
    selection: Selection,
    sense_voice_installed: bool,
    listening: bool,
    preparing: bool,
    apple: Option<apple_speech::Availability>,
    apple_error: Option<String>,
    auto_locales: Vec<String>,
    active_locale: Option<String>,
}

#[tauri::command(async)]
pub fn voice_models(app: AppHandle) -> SpeechModels {
    let apple = apple_speech::status(&app);
    let voice = app.state::<Voice>();
    let selection = voice.selection.lock().unwrap().clone();
    let listening = voice
        .running
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|r| !r.stop.load(Ordering::SeqCst));
    let active_locale = voice.status.lock().unwrap().language.clone();
    SpeechModels {
        auto_locales: apple.as_ref().map(|a| a.auto_locales()).unwrap_or_default(),
        active_locale,
        selection,
        sense_voice_installed: base(&app).is_some(),
        listening,
        preparing: app
            .state::<apple_speech::Jobs>()
            .preparing
            .load(Ordering::SeqCst),
        apple: apple.as_ref().ok().cloned(),
        apple_error: apple.err(),
    }
}

#[tauri::command(async)]
pub fn voice_prepare(app: AppHandle, locale: String) -> Result<(), String> {
    let result = apple_speech::prepare(&app, &locale);
    let _ = app.emit("voice-models-changed", ());
    result
}

#[tauri::command(async)]
pub fn voice_select(app: AppHandle, provider: Provider, locale: String) -> Result<(), String> {
    let voice = app.state::<Voice>();
    let mut running = voice.running.lock().unwrap();
    preview::stop_all(&app);
    if voice.shutting_down.load(Ordering::SeqCst) {
        return Err("Audio input is shutting down".into());
    }
    if running
        .as_ref()
        .is_some_and(|r| !r.stop.load(Ordering::SeqCst))
    {
        return Err("Stop recording before changing the speech model".into());
    }
    if provider == Provider::Apple {
        if locale == "auto" && base(&app).is_none() {
            return Err("Auto requires the bundled SenseVoice model".into());
        }
        if !apple_speech::status(&app)?.ready(&locale) {
            return Err("Choose a supported Apple language and download its model first".into());
        }
    } else if base(&app).is_none() {
        return Err("Download the SenseVoice speech model first".into());
    }
    let selection = Selection {
        provider,
        locale: if provider == Provider::Apple {
            locale
        } else {
            String::new()
        },
    };
    selection.save(&app)?;
    stop(&app, &mut running);
    if let Some(thread) = voice.warmup.lock().unwrap().take() {
        let _ = thread.join();
    }
    *voice.engine.lock().unwrap() = None;
    *voice.apple.lock().unwrap() = None;
    *voice.selection.lock().unwrap() = selection;
    drop(running);
    preload(&app);
    let _ = app.emit("voice-models-changed", ());
    Ok(())
}

/// 启动后静默加载模型；这里只碰模型文件，不打开音频设备。
pub fn preload(app: &AppHandle) {
    let voice = app.state::<Voice>();
    let mut warmup = voice.warmup.lock().unwrap();
    if warmup.is_some() {
        return;
    }
    let app = app.clone();
    *warmup = Some(thread::spawn(move || {
        let voice = app.state::<Voice>();
        let selection = voice.selection.lock().unwrap().clone();
        if selection.provider == Provider::Apple {
            let mut cached = voice.apple.lock().unwrap();
            if cached.is_none() {
                match apple_speech::Session::start(&app, &selection.locale, capture_id()) {
                    Ok(session) => *cached = Some(session),
                    Err(error) => eprintln!("Apple speech preload failed: {error}"),
                }
            }
            return;
        }
        let mut cached = voice.engine.lock().unwrap();
        if cached.is_some() {
            return;
        }
        let Some(models) = base(&app) else {
            return;
        };
        let t = Instant::now();
        match Engine::load(&models) {
            Ok(engine) => {
                *cached = Some(engine);
                eprintln!("语音模型已预加载（{} ms）", t.elapsed().as_millis());
            }
            Err(msg) => eprintln!("语音模型预加载失败：{msg}"),
        }
    }));
}

/// 语音模型所在的目录：优先用安装包自带的（资源目录下的 voice/），没有再用下载的
pub fn base(app: &AppHandle) -> Option<std::path::PathBuf> {
    if let Some(d) = bundled(app) {
        return Some(d);
    }
    llm::models_dir(app).filter(|d| crate::models::installed("voice", d))
}

/// 安装包里带着语音模型时，返回资源目录
pub fn bundled(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .resource_dir()
        .ok()
        .filter(|d| crate::models::installed("voice", d))
}

fn installed(app: &AppHandle) -> bool {
    match app.state::<Voice>().selection.lock().unwrap().provider {
        Provider::Sensevoice => base(app).is_some(),
        // The saved selection was validated; the native helper rechecks system assets at start.
        Provider::Apple => cfg!(target_os = "macos"),
    }
}

fn set_status(app: &AppHandle, s: VoiceStatus) {
    *app.state::<Voice>().status.lock().unwrap() = s.clone();
    let _ = app.emit("voice-status", s);
    // 停机时主线程会等语音线程，语音线程不能同步等主线程更新托盘。
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let listening = handle.state::<Voice>().status.lock().unwrap().listening;
        crate::shell::set_listening(&handle, listening);
    });
}

pub fn detected_language(app: &AppHandle, locale: &str) {
    let mut status = app.state::<Voice>().status.lock().unwrap().clone();
    status.language = Some(locale.to_owned());
    set_status(app, status);
}

#[tauri::command]
pub fn voice_status(app: AppHandle) -> VoiceStatus {
    let mut s = app.state::<Voice>().status.lock().unwrap().clone();
    s.installed = installed(&app);
    s
}

/// Linux 上优先用 PulseAudio（PipeWire 自带兼容层）：ALSA 直接列出来的是 null、jack、oss、
/// 重采样之类的插件，看不出哪个是麦克风
fn audio_host() -> cpal::Host {
    #[cfg(target_os = "linux")]
    {
        if cpal::available_hosts().contains(&cpal::HostId::PulseAudio) {
            if let Ok(h) = cpal::host_from_id(cpal::HostId::PulseAudio) {
                return h;
            }
        }
    }
    cpal::default_host()
}

/// 能拿来收音的输入设备：去掉扬声器的回采（PulseAudio 的 `.monitor`）和 ALSA 的插件设备
fn usable(d: &cpal::Device) -> Option<String> {
    use cpal::traits::DeviceTrait;
    let id = d.id().map(|i| i.to_string()).unwrap_or_default();
    let name = d.description().ok()?.name().to_string();
    let plugin = id.starts_with("alsa")
        && !["default", "pipewire", "pulse"]
            .iter()
            .any(|k| id.ends_with(k))
        && !id.contains("hw:")
        && !id.contains("plughw:")
        && !id.contains("sysdefault");
    (!id.ends_with(".monitor") && !name.starts_with("Monitor of") && !plugin).then_some(name)
}

#[tauri::command]
pub fn voice_devices() -> Vec<String> {
    use cpal::traits::HostTrait;
    let mut names: Vec<String> = audio_host()
        .input_devices()
        .map(|ds| ds.filter_map(|d| usable(&d)).collect())
        .unwrap_or_default();
    names.dedup();
    names
}

fn microphone(host: &cpal::Host, device: Option<&str>) -> Result<cpal::Device, String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    if let Some(name) = device.filter(|name| !name.is_empty()) {
        host.input_devices()
            .map_err(|error| error.to_string())?
            .find(|device| device.description().is_ok_and(|d| d.name() == name))
            .ok_or_else(|| "The selected microphone is no longer available".into())
    } else {
        host.default_input_device()
            .ok_or_else(|| "No microphone found".into())
    }
}

#[tauri::command(async)]
pub fn voice_start(
    app: AppHandle,
    device: Option<String>,
    source: Option<String>,
) -> Result<(), String> {
    let source = Source::parse(source.as_deref());
    let voice = app.state::<Voice>();
    let mut running = voice.running.lock().unwrap();
    preview::stop_all(&app);
    if !installed(&app) {
        return Err("Download the speech recognition model first".into());
    }
    if voice.shutting_down.load(Ordering::SeqCst) {
        return Err("Audio input is shutting down".into());
    }
    if running
        .as_ref()
        .is_some_and(|r| r.stop.load(Ordering::SeqCst))
    {
        stop(&app, &mut running);
    }
    if running.is_some() {
        return Ok(());
    }
    let selection = voice.selection.lock().unwrap().clone();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let handle = app.clone();
    // cpal 的流一直留在这个线程里：有些平台上 Stream 不能跨线程。
    let (ready_tx, ready_rx) = mpsc::channel::<Result<String, String>>();
    let thread = thread::spawn(move || run(handle, selection, device, source, flag, ready_tx));
    match ready_rx.recv_timeout(Duration::from_secs(30)) {
        Ok(Ok(_)) => {
            *running = Some(Running { stop, thread });
            Ok(())
        }
        Ok(Err(msg)) => {
            let _ = thread.join();
            set_status(
                &app,
                VoiceStatus {
                    installed: true,
                    message: Some(msg.clone()),
                    ..Default::default()
                },
            );
            Err(msg)
        }
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            // 保留句柄，退出或重试时仍能等它结束，不能遗留后台加载线程。
            *running = Some(Running { stop, thread });
            let msg = "Timed out starting audio input".to_string();
            set_status(
                &app,
                VoiceStatus {
                    installed: true,
                    message: Some(msg.clone()),
                    ..Default::default()
                },
            );
            Err(msg)
        }
    }
}

#[tauri::command(async)]
pub fn voice_stop(app: AppHandle) {
    let voice = app.state::<Voice>();
    let mut running = voice.running.lock().unwrap();
    stop(&app, &mut running);
}

fn stop(app: &AppHandle, running: &mut Option<Running>) {
    preview::stop_all(app);
    if let Some(r) = running.take() {
        r.stop.store(true, Ordering::SeqCst);
        let _ = r.thread.join();
    }
    set_status(
        app,
        VoiceStatus {
            installed: installed(app),
            ..Default::default()
        },
    );
}

/// 模型文件变了（下完或删了）：停掉正在进行的收音，让界面重新读状态
pub fn reload(app: &AppHandle) {
    unload(app);
    preload(app);
    let _ = app.emit("voice-models-changed", ());
}

pub fn unload(app: &AppHandle) {
    let voice = app.state::<Voice>();
    let mut running = voice.running.lock().unwrap();
    stop(app, &mut running);
    if let Some(thread) = voice.warmup.lock().unwrap().take() {
        let _ = thread.join();
    }
    *voice.engine.lock().unwrap() = None;
    *voice.apple.lock().unwrap() = None;
}

pub fn shutdown(app: &AppHandle) {
    app.state::<Voice>()
        .shutting_down
        .store(true, Ordering::SeqCst);
    app.state::<apple_speech::Jobs>().shutdown();
    unload(app);
}

/// 收哪些声音
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Source {
    Mic,
    /// 麦克风 + 系统声音（线上会议：自己的声音从麦克风来，别人的从扬声器来）
    Both,
    /// 只收系统声音
    System,
}

impl Source {
    pub fn parse(s: Option<&str>) -> Self {
        match s {
            Some("both") => Self::Both,
            Some("system") => Self::System,
            _ => Self::Mic,
        }
    }

    fn names(self) -> &'static [&'static str] {
        match self {
            Self::Mic => &["mic"],
            Self::System => &["system"],
            Self::Both => &["mic", "system"],
        }
    }
}

/// 系统声音从哪来：Linux 上是默认输出设备的回采（PulseAudio / PipeWire 的 monitor 源，本身就是输入设备）；
/// Windows 上直接把默认输出设备当输入打开（WASAPI loopback）。返回的 bool 表示是不是 loopback
#[cfg(target_os = "linux")]
fn system_source(host: &cpal::Host) -> Result<(cpal::Device, bool), String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let out = host
        .default_output_device()
        .and_then(|d| d.description().ok().map(|x| x.name().to_string()));
    let monitors: Vec<cpal::Device> = host
        .input_devices()
        .map(|ds| {
            ds.filter(|d| {
                let id = d.id().map(|i| i.to_string()).unwrap_or_default();
                let name = d
                    .description()
                    .map(|x| x.name().to_string())
                    .unwrap_or_default();
                id.ends_with(".monitor") || name.starts_with("Monitor of")
            })
            .collect()
        })
        .unwrap_or_default();
    // 优先用正在出声的那个输出设备的回采
    let wanted = out.map(|o| format!("Monitor of {o}"));
    let pick = monitors
        .iter()
        .position(|d| {
            wanted
                .as_deref()
                .is_some_and(|w| d.description().is_ok_and(|x| x.name() == w))
        })
        .unwrap_or(0);
    monitors
        .into_iter()
        .nth(pick)
        .map(|d| (d, false))
        .ok_or_else(|| "No system audio source found (PulseAudio / PipeWire monitor source)".into())
}

#[cfg(windows)]
fn system_source(host: &cpal::Host) -> Result<(cpal::Device, bool), String> {
    use cpal::traits::HostTrait;
    host.default_output_device()
        .map(|d| (d, true))
        .ok_or_else(|| "No speakers found; can't capture system audio".into())
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
fn system_source(_host: &cpal::Host) -> Result<(cpal::Device, bool), String> {
    Err("System audio capture is not supported on this platform".into())
}

/// 打开一路声音：声卡回调里只把这一段转成单声道 f32，连同来源编号丢进通道，重活在外面的循环里做。
/// `loopback`：设备是输出设备（Windows 回采），按它的输出格式打开
fn open_stream(
    dev: &cpal::Device,
    loopback: bool,
    src: usize,
    tx: mpsc::Sender<(usize, Vec<f32>)>,
    on_err: impl FnMut(String) + Send + Clone + 'static,
) -> Result<(cpal::Stream, i32), String> {
    use cpal::traits::DeviceTrait;
    let what = if src == 0 {
        "Microphone"
    } else {
        "System audio"
    };
    let supported = if loopback {
        dev.default_output_config()
    } else {
        dev.default_input_config()
    }
    .map_err(|e| format!("{what} could not be opened: {e}"))?;
    let channels = supported.channels() as usize;
    let rate = supported.sample_rate() as i32;
    let mut config = supported.config();
    // PulseAudio 的默认缓冲可能长达两秒；设备允许时按 20 ms 收一块。
    if let cpal::SupportedBufferSize::Range { min, max } = *supported.buffer_size() {
        config.buffer_size = cpal::BufferSize::Fixed((rate as u32 / 50).clamp(min, max));
    }
    macro_rules! stream_of {
        ($t:ty, $to_f32:expr) => {{
            let tx = tx.clone();
            let mut on_err = on_err.clone();
            dev.build_input_stream::<$t, _, _>(
                config.clone(),
                move |data: &[$t], _| {
                    let mono: Vec<f32> = data
                        .chunks(channels)
                        .map(|frame| {
                            frame.iter().map(|&s| $to_f32(s)).sum::<f32>() / channels as f32
                        })
                        .collect();
                    let _ = tx.send((src, mono));
                },
                move |err: cpal::Error| on_err(format!("{what} error: {err}")),
                None,
            )
        }};
    }
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => stream_of!(f32, |s: f32| s),
        cpal::SampleFormat::I16 => stream_of!(i16, |s: i16| s as f32 / 32768.0),
        cpal::SampleFormat::U16 => stream_of!(u16, |s: u16| (s as f32 - 32768.0) / 32768.0),
        cpal::SampleFormat::I32 => stream_of!(i32, |s: i32| s as f32 / 2147483648.0),
        other => return Err(format!("{what} sample format {other:?} is not supported")),
    }
    .map_err(|e| format!("{what} could not be opened: {e}"))?;
    Ok((stream, rate))
}

fn run(
    app: AppHandle,
    selection: Selection,
    device: Option<String>,
    source: Source,
    stop: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<String, String>>,
) {
    use cpal::traits::{DeviceTrait, StreamTrait};

    let voice = app.state::<Voice>();
    let mut cached = voice.engine.lock().unwrap();
    if selection.provider == Provider::Sensevoice && cached.is_none() {
        match base(&app)
            .ok_or_else(|| "Speech recognition model not found".to_string())
            .and_then(|models| Engine::load(&models))
        {
            Ok(engine) => *cached = Some(engine),
            Err(msg) => {
                let _ = ready.send(Err(msg));
                return;
            }
        }
    }
    if stop.load(Ordering::SeqCst) {
        return;
    }
    let mut apple = if selection.provider == Provider::Apple {
        let cached = voice.apple.lock().unwrap().take();
        match cached
            .map(Ok)
            .unwrap_or_else(|| apple_speech::Session::start(&app, &selection.locale, capture_id()))
        {
            Ok(session) => Some(session),
            Err(msg) => {
                let _ = ready.send(Err(msg));
                return;
            }
        }
    } else {
        None
    };
    if stop.load(Ordering::SeqCst) {
        return;
    }
    let mut engine = if apple.is_none() {
        cached.as_mut()
    } else {
        None
    };

    let host = audio_host();
    let (tx, rx) = mpsc::channel::<(usize, Vec<f32>)>();
    let err_app = app.clone();
    let on_err = move |msg: String| {
        let _ = err_app.emit("voice-error", msg);
    };
    // 每一路：流（得一直拿着，丢了就停了）、要不要重采样到 16 kHz
    let mut streams = vec![];
    let mut resamplers: Vec<Option<LinearResampler>> = vec![None, None];
    let mut names = vec![];
    #[cfg(target_os = "macos")]
    let mut system_audio = None;

    if source != Source::System {
        let dev = match microphone(&host, device.as_deref()) {
            Ok(device) => device,
            Err(error) => {
                let _ = ready.send(Err(error));
                return;
            }
        };
        match open_stream(&dev, false, 0, tx.clone(), on_err.clone()) {
            Ok((stream, rate)) => {
                streams.push(stream);
                resamplers[0] = (rate != SAMPLE_RATE)
                    .then(|| LinearResampler::create(rate, SAMPLE_RATE))
                    .flatten();
                names.push(
                    dev.description()
                        .map(|d| d.name().to_string())
                        .unwrap_or_else(|_| "Microphone".into()),
                );
            }
            Err(msg) => {
                let _ = ready.send(Err(msg));
                return;
            }
        }
    }
    // Activate the microphone before ScreenCaptureKit so device setup precedes system capture.
    for s in &streams {
        if let Err(err) = s.play() {
            let _ = ready.send(Err(format!("Couldn't start microphone: {err}")));
            return;
        }
    }
    if source != Source::Mic {
        #[cfg(target_os = "macos")]
        match apple_speech::SystemAudio::start(&app, tx.clone()) {
            Ok(capture) => {
                system_audio = Some(capture);
                names.push("System audio".into());
            }
            Err(msg) => {
                let _ = ready.send(Err(msg));
                return;
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let opened = system_source(&host).and_then(|(dev, loopback)| {
                open_stream(&dev, loopback, 1, tx.clone(), on_err.clone())
            });
            match opened {
                Ok((stream, rate)) => {
                    if let Err(err) = stream.play() {
                        let _ = ready.send(Err(format!("Couldn't start system audio: {err}")));
                        return;
                    }
                    streams.push(stream);
                    resamplers[1] = (rate != SAMPLE_RATE)
                        .then(|| LinearResampler::create(rate, SAMPLE_RATE))
                        .flatten();
                    names.push("System audio".into());
                }
                Err(msg) => {
                    let _ = ready.send(Err(msg));
                    return;
                }
            }
        }
    }
    drop(tx);
    let name = names.join(" + ");
    set_status(
        &app,
        VoiceStatus {
            installed: true,
            listening: true,
            device: Some(name.clone()),
            message: None,
            language: (selection.provider == Provider::Apple && selection.locale != "auto")
                .then_some(selection.locale.clone()),
        },
    );
    let _ = ready.send(Ok(name));

    // 只开了系统声音时，它是第 1 路；混音器按实际开了几路来
    let sources = source.names();
    let mut mixer = Mixer::new(sources.len());
    if let Some(engine) = engine.as_mut() {
        engine.sources.names = sources.iter().map(|s| (*s).into()).collect();
    }
    let slot = |src: usize| if source == Source::System { 0 } else { src };
    let mut events = vec![];
    let mut level_sum = 0.0f32;
    let mut level_n = 0usize;
    let mut last_level = Instant::now();
    let mut failure = None;
    loop {
        let stopping = stop.load(Ordering::SeqCst);
        if stopping {
            if apple.is_none() || voice.shutting_down.load(Ordering::SeqCst) {
                break;
            }
            // Auto may start Apple's analyzer while capture queues audio; retain that tail on stop.
            streams.clear();
            #[cfg(target_os = "macos")]
            drop(system_audio.take());
        }
        #[cfg(target_os = "macos")]
        if let Some(capture) = &system_audio {
            if let Err(error) = capture.poll() {
                failure = Some(error);
                break;
            }
        }
        if let Some(apple) = apple.as_mut() {
            if let Err(error) = apple.poll(&app, sources) {
                failure = Some(error);
                break;
            }
        }
        let received = if stopping {
            rx.try_recv().ok()
        } else {
            rx.recv_timeout(Duration::from_millis(100)).ok()
        };
        let Some((src, chunk)) = received else {
            if stopping {
                break;
            }
            if let Some(engine) = engine.as_mut() {
                engine.review_one(&mut events);
            }
            emit_events(&app, &mut events);
            continue;
        };
        let samples = match &resamplers[src] {
            Some(r) => r.resample(&chunk, false),
            None => chunk,
        };
        mixer.push(slot(src), &samples);
        let tracks = mixer.pop_tracks();
        let mixed = if let Some(engine) = engine.as_mut() {
            engine.feed_tracks(&tracks, &mut events)
        } else {
            let mixed = mix(&tracks);
            if let Err(error) = apple.as_mut().unwrap().feed(&app, &mixed) {
                failure = Some(error);
                break;
            }
            mixed
        };
        if mixed.is_empty() {
            continue;
        }
        for s in &mixed {
            level_sum += s * s;
        }
        level_n += mixed.len();
        if last_level.elapsed() >= Duration::from_millis(100) && level_n > 0 {
            let rms = (level_sum / level_n as f32).sqrt();
            let _ = app.emit("voice-level", rms);
            level_sum = 0.0;
            level_n = 0;
            last_level = Instant::now();
        }
        emit_events(&app, &mut events);
        if let Some(engine) = engine.as_mut() {
            if !engine.in_speech && !stop.load(Ordering::SeqCst) {
                engine.review_one(&mut events);
                emit_events(&app, &mut events);
            }
        }
    }
    drop(streams);
    #[cfg(target_os = "macos")]
    drop(system_audio);
    if let Some(engine) = engine.as_mut() {
        engine.finish(&mut events);
        for mut retry in engine.retries.drain(..) {
            let review = retry.heard.review.as_mut().unwrap();
            review.decision = "cancelled".into();
            retry.heard.revision = u32::from(!review.original.is_empty());
            events.push(Event::Final(retry.heard));
        }
        emit_events(&app, &mut events);
        engine.reset();
    }
    if let Some(apple) = apple.as_mut() {
        if failure.is_none() && !voice.shutting_down.load(Ordering::SeqCst) {
            failure = apple.finish(&app, sources).err();
        }
    }
    stop.store(true, Ordering::SeqCst);
    if let Some(error) = &failure {
        let _ = app.emit("voice-error", error);
    }
    set_status(
        &app,
        VoiceStatus {
            installed: installed(&app),
            message: failure,
            ..Default::default()
        },
    );
}

fn emit_events(app: &AppHandle, events: &mut Vec<Event>) {
    for e in events.drain(..) {
        match e {
            Event::Partial(text) => {
                let _ = app.emit("voice-partial", text);
            }
            Event::Final(heard) => {
                let _ = app.emit("voice-final", heard);
            }
        }
    }
}

#[cfg(test)]
#[path = "voice_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "voice_eval.rs"]
mod eval;
