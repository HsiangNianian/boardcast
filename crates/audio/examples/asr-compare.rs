//! Compare SenseVoice, Parakeet and the app's routing between them on recordings with known text.
//! Usage: asr-compare <voice-models> <cases.json>
//! `cases.json` lists `{ "id", "wav", "text", "language" }`; each recording is one sentence or turn.
//! Chinese is scored by character and English by word, with punctuation and case removed.
//! `ENHANCE=gain` first brings each recording to the same loudness, and `ENHANCE=gtcrn:<model.onnx>`
//! first runs GTCRN denoising, to see whether either helps quiet or noisy recordings.

use boardcast_audio::{is_english, load_parakeet, load_sense_voice, transcribe};
use serde_json::Value;
use sherpa_onnx::Wave;
use std::path::Path;
use std::time::Instant;

fn units(text: &str, language: &str) -> Vec<String> {
    let clean: String = text
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect();
    if language == "en" {
        return clean.split_whitespace().map(str::to_string).collect();
    }
    // Chinese by character, with any English words inside kept whole.
    let mut out = vec![];
    for word in clean.split_whitespace() {
        if word.is_ascii() {
            out.push(word.to_string());
        } else {
            let mut latin = String::new();
            for c in word.chars() {
                if c.is_ascii_alphanumeric() {
                    latin.push(c);
                } else {
                    if !latin.is_empty() {
                        out.push(std::mem::take(&mut latin));
                    }
                    out.push(c.to_string());
                }
            }
            if !latin.is_empty() {
                out.push(latin);
            }
        }
    }
    out
}

fn edits(a: &[String], b: &[String]) -> usize {
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

/// Bring speech to about -20 dBFS RMS, never past a peak of 0.95.
fn louder(samples: &[f32]) -> Vec<f32> {
    let rms = (samples.iter().map(|x| x * x).sum::<f32>() / samples.len().max(1) as f32)
        .sqrt()
        .max(1e-6);
    let peak = samples
        .iter()
        .fold(0.0_f32, |m, x| m.max(x.abs()))
        .max(1e-6);
    let gain = (0.1 / rms).min(0.95 / peak);
    samples.iter().map(|x| x * gain).collect()
}

fn enhance(samples: &[f32]) -> Vec<f32> {
    match std::env::var("ENHANCE").as_deref() {
        Ok("gain") => louder(samples),
        Ok(other) if other.starts_with("gtcrn:") => {
            use sherpa_onnx::{OfflineSpeechDenoiser, OfflineSpeechDenoiserConfig};
            let mut config = OfflineSpeechDenoiserConfig::default();
            config.model.gtcrn.model = Some(other["gtcrn:".len()..].to_string());
            let denoiser = OfflineSpeechDenoiser::create(&config).expect("GTCRN loads");
            denoiser.run(samples, boardcast_audio::SAMPLE_RATE).samples
        }
        _ => samples.to_vec(),
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [models, cases] = args.as_slice() else {
        anyhow::bail!("usage: asr-compare <voice-models> <cases.json>")
    };
    let cases: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(cases)?)?;
    let models = Path::new(models);
    let sense = load_sense_voice(&models.join("sense-voice"))?;
    let parakeet = load_parakeet(&models.join("parakeet-en"))
        .ok_or_else(|| anyhow::anyhow!("Parakeet did not load"))?;
    // (errors, units, seconds of recognizing) per engine and language
    let mut totals: std::collections::BTreeMap<(String, &str), (usize, usize, f64)> =
        Default::default();
    for case in &cases {
        let language = case["language"].as_str().unwrap_or("zh");
        let wav = case["wav"].as_str().unwrap_or_default();
        let Some(wave) = Wave::read(wav) else {
            eprintln!("skipped {wav}: cannot read");
            continue;
        };
        let samples = enhance(wave.samples());
        let reference = units(case["text"].as_str().unwrap_or_default(), language);
        let t = Instant::now();
        let by_sense = transcribe(&sense, &samples);
        let sense_s = t.elapsed().as_secs_f64();
        let t = Instant::now();
        let by_parakeet = transcribe(&parakeet, &samples);
        let parakeet_s = t.elapsed().as_secs_f64();
        let routed = if is_english(&by_sense) && !by_parakeet.is_empty() {
            by_parakeet.clone()
        } else {
            by_sense.clone()
        };
        let routed_s = sense_s
            + if is_english(&by_sense) {
                parakeet_s
            } else {
                0.0
            };
        for (engine, text, secs) in [
            ("sensevoice", &by_sense, sense_s),
            ("parakeet", &by_parakeet, parakeet_s),
            ("routed", &routed, routed_s),
        ] {
            let errors = edits(&reference, &units(text, language));
            let entry = totals.entry((engine.to_string(), language)).or_default();
            (entry.0, entry.1, entry.2) =
                (entry.0 + errors, entry.1 + reference.len(), entry.2 + secs);
        }
        println!(
            "{} [{}]\n  ref       {}\n  sense     {}\n  parakeet  {}\n  routed    {}",
            case["id"].as_str().unwrap_or("?"),
            language,
            case["text"].as_str().unwrap_or_default(),
            by_sense,
            by_parakeet,
            routed
        );
    }
    println!("\nengine      language  error rate   errors/units   recognizing");
    for ((engine, language), (errors, units, secs)) in &totals {
        println!(
            "{engine:<11} {language:<9} {:>9.2}%   {errors:>5}/{units:<6}   {secs:.1}s",
            100.0 * *errors as f64 / (*units).max(1) as f64
        );
    }
    Ok(())
}
