//! 语音输入：麦克风 → 16 kHz → Silero VAD 断句 → SenseVoice 识别 → 事件推给前端。
//!
//! - 说话过程中，每隔 `PARTIAL_EVERY` 把这句已经说了的部分重新识别一遍，作为临时字幕（`voice-partial`）；
//!   SenseVoice 不是流式模型，但几秒钟的音频识别只要几十毫秒，这样做代价很小。
//! - VAD 判断一句话说完（停顿 `MIN_SILENCE` 秒），就用这一整段识别出定稿（`voice-final`），前端拿去出板书。
//! - 音量每 100 ms 推一次（`voice-level`），界面上画一个电平条，让人知道麦克风在收音。
//! - 采集在 Rust 里用 cpal 做，不走浏览器的 getUserMedia：各平台 WebView 的麦克风权限处理不一致。
//! - 线上会议可以同时收系统声音（别人的声音从扬声器出来）：Linux 用默认输出的 monitor 源，
//!   Windows 用 WASAPI loopback；两路各自转成 16 kHz 后混成一路（`Mixer`）再断句识别。
//!
//! - 每句定稿时顺带算一个声纹向量（CAM++，`voiceprint`），前端拿它和这场会已经出现过的声音比，判断是谁在说。
//!   比对和起名放在前端：声纹跟着会议存，中途关麦再开、过几天接着开都认得人。
//!
//! 识别部分（`Engine`）不依赖 Tauri 和声卡，可以直接喂 wav 测试。

