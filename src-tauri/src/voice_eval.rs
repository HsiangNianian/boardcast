//! File-based corpus evaluation uses the desktop's actual segmentation and recognition engine.
use super::*;
use serde::Deserialize;
use sherpa_onnx::{
    OfflineSpeechDenoiser, OfflineSpeechDenoiserConfig, OfflineWhisperModelConfig, Wave,
};
use std::path::PathBuf;

#[derive(Deserialize)]
struct Sample {
    id: String,
    path: PathBuf,
}

#[derive(Deserialize)]
struct Input {
    models: PathBuf,
    extra_models: PathBuf,
    model: String,
    denoise: String,
    vad: String,
    samples: Vec<Sample>,
    output: PathBuf,
}

#[test]
#[ignore = "run pnpm eval:asr to download and verify the public corpus and models"]
fn corpus() {
    let input = std::env::var("BOARDCAST_ASR_INPUT").expect("Run pnpm eval:asr");
    let input: Input = serde_json::from_slice(&std::fs::read(input).expect("Read eval input"))
        .expect("Invalid eval input");
    assert!(!input.samples.is_empty(), "Empty evaluation is not a pass");
    assert!(matches!(
        input.model.as_str(),
        "sensevoice" | "whisper-base"
    ));
    assert!(matches!(input.denoise.as_str(), "none" | "gtcrn"));
    assert!(matches!(input.vad.as_str(), "on" | "off"));
    let loading = Instant::now();
    let mut engine = Engine::load(&input.models).expect("Load the desktop voice engine");
    if input.model == "whisper-base" {
        let path = |name: &str| input.extra_models.join(name).to_string_lossy().into_owned();
        let mut config = OfflineRecognizerConfig::default();
        config.model_config.whisper = OfflineWhisperModelConfig {
            encoder: Some(path("base-encoder.int8.onnx")),
            decoder: Some(path("base-decoder.int8.onnx")),
            language: None,
            task: Some("transcribe".into()),
            tail_paddings: -1,
            ..Default::default()
        };
        config.model_config.tokens = Some(path("base-tokens.txt"));
        config.model_config.num_threads = 2;
        engine.asr = OfflineRecognizer::create(&config).expect("Load Whisper base int8");
    }
    let denoiser = if input.denoise == "gtcrn" {
        let mut config = OfflineSpeechDenoiserConfig::default();
        config.model.gtcrn.model = Some(
            input
                .extra_models
                .join("gtcrn_simple.onnx")
                .to_string_lossy()
                .into_owned(),
        );
        Some(OfflineSpeechDenoiser::create(&config).expect("Load GTCRN"))
    } else {
        None
    };
    let load_ms = loading.elapsed().as_secs_f64() * 1000.0;
    let mut results = vec![];
    for sample in input.samples {
        let wave = Wave::read(&sample.path.to_string_lossy()).expect("Read corpus WAV");
        assert_eq!(wave.sample_rate(), SAMPLE_RATE, "{}", sample.id);
        assert!(!wave.samples().is_empty(), "{}", sample.id);
        assert!(
            wave.samples().iter().all(|v| v.is_finite()),
            "{}",
            sample.id
        );
        engine.reset();
        let started = Instant::now();
        let samples = if let Some(denoiser) = &denoiser {
            let mut enhanced = denoiser.run(wave.samples(), SAMPLE_RATE);
            assert_eq!(enhanced.sample_rate, SAMPLE_RATE);
            assert!(enhanced.samples.len().abs_diff(wave.samples().len()) <= 512);
            enhanced.samples.resize(wave.samples().len(), 0.0);
            assert!(enhanced.samples.iter().all(|v| v.is_finite()));
            enhanced.samples
        } else {
            wave.samples().to_vec()
        };
        let enhancement_ms = started.elapsed().as_secs_f64() * 1000.0;
        let mut events = vec![];
        if input.vad == "on" {
            // The same 100 ms input chunks as capture; no microphone or GUI is opened.
            for chunk in samples.chunks(SAMPLE_RATE as usize / 10) {
                engine.feed(chunk, &mut events);
            }
            engine.finish(&mut events);
        } else {
            // Bound long-meeting inference without detecting or dropping silence.
            let window = (MAX_SPEECH * SAMPLE_RATE as f32) as usize;
            for (i, chunk) in samples.chunks(window).enumerate() {
                engine.fed = i * window + chunk.len();
                engine.finalize(chunk, i * window, false, &mut events);
            }
        }
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        let mut partials = 0;
        let finals: Vec<Heard> = events
            .into_iter()
            .filter_map(|event| match event {
                Event::Partial(_) => {
                    partials += 1;
                    None
                }
                Event::Final(mut heard) => {
                    heard.voiceprint = None;
                    Some(heard)
                }
            })
            .collect();
        let duration = wave.samples().len() as f64 / SAMPLE_RATE as f64;
        assert!(finals
            .iter()
            .all(|h| h.start >= 0.0 && h.end as f64 <= duration + 0.1 && h.start < h.end));
        eprintln!(
            "{}: {} finals, {:.2}s processing / {:.2}s audio",
            sample.id,
            finals.len(),
            elapsed_ms / 1000.0,
            duration
        );
        results.push(serde_json::json!({
            "id": sample.id, "duration": duration, "elapsedMs": elapsed_ms,
            "inputRms": rms(wave.samples()),
            "inputPeak": wave.samples().iter().fold(0.0_f32, |peak, x| peak.max(x.abs())),
            "vadSegments": (input.vad == "on").then_some(engine.vad_segments),
            "vadSeconds": (input.vad == "on").then_some(engine.vad_samples as f64 / SAMPLE_RATE as f64),
            "enhancementMs": enhancement_ms, "partials": partials, "segments": finals,
        }));
    }
    std::fs::write(
        input.output,
        serde_json::to_vec_pretty(&serde_json::json!({
            "model": input.model, "denoise": input.denoise, "vad": input.vad,
            "loadMs": load_ms, "samples": results,
        }))
        .unwrap(),
    )
    .expect("Write evaluation results");
}
