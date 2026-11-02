//! File-based corpus evaluation uses the desktop's actual segmentation and recognition engine.
use super::*;
use serde::Deserialize;
use sherpa_onnx::{
    OfflineCohereTranscribeModelConfig, OfflineParaformerModelConfig, OfflineQwen3ASRModelConfig,
    OfflineSpeechDenoiser, OfflineSpeechDenoiserConfig, OfflineTransducerModelConfig,
    OfflineWhisperModelConfig, Wave,
};
use std::path::PathBuf;

#[derive(Deserialize)]
struct Sample {
    id: String,
    path: PathBuf,
    variant: String,
}

#[derive(Deserialize)]
struct Input {
    models: PathBuf,
    extra_models: PathBuf,
    model: String,
    recognizer: Recognizer,
    denoise: String,
    vad: String,
    policy: String,
    samples: Vec<Sample>,
    output: PathBuf,
}

#[derive(Deserialize)]
#[serde(tag = "family", rename_all = "lowercase", deny_unknown_fields)]
enum Recognizer {
    Sensevoice,
    Whisper {
        encoder: String,
        decoder: String,
        tokens: String,
    },
    Paraformer {
        model: String,
        tokens: String,
    },
    Transducer {
        encoder: String,
        decoder: String,
        joiner: String,
        tokens: String,
    },
    Qwen {
        conv_frontend: String,
        encoder: String,
        decoder: String,
        tokenizer: String,
    },
    Cohere {
        encoder: String,
        decoder: String,
        tokens: String,
        language: String,
    },
}

impl Recognizer {
    fn config(&self, root: &Path) -> Option<OfflineRecognizerConfig> {
        let p = |name: &str| Some(root.join(name).to_string_lossy().into_owned());
        let mut config = OfflineRecognizerConfig::default();
        let m = &mut config.model_config;
        m.num_threads = 2;
        m.provider = Some("cpu".into());
        match self {
            Self::Sensevoice => return None,
            Self::Whisper {
                encoder,
                decoder,
                tokens,
            } => {
                m.whisper = OfflineWhisperModelConfig {
                    encoder: p(encoder),
                    decoder: p(decoder),
                    language: None,
                    task: Some("transcribe".into()),
                    tail_paddings: -1,
                    ..Default::default()
                };
                m.tokens = p(tokens);
            }
            Self::Paraformer { model, tokens } => {
                m.paraformer = OfflineParaformerModelConfig { model: p(model) };
                m.tokens = p(tokens);
            }
            Self::Transducer {
                encoder,
                decoder,
                joiner,
                tokens,
            } => {
                m.transducer = OfflineTransducerModelConfig {
                    encoder: p(encoder),
                    decoder: p(decoder),
                    joiner: p(joiner),
                };
                m.tokens = p(tokens);
            }
            Self::Qwen {
                conv_frontend,
                encoder,
                decoder,
                tokenizer,
            } => {
                m.qwen3_asr = OfflineQwen3ASRModelConfig {
                    conv_frontend: p(conv_frontend),
                    encoder: p(encoder),
                    decoder: p(decoder),
                    tokenizer: p(tokenizer),
                    ..Default::default()
                };
            }
            Self::Cohere {
                encoder,
                decoder,
                tokens,
                language,
            } => {
                assert!(matches!(language.as_str(), "zh" | "ja" | "en"));
                m.cohere_transcribe = OfflineCohereTranscribeModelConfig {
                    encoder: p(encoder),
                    decoder: p(decoder),
                    language: Some(language.clone()),
                    use_punct: true,
                    use_itn: true,
                };
                m.tokens = p(tokens);
            }
        }
        Some(config)
    }
}

