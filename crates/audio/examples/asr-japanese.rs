//! SenseVoice against ReazonSpeech on Japanese: short clips, and a long recording cut into sentences by
//! the same speech detector the app uses. ReazonSpeech aborted the whole process on a very short piece
//! in an earlier evaluation, so pieces under `MIN_SECONDS` go to SenseVoice alone.
//! Usage: asr-japanese <voice-models> <reazonspeech-dir> <cases.json>
//! `cases.json` lists `{ "id", "wav", "text", "long" }`; long recordings are cut into sentences first.

use boardcast_audio::{load_sense_voice, transcribe, SAMPLE_RATE};
use serde_json::Value;
use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig, SileroVadModelConfig,
    VadModelConfig, VoiceActivityDetector, Wave,
};
use std::path::Path;
use std::time::Instant;

const MIN_SECONDS: f32 = 0.3;

fn reazon(dir: &Path) -> anyhow::Result<OfflineRecognizer> {
    let file = |n: &str| Some(dir.join(n).to_string_lossy().to_string());
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.transducer = OfflineTransducerModelConfig {
        encoder: file("encoder-epoch-99-avg-1.int8.onnx"),
        decoder: file("decoder-epoch-99-avg-1.int8.onnx"),
        joiner: file("joiner-epoch-99-avg-1.int8.onnx"),
    };
    config.model_config.tokens = file("tokens.txt");
    config.model_config.num_threads = 2;
    OfflineRecognizer::create(&config).ok_or_else(|| anyhow::anyhow!("ReazonSpeech did not load"))
}

/// Sentences, as the app's speech detector cuts them.
fn sentences(models: &Path, samples: &[f32]) -> Vec<Vec<f32>> {
    let config = VadModelConfig {
        silero_vad: SileroVadModelConfig {
            model: Some(models.join("silero_vad.onnx").to_string_lossy().to_string()),
            threshold: 0.5,
            min_silence_duration: 0.45,
            min_speech_duration: 0.25,
            window_size: 512,
            max_speech_duration: 10.0,
        },
        sample_rate: SAMPLE_RATE,
        num_threads: 1,
        ..Default::default()
    };
    let vad = VoiceActivityDetector::create(&config, 60.0).expect("VAD");
    let mut out = vec![];
    for chunk in samples.chunks(512) {
        if chunk.len() == 512 {
            vad.accept_waveform(chunk);
        }
        while let Some(segment) = vad.front() {
            out.push(segment.samples().to_vec());
            vad.pop();
        }
    }
    vad.flush();
    while let Some(segment) = vad.front() {
        out.push(segment.samples().to_vec());
        vad.pop();
    }
    out
}

fn chars(text: &str) -> Vec<char> {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn edits(a: &[char], b: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let here = row[j + 1];
            row[j + 1] = (prev + usize::from(x != y)).min(row[j] + 1).min(here + 1);
            prev = here;
        }
    }
    row[b.len()]
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (models, reazon_dir, cases) = (Path::new(&args[0]), Path::new(&args[1]), &args[2]);
    let sense = load_sense_voice(&models.join("sense-voice"))?;
    let reazon = reazon(reazon_dir)?;
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(cases)?)?;
    for long in [false, true] {
        let (mut errors, mut units, mut times, mut pieces, mut short) =
            ([0_usize; 2], 0, [0.0_f64; 2], 0, 0);
        for case in cases
            .iter()
            .filter(|c| c["long"].as_bool().unwrap_or(false) == long)
        {
            let Some(wave) = Wave::read(case["wav"].as_str().unwrap_or_default()) else {
                continue;
            };
            let parts = if long {
                sentences(models, wave.samples())
            } else {
                vec![wave.samples().to_vec()]
            };
            let mut said = [String::new(), String::new()];
            for part in &parts {
                pieces += 1;
                let t = Instant::now();
                let by_sense = transcribe(&sense, part);
                times[0] += t.elapsed().as_secs_f64();
                let by_reazon = if (part.len() as f32) < MIN_SECONDS * SAMPLE_RATE as f32 {
                    short += 1;
                    by_sense.clone()
                } else {
                    let t = Instant::now();
                    let text = transcribe(&reazon, part);
                    times[1] += t.elapsed().as_secs_f64();
                    text
                };
                said[0].push_str(&by_sense);
                said[1].push_str(&by_reazon);
            }
            let reference = chars(case["text"].as_str().unwrap_or_default());
            for (i, text) in said.iter().enumerate() {
                errors[i] += edits(&reference, &chars(text));
            }
            units += reference.len();
            if !long {
                println!(
                    "{}\n  ref    {}\n  sense  {}\n  reazon {}",
                    case["id"].as_str().unwrap_or("?"),
                    case["text"].as_str().unwrap_or_default(),
                    said[0],
                    said[1]
                );
            }
        }
        if units == 0 {
            continue;
        }
        println!(
            "\n{} ({pieces} pieces, {short} under {MIN_SECONDS} s left to SenseVoice):",
            if long {
                "long recordings"
            } else {
                "short clips"
            }
        );
        for (i, name) in ["SenseVoice", "ReazonSpeech"].iter().enumerate() {
            println!(
                "  {name:<13} CER {:>6.2}%  ({}/{units})  recognizing {:.1} s",
                100.0 * errors[i] as f64 / units as f64,
                errors[i],
                times[i]
            );
        }
    }
    Ok(())
}
