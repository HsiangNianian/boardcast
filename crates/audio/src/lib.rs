//! The audio chain: microphone or file -> 16 kHz mono -> Silero VAD -> SenseVoice -> finished text.
//! It knows nothing about boards; callers receive each sentence through a callback.

use anyhow::{anyhow, Context, Result};
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig, SileroVadModelConfig, VadModelConfig, VoiceActivityDetector, Wave};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

pub const SAMPLE_RATE: i32 = 16_000;
const VAD_WINDOW: usize = 512;
/// Same pause as the desktop app: this long a silence ends a sentence.
const MIN_SILENCE: f32 = 0.45;

/// A finished sentence, its recognition time, and when the sentence was detected as over.
pub trait Heard: FnMut(String, u128, Instant) {}
impl<F: FnMut(String, u128, Instant)> Heard for F {}

pub struct Ear {
    vad: VoiceActivityDetector,
    asr: OfflineRecognizer,
    pending: Vec<f32>,
}

impl Ear {
    /// `models` holds `silero_vad.onnx` and `sense-voice/`.
    pub fn load(models: &Path) -> Result<Self> {
        let path = |p: &str| models.join(p).to_string_lossy().to_string();
        let vad_config = VadModelConfig {
            silero_vad: SileroVadModelConfig {
                model: Some(path("silero_vad.onnx")),
                threshold: 0.5,
                min_silence_duration: MIN_SILENCE,
                min_speech_duration: 0.25,
                window_size: VAD_WINDOW as i32,
                max_speech_duration: 10.0,
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
        Ok(Self { vad, asr, pending: vec![] })
    }

    pub fn feed(&mut self, samples: &[f32], heard: &mut impl Heard) {
        self.pending.extend_from_slice(samples);
        let whole = self.pending.len() / VAD_WINDOW * VAD_WINDOW;
        for chunk in self.pending[..whole].chunks_exact(VAD_WINDOW) {
            self.vad.accept_waveform(chunk);
        }
        self.pending.drain(..whole);
        self.drain(heard);
    }

    fn drain(&mut self, heard: &mut impl Heard) {
        while let Some(segment) = self.vad.front() {
            let ended = Instant::now();
            let stream = self.asr.create_stream();
            stream.accept_waveform(SAMPLE_RATE, segment.samples());
            self.asr.decode(&stream);
            let text = stream.get_result().map(|r| r.text.trim().to_string()).unwrap_or_default();
            let asr_ms = ended.elapsed().as_millis();
            self.vad.pop();
            if !text.is_empty() {
                heard(text, asr_ms, ended);
            }
        }
    }

    /// The input ended: whatever is still buffered counts as a sentence.
    pub fn finish(&mut self, heard: &mut impl Heard) {
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
    let host = cpal::host_from_id(cpal::HostId::PulseAudio).unwrap_or_else(|_| cpal::default_host());
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