use crate::llm;
use crate::models::voice_files;
use serde::Serialize;
use sherpa_onnx::{
    LinearResampler, OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig,
    SileroVadModelConfig, SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig,
    VadModelConfig, VoiceActivityDetector,
};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub const SAMPLE_RATE: i32 = 16_000;
/// Silero VAD 在 16 kHz 下一次吃 512 个样本
const WINDOW: usize = 512;
/// 停顿多久算一句话说完。越短出字越快，但一句话中间换气也可能被切开
const MIN_SILENCE: f32 = 0.45;
/// 一句话最长多少秒，超过就强制切开，免得一直说不停时字幕迟迟不定稿
const MAX_SPEECH: f32 = 12.0;
/// 说话过程中多久刷新一次临时字幕
const PARTIAL_EVERY: Duration = Duration::from_millis(600);
/// 检测到开口时往前多带一点，免得吃掉第一个字
const PREROLL: usize = SAMPLE_RATE as usize / 4;
/// 短于这个长度（样本数）的句子不算声纹：太短的「嗯」「对」声纹不准，前端会归给上一个说话的人
const MIN_VOICEPRINT: usize = SAMPLE_RATE as usize * 4 / 5;

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatus {
    /// 语音模型下好了没有
    pub installed: bool,
    pub listening: bool,
    pub device: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heard {
    pub text: String,
    /// 这句话在本次收音里的起止时间（秒）
    pub start: f32,
    pub end: f32,
    /// 这句话的声纹（单位长度）；没有声纹模型或者句子太短时为 None
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voiceprint: Option<Vec<f32>>,
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
    recent: Vec<f32>,
    in_speech: bool,
    last_partial: Instant,
    partial_text: String,
    /// 已经喂进来的样本总数，用来算时间
    fed: usize,
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
        let vad = VoiceActivityDetector::create(&vad_config, 60.0).ok_or("断句模型加载失败")?;

        let mut asr_config = OfflineRecognizerConfig::default();
        asr_config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(path(voice_files::SENSE_VOICE)),
            language: Some("auto".into()),
            // 数字、日期规整成阿拉伯数字，带标点
            use_itn: true,
        };
        asr_config.model_config.tokens = Some(path(voice_files::TOKENS));
        asr_config.model_config.num_threads = 2;
        let asr = OfflineRecognizer::create(&asr_config).ok_or("识别模型加载失败")?;

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
            recent: vec![],
            in_speech: false,
            last_partial: Instant::now(),
            partial_text: String::new(),
            fed: 0,
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

    /// 喂一段 16 kHz 单声道样本
    pub fn feed(&mut self, samples: &[f32], out: &mut Vec<Event>) {
        self.pending.extend_from_slice(samples);
        while self.pending.len() >= WINDOW {
            let chunk: Vec<f32> = self.pending.drain(..WINDOW).collect();
            self.fed += WINDOW;
            self.vad.accept_waveform(&chunk);

            if self.in_speech {
                self.speech.extend_from_slice(&chunk);
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
                self.last_partial = Instant::now();
            }
            self.drain_segments(out);
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
            let mut rest = std::mem::take(&mut self.pending);
            rest.resize(WINDOW, 0.0);
            self.vad.accept_waveform(&rest);
        }
        self.vad.flush();
        self.drain_segments(out);
    }

    fn drain_segments(&mut self, out: &mut Vec<Event>) {
        while let Some(seg) = self.vad.front() {
            let text = self.recognize(seg.samples());
            let voiceprint = if text.is_empty() {
                None
            } else {
                self.voiceprint(seg.samples())
            };
            let start = seg.start() as f32 / SAMPLE_RATE as f32;
            let end = start + seg.n() as f32 / SAMPLE_RATE as f32;
            self.vad.pop();
            if !text.is_empty() {
                out.push(Event::Final(Heard {
                    text,
                    start,
                    end,
                    voiceprint,
                }));
            }
            self.in_speech = self.vad.detected();
            self.speech.clear();
            self.partial_text.clear();
        }
        if !self.vad.detected() && self.in_speech && self.vad.is_empty() {
            // VAD 已经不认为在说话，但还没给出段落（太短被丢弃了），收尾
            self.in_speech = false;
            self.speech.clear();
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
    pub fn pop(&mut self) -> Vec<f32> {
        let longest = self.queues.iter().map(|q| q.len()).max().unwrap_or(0);
        let shortest = self.queues.iter().map(|q| q.len()).min().unwrap_or(0);
        let n = if longest >= MAX_LAG {
            longest
        } else {
            shortest
        };
        let mut out = vec![0.0f32; n];
        for q in &mut self.queues {
            let take = n.min(q.len());
            for (o, s) in out.iter_mut().zip(q.drain(..take)) {
                *o += s;
            }
        }
        for o in &mut out {
            *o = o.clamp(-1.0, 1.0);
        }
        out
    }
}

// —— 采集和生命周期 ——

struct Running {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

#[derive(Default)]
pub struct Voice {
    running: Mutex<Option<Running>>,
    status: Mutex<VoiceStatus>,
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
    base(app).is_some()
}

fn set_status(app: &AppHandle, s: VoiceStatus) {
    *app.state::<Voice>().status.lock().unwrap() = s.clone();
    let _ = app.emit("voice-status", s);
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

#[tauri::command]
pub fn voice_start(
    app: AppHandle,
    device: Option<String>,
    source: Option<String>,
) -> Result<(), String> {
    let source = Source::parse(source.as_deref());
    if !installed(&app) {
        return Err("语音模型还没下载".into());
    }
    let voice = app.state::<Voice>();
    let mut running = voice.running.lock().unwrap();
    if running.is_some() {
        return Ok(());
    }
    let models = base(&app).ok_or("找不到语音模型")?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let handle = app.clone();
    // 模型加载和 cpal 的流都放在这个线程里：有些平台上 cpal 的 Stream 不能跨线程
    let (ready_tx, ready_rx) = mpsc::channel::<Result<String, String>>();
    let thread = thread::spawn(move || run(handle, models, device, source, flag, ready_tx));
    match ready_rx.recv_timeout(Duration::from_secs(30)) {
        Ok(Ok(name)) => {
            *running = Some(Running { stop, thread });
            set_status(
                &app,
                VoiceStatus {
                    installed: true,
                    listening: true,
                    device: Some(name),
                    message: None,
                },
            );
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
            Err("打开收音超时".into())
        }
    }
}

#[tauri::command]
pub fn voice_stop(app: AppHandle) {
    let taken = app.state::<Voice>().running.lock().unwrap().take();
    if let Some(r) = taken {
        r.stop.store(true, Ordering::SeqCst);
        let _ = r.thread.join();
    }
    set_status(
        &app,
        VoiceStatus {
            installed: installed(&app),
            ..Default::default()
        },
    );
}

/// 模型文件变了（下完或删了）：停掉正在进行的收音，让界面重新读状态
pub fn reload(app: &AppHandle) {
    voice_stop(app.clone());
}

pub fn unload(app: &AppHandle) {
    voice_stop(app.clone());
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
        .ok_or_else(|| "找不到系统声音（PulseAudio / PipeWire 的回采源）".into())
}

#[cfg(windows)]
fn system_source(host: &cpal::Host) -> Result<(cpal::Device, bool), String> {
    use cpal::traits::HostTrait;
    host.default_output_device()
        .map(|d| (d, true))
        .ok_or_else(|| "找不到扬声器，收不了系统声音".into())
}

#[cfg(not(any(target_os = "linux", windows)))]
fn system_source(_host: &cpal::Host) -> Result<(cpal::Device, bool), String> {
    Err("这个系统上暂时收不了系统声音（macOS 需要录屏权限，还没做）".into())
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
        "麦克风"
    } else {
        "系统声音"
    };
    let supported = if loopback {
        dev.default_output_config()
    } else {
        dev.default_input_config()
    }
    .map_err(|e| format!("{what}打不开：{e}"))?;
    let channels = supported.channels() as usize;
    let rate = supported.sample_rate() as i32;
    let config = supported.config();
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
                move |err: cpal::Error| on_err(format!("{what}出错：{err}")),
                None,
            )
        }};
    }
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => stream_of!(f32, |s: f32| s),
        cpal::SampleFormat::I16 => stream_of!(i16, |s: i16| s as f32 / 32768.0),
        cpal::SampleFormat::U16 => stream_of!(u16, |s: u16| (s as f32 - 32768.0) / 32768.0),
        cpal::SampleFormat::I32 => stream_of!(i32, |s: i32| s as f32 / 2147483648.0),
        other => return Err(format!("{what}的采样格式 {other:?} 不支持")),
    }
    .map_err(|e| format!("{what}打不开：{e}"))?;
    Ok((stream, rate))
}