#[test]
#[ignore = "run pnpm eval:asr to download and verify the public corpus and models"]
fn corpus() {
    let input = std::env::var("BOARDCAST_ASR_INPUT").expect("Run pnpm eval:asr");
    let input: Input = serde_json::from_slice(&std::fs::read(input).expect("Read eval input"))
        .expect("Invalid eval input");
    assert!(!input.samples.is_empty(), "Empty evaluation is not a pass");
    assert!(matches!(input.denoise.as_str(), "none" | "gtcrn"));
    assert!(matches!(input.vad.as_str(), "on" | "off"));
    assert!(matches!(input.policy.as_str(), "static" | "dynamic"));
    assert!(input.policy != "dynamic" || input.vad == "on");
    let loading = Instant::now();
    let mut engine = Engine::load(&input.models).expect("Load the desktop voice engine");
    if let Some(config) = input.recognizer.config(&input.extra_models) {
        engine.asr = OfflineRecognizer::create(&config).expect("Load candidate recognizer");
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
        let original = wave.samples();
        let mut seed = 7_u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed as f64 / u32::MAX as f64 * 2.0 - 1.0) as f32
        };
        let tracks = match sample.variant.as_str() {
            "original" => vec![original.to_vec()],
            "quiet" => vec![original.iter().map(|s| s * 0.15).collect()],
            "noise" => {
                let level = (rms(original) * 0.25).max(0.002);
                vec![original
                    .iter()
                    .map(|s| (s + noise() * level).clamp(-1.0, 1.0))
                    .collect()]
            }
            "dual_echo" => {
                // Same utterance in both channels: no new words or invented overlap gold.
                let gain = 0.35 / rms(original).max(0.0001);
                let a: Vec<f32> = original
                    .iter()
                    .map(|s| (s * gain).clamp(-0.98, 0.98))
                    .collect();
                let b = a.iter().map(|s| s * 0.2).collect();
                vec![a, b]
            }
            _ => panic!("Unknown variant {}", sample.variant),
        };
        let baseline = if input.policy == "dynamic" {
            Some(evaluate(
                &mut engine,
                &tracks,
                &input.vad,
                false,
                denoiser.as_ref(),
            ))
        } else {
            None
        };
        let mut result = evaluate(
            &mut engine,
            &tracks,
            &input.vad,
            input.policy == "dynamic",
            denoiser.as_ref(),
        );
        result["id"] = sample.id.clone().into();
        result["variant"] = sample.variant.into();
        result["baseline"] = serde_json::json!(baseline);
        eprintln!(
            "{}: {} finals, {} ms",
            sample.id,
            result["segments"].as_array().unwrap().len(),
            result["elapsedMs"]
        );
        results.push(result);
    }
    std::fs::write(
        input.output,
        serde_json::to_vec_pretty(&serde_json::json!({
            "model": input.model, "denoise": input.denoise, "vad": input.vad, "policy": input.policy,
            "loadMs": load_ms, "samples": results,
        }))
        .unwrap(),
    )
    .expect("Write evaluation results");
}

fn evaluate(
    engine: &mut Engine,
    tracks: &[Vec<f32>],
    vad: &str,
    dynamic: bool,
    denoiser: Option<&OfflineSpeechDenoiser>,
) -> serde_json::Value {
    engine.reset();
    engine.dynamic = dynamic;
    engine.sources.names = if tracks.len() == 2 {
        vec!["mic".into(), "system".into()]
    } else {
        vec!["fixture".into()]
    };
    let started = Instant::now();
    let raw = mix(tracks);
    let processed = if let Some(denoiser) = denoiser {
        let mut enhanced = denoiser.run(&raw, SAMPLE_RATE);
        assert_eq!(enhanced.sample_rate, SAMPLE_RATE);
        assert!(enhanced.samples.len().abs_diff(raw.len()) <= 512);
        enhanced.samples.resize(raw.len(), 0.0);
        assert!(enhanced.samples.iter().all(|v| v.is_finite()));
        vec![enhanced.samples]
    } else {
        tracks.to_vec()
    };
    let enhancement_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut events = vec![];
    if vad == "on" {
        let chunk = SAMPLE_RATE as usize / 10;
        for start in (0..raw.len()).step_by(chunk) {
            let end = (start + chunk).min(raw.len());
            let parts: Vec<Vec<f32>> = processed.iter().map(|t| t[start..end].to_vec()).collect();
            engine.feed_tracks(&parts, &mut events);
            if !engine.in_speech {
                engine.review_one(&mut events);
            }
        }
        engine.finish(&mut events);
        // Offline eval waits for optional work; live Stop cancels queued retries.
        while !engine.retries.is_empty() {
            engine.review_one(&mut events);
        }
    } else {
        let samples = mix(&processed);
        let window = (MAX_SPEECH * SAMPLE_RATE as f32) as usize;
        for (i, chunk) in samples.chunks(window).enumerate() {
            engine.fed = i * window + chunk.len();
            engine.finalize(chunk, i * window, false, &mut events);
        }
    }
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut partials = 0;
    let mut finals: Vec<Heard> = vec![];
    let mut trace = vec![];
    for event in events {
        match event {
            Event::Partial(_) => partials += 1,
            Event::Final(mut h) => {
                h.voiceprint = None;
                trace.push(h.clone());
                if let Some(old) = finals.iter_mut().find(|f| f.id == h.id) {
                    *old = h;
                } else {
                    finals.push(h);
                }
            }
        }
    }
    let duration = raw.len() as f64 / SAMPLE_RATE as f64;
    assert!(finals
        .iter()
        .all(|h| h.start >= 0.0 && h.end as f64 <= duration + 0.1 && h.start < h.end));
    serde_json::json!({
        "duration": duration, "elapsedMs": elapsed_ms, "inputRms": rms(&raw),
        "inputPeak": raw.iter().fold(0.0_f32, |peak, x| peak.max(x.abs())),
        "vadSegments": (vad == "on").then_some(engine.vad_segments),
        "vadSeconds": (vad == "on").then_some(engine.vad_samples as f64 / SAMPLE_RATE as f64),
        "enhancementMs": enhancement_ms, "partials": partials, "segments": finals, "trace": trace,
    })
}