fn run(
    app: AppHandle,
    models: std::path::PathBuf,
    device: Option<String>,
    source: Source,
    stop: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<String, String>>,
) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let mut engine = match Engine::load(&models) {
        Ok(e) => e,
        Err(msg) => {
            let _ = ready.send(Err(msg));
            return;
        }
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

    if source != Source::System {
        // 设置里选的麦克风拔掉了就退回系统默认的，不让收音直接失败
        let chosen = device
            .as_deref()
            .filter(|n| !n.is_empty())
            .and_then(|name| {
                host.input_devices()
                    .ok()
                    .and_then(|mut ds| ds.find(|d| d.description().is_ok_and(|x| x.name() == name)))
            });
        let Some(dev) = chosen.or_else(|| host.default_input_device()) else {
            let _ = ready.send(Err("没有找到麦克风".into()));
            return;
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
                        .unwrap_or_else(|_| "麦克风".into()),
                );
            }
            Err(msg) => {
                let _ = ready.send(Err(msg));
                return;
            }
        }
    }
    if source != Source::Mic {
        let opened = system_source(&host)
            .and_then(|(dev, loopback)| open_stream(&dev, loopback, 1, tx.clone(), on_err.clone()));
        match opened {
            Ok((stream, rate)) => {
                streams.push(stream);
                resamplers[1] = (rate != SAMPLE_RATE)
                    .then(|| LinearResampler::create(rate, SAMPLE_RATE))
                    .flatten();
                names.push("系统声音".into());
            }
            Err(msg) => {
                let _ = ready.send(Err(msg));
                return;
            }
        }
    }
    drop(tx);
    for s in &streams {
        if let Err(err) = s.play() {
            let _ = ready.send(Err(format!("收音打不开：{err}")));
            return;
        }
    }
    let _ = ready.send(Ok(names.join(" + ")));

    // 只开了系统声音时，它是第 1 路；混音器按实际开了几路来
    let mut mixer = Mixer::new(streams.len());
    let slot = |src: usize| if source == Source::System { 0 } else { src };
    let mut events = vec![];
    let mut level_sum = 0.0f32;
    let mut level_n = 0usize;
    let mut last_level = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        let Ok((src, chunk)) = rx.recv_timeout(Duration::from_millis(100)) else {
            continue;
        };
        let samples = match &resamplers[src] {
            Some(r) => r.resample(&chunk, false),
            None => chunk,
        };
        mixer.push(slot(src), &samples);
        let mixed = mixer.pop();
        if mixed.is_empty() {
            continue;
        }
        for s in &mixed {
            level_sum += s * s;
        }
        level_n += mixed.len();
        engine.feed(&mixed, &mut events);
        if last_level.elapsed() >= Duration::from_millis(100) && level_n > 0 {
            let rms = (level_sum / level_n as f32).sqrt();
            let _ = app.emit("voice-level", rms);
            level_sum = 0.0;
            level_n = 0;
            last_level = Instant::now();
        }
        emit_events(&app, &mut events);
    }
    drop(streams);
    engine.finish(&mut events);
    emit_events(&app, &mut events);
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
