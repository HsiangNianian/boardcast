//! Hearing every recording of a suite as the app does, scoring what was heard, and writing a report.

use crate::corpus::{self, Corpus};
use crate::score::{error_rate, keyword_recall, recognition_failures, ErrorRate, Recall};
use anyhow::{bail, ensure, Context, Result};
use boardcast_audio::{
    load_sense_voice, transcribe, Ear, Engine, Recognizers, Sentence, SAMPLE_RATE,
};
use serde::Serialize;
use serde_json::json;
use sherpa_onnx::OfflineRecognizer;
use sherpa_onnx::Wave;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct Options {
    pub suite: String,
    /// `routed`: SenseVoice, Parakeet and ReazonSpeech, routed by language as the app does. `sensevoice`: SenseVoice alone.
    pub engine: String,
    /// Off cuts fixed 10-second windows instead of detecting speech: a control, not something the app does.
    pub vad: bool,
    /// `gain` first brings each recording to the same loudness.
    pub enhance: String,
    pub only: Option<Vec<String>>,
    pub output: Option<PathBuf>,
}

impl Options {
    fn label(&self) -> String {
        format!(
            "{}-{}-vad-{}-{}",
            self.engine,
            self.enhance,
            if self.vad { "on" } else { "off" },
            self.suite
        )
    }

    /// The configuration the app ships: its speech checks are enforced, while the controls' are reported.
    fn production(&self) -> bool {
        self.engine == "routed" && self.vad && self.enhance == "none"
    }
}

#[derive(Serialize)]
struct Detail {
    id: String,
    language: String,
    scoring: String,
    industry: Option<String>,
    reference: String,
    hypothesis: String,
    score: Option<ErrorRate>,
    keywords: Recall,
    /// Before any enhancement.
    input_rms: f32,
    input_peak: f32,
    segments: usize,
    speech_seconds: f64,
    /// How many sentences each recognizer wrote.
    engines: Vec<(String, usize)>,
    duration: f64,
    elapsed_ms: u128,
    rtf: f64,
}

#[derive(Serialize)]
struct Aggregate {
    language: String,
    samples: usize,
    errors: usize,
    units: usize,
    rate: Option<f64>,
}

/// Bring speech to about -20 dBFS RMS, never past a peak of 0.95, as crates/audio/examples/asr-compare does.
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

fn engine_name(engine: Engine) -> &'static str {
    match engine {
        Engine::SenseVoice => "sensevoice",
        Engine::Parakeet => "parakeet",
        Engine::Reazon => "reazonspeech",
        Engine::System => "system",
    }
}

/// How recordings are heard: as the app does, or in fixed windows through SenseVoice alone.
enum Hearing {
    Ear(Box<Ear>),
    Windows(OfflineRecognizer),
}

impl Hearing {
    /// Each sentence with the recognizer that wrote it, and how long it was.
    fn hear(&mut self, samples: &[f32]) -> Vec<(String, &'static str, f64)> {
        match self {
            Self::Windows(asr) => {
                let window = SAMPLE_RATE as usize * 10;
                samples
                    .chunks(window)
                    .map(|w| {
                        (
                            transcribe(asr, w),
                            "sensevoice",
                            w.len() as f64 / f64::from(SAMPLE_RATE),
                        )
                    })
                    .collect()
            }
            Self::Ear(ear) => {
                let mut heard: Vec<Sentence> = vec![];
                let mut keep = |s: Sentence| heard.push(s);
                // Faster than real time, in pieces about as long as the microphone's.
                for chunk in samples.chunks(1600) {
                    ear.feed(chunk, &mut keep);
                }
                ear.finish(&mut keep);
                heard
                    .into_iter()
                    .map(|s| (s.text, engine_name(s.engine), s.seconds))
                    .collect()
            }
        }
    }
}

pub fn run(root: &Path, options: &Options) -> Result<()> {
    ensure!(
        ["routed", "sensevoice"].contains(&options.engine.as_str()),
        "the engine is routed or sensevoice"
    );
    ensure!(
        ["none", "gain"].contains(&options.enhance.as_str()),
        "enhance is none or gain"
    );
    ensure!(
        options.vad || options.engine == "sensevoice",
        "VAD off is a control for SenseVoice alone"
    );
    let mut corpus: Corpus = corpus::load(root, &options.suite)?;
    if let Some(only) = &options.only {
        corpus.samples.retain(|s| only.contains(&s.id));
        ensure!(!corpus.samples.is_empty(), "no matching samples");
    }
    let audio = corpus::audio_dir(root);
    for s in &corpus.samples {
        let path = audio.join(&s.file);
        ensure!(
            path.is_file(),
            "{} is missing; run `asr-eval download` first",
            path.display()
        );
        ensure!(
            corpus::sha256_of(&path)? == s.sha256,
            "{} does not match its SHA-256; download it again",
            s.file
        );
    }

    let models = root.join("resources/voice");
    let started = Instant::now();
    let mut hearing = if options.vad {
        let recognizers = match options.engine.as_str() {
            "routed" => Recognizers::load(&models),
            _ => Recognizers {
                system: None,
                sense_voice: Some(load_sense_voice(&models.join("sense-voice"))?),
                english: None,
                japanese: None,
                sense_voice_dir: Some(models.join("sense-voice")),
                chinese: None,
            },
        };
        ensure!(
            !recognizers.is_empty(),
            "no speech models in {}; fetch them first",
            models.display()
        );
        Hearing::Ear(Box::new(Ear::with(&models, recognizers)?))
    } else {
        Hearing::Windows(load_sense_voice(&models.join("sense-voice"))?)
    };
    let load_ms = started.elapsed().as_millis();

    let mut details: Vec<Detail> = vec![];
    for s in &corpus.samples {
        let wave = Wave::read(&audio.join(&s.file).to_string_lossy())
            .with_context(|| format!("reading {}", s.file))?;
        ensure!(
            wave.sample_rate() == SAMPLE_RATE,
            "{} is not 16 kHz",
            s.file
        );
        let raw = wave.samples();
        let input_rms = (raw.iter().map(|x| x * x).sum::<f32>() / raw.len().max(1) as f32).sqrt();
        let input_peak = raw.iter().fold(0.0_f32, |m, x| m.max(x.abs()));
        let samples = if options.enhance == "gain" {
            louder(raw)
        } else {
            raw.to_vec()
        };
        let t0 = Instant::now();
        let heard = hearing.hear(&samples);
        let elapsed_ms = t0.elapsed().as_millis();
        let hypothesis = heard
            .iter()
            .map(|(t, _, _)| t.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let mut engines: Vec<(String, usize)> = vec![];
        for (_, engine, _) in &heard {
            match engines.iter_mut().find(|(e, _)| e == engine) {
                Some((_, n)) => *n += 1,
                None => engines.push(((*engine).to_string(), 1)),
            }
        }
        let reference = s.reference_text();
        let detail = Detail {
            id: s.id.clone(),
            language: s.language.clone(),
            scoring: s.scoring.clone(),
            industry: s.industry.clone(),
            score: s
                .scored()
                .then(|| error_rate(&reference, &hypothesis, &s.language)),
            keywords: keyword_recall(&s.keywords, &hypothesis, &s.language),
            reference,
            hypothesis,
            input_rms,
            input_peak,
            segments: heard.len(),
            speech_seconds: heard.iter().map(|(_, _, secs)| secs).sum(),
            engines,
            duration: s.duration,
            elapsed_ms,
            rtf: elapsed_ms as f64 / 1000.0 / s.duration,
        };
        eprintln!(
            "{}: {} · rtf {:.3}",
            detail.id,
            detail
                .score
                .as_ref()
                .and_then(|e| e.rate)
                .map_or_else(|| "unscored".into(), |r| format!("{:.2}%", r * 100.0)),
            detail.rtf
        );
        details.push(detail);
    }
    report(root, &corpus, options, &details, load_ms)
}

fn report(
    root: &Path,
    corpus: &Corpus,
    options: &Options,
    details: &[Detail],
    load_ms: u128,
) -> Result<()> {
    let aggregates: Vec<Aggregate> = ["zh", "ja", "en", "zh-en"]
        .iter()
        .map(|language| {
            let rows: Vec<&Detail> = details
                .iter()
                .filter(|d| d.language == *language && d.scoring == "reference")
                .collect();
            let (errors, units) = rows
                .iter()
                .filter_map(|d| d.score.as_ref())
                .fold((0, 0), |(e, u), s| (e + s.errors, u + s.units));
            Aggregate {
                language: (*language).into(),
                samples: rows.len(),
                errors,
                units,
                rate: (units > 0).then(|| errors as f64 / units as f64),
            }
        })
        .collect();
    let failures = recognition_failures(
        details
            .iter()
            .map(|d| (d.id.as_str(), d.scoring.as_str(), d.hypothesis.as_str())),
    );
    let label = options.label();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let dir = options.output.clone().unwrap_or_else(|| {
        root.join("eval/reports/asr")
            .join(format!("{stamp}-{label}"))
    });
    ensure!(
        !dir.join("report.json").exists(),
        "refusing to overwrite a previous run: {}",
        dir.display()
    );
    std::fs::create_dir_all(&dir)?;
    let commit = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    let manifest = root.join("eval/asr").join(if options.suite == "meetings" {
        "meetings.json"
    } else {
        "corpus.json"
    });
    let report = json!({
        "corpus": corpus.version,
        "manifest_sha256": corpus::sha256_of(&manifest)?,
        "commit": commit,
        "label": label,
        "suite": options.suite,
        "engine": options.engine,
        "vad": options.vad,
        "enhance": options.enhance,
        "load_ms": load_ms,
        "checks": { "enforced": options.production(), "passed": failures.is_empty(), "failures": failures },
        "aggregates": aggregates,
        "details": details,
    });
    std::fs::write(
        dir.join("report.json"),
        serde_json::to_string_pretty(&report)? + "\n",
    )?;

    let pct = |r: Option<f64>| r.map_or_else(|| "—".into(), |r| format!("{:.2}%", r * 100.0));
    let metric = |l: &str| match l {
        "en" => "WER",
        "zh-en" => "MER",
        _ => "CER",
    };
    let mut md = format!(
        "# ASR · {label}\n\nCorpus: {}. Commit: {commit}.\n\n",
        corpus.version
    );
    md += match (options.engine.as_str(), options.vad) {
        ("routed", _) => "The app's own hearing: speech detection, SenseVoice for every sentence, then Parakeet for English and ReazonSpeech for Japanese where they are present, with voiceprints. No reference text reaches the recognizer.\n\n",
        (_, true) => "SenseVoice alone, with the app's speech detection and voiceprints.\n\n",
        (_, false) => "Speech detection bypassed: fixed 10-second windows, silence included, through SenseVoice alone. Windows may split words. This is a control, not something the app does.\n\n",
    };
    md += "Audio is fed faster than real time. RTF is processing time over audio duration, not microphone-to-screen latency.\n\n";
    let enforced = if options.production() {
        "enforced for the configuration the app ships"
    } else {
        "reported only, for this control"
    };
    let _ = writeln!(
        md,
        "Speech checks: **{}** ({enforced}).\n",
        if failures.is_empty() { "PASS" } else { "FAIL" }
    );
    for f in &failures {
        let _ = writeln!(md, "- {f}");
    }
    md += "\n| Language | Metric | Reference samples | Errors / units | Rate |\n|---|---|---:|---:|---:|\n";
    for a in &aggregates {
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} / {} | {} |",
            a.language,
            metric(&a.language),
            a.samples,
            a.errors,
            a.units,
            pct(a.rate)
        );
    }
    md += "\nOnly reference-scored samples enter the language totals. Ordered scores join overlapping turns by start time and can penalise a different valid order; provisional scores use edited minutes or unchecked text. Both are diagnostic. A missing score is not zero errors.\n\n| Sample / industry | Reference | Error rate | Keyword recall | Input dBFS | Speech seconds | Sentences | Recognizers | RTF |\n|---|---|---:|---:|---:|---:|---:|---|---:|\n";
    for d in details {
        let db = if d.input_rms > 0.0 {
            format!("{:.1}", 20.0 * d.input_rms.log10())
        } else {
            "−∞".into()
        };
        let engines: Vec<String> = d.engines.iter().map(|(e, n)| format!("{e} {n}")).collect();
        let _ = writeln!(
            md,
            "| {}{} | {} | {} | {} | {db} | {:.2} | {} | {} | {:.3} |",
            d.id,
            d.industry
                .as_ref()
                .map_or_else(String::new, |i| format!(" / {i}")),
            d.scoring,
            pct(d.score.as_ref().and_then(|s| s.rate)),
            pct(d.keywords.rate),
            d.speech_seconds,
            d.segments,
            engines.join(", "),
            d.rtf
        );
    }
    for d in details {
        let _ = write!(
            md,
            "\n## {}\n\nReference ({}):\n\n{}\n\nRecognized:\n\n{}\n",
            d.id,
            d.scoring,
            if d.reference.is_empty() {
                "(silence)"
            } else {
                &d.reference
            },
            if d.hypothesis.is_empty() {
                "(empty)"
            } else {
                &d.hypothesis
            }
        );
    }
    std::fs::write(dir.join("report.md"), &md)?;
    if let Ok(summary) = std::env::var("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(summary)?
            .write_all(md.as_bytes())?;
    }
    println!("Report: {}", dir.join("report.md").display());
    if options.production() && !failures.is_empty() {
        bail!("{}", failures.join("; "));
    }
    if !failures.is_empty() {
        eprintln!(
            "{}control: {}",
            if std::env::var("GITHUB_ACTIONS").is_ok() {
                "::warning::"
            } else {
                ""
            },
            failures.join("; ")
        );
    }
    Ok(())
}

/// Compare several runs' language totals side by side; they must be on the same corpus and suite.
pub fn compare(paths: &[PathBuf]) -> Result<()> {
    ensure!(
        !paths.is_empty(),
        "no reports: an empty comparison is not a pass"
    );
    let reports: Vec<serde_json::Value> = paths
        .iter()
        .map(|p| {
            Ok(serde_json::from_slice(
                &std::fs::read(p).with_context(|| format!("reading {}", p.display()))?,
            )?)
        })
        .collect::<Result<_>>()?;
    for key in ["manifest_sha256", "suite"] {
        ensure!(
            reports.iter().all(|r| r[key] == reports[0][key]),
            "the reports differ in {key}"
        );
    }
    let mut md = String::from("# ASR comparison\n\nLower is better. RTF includes speech detection and voiceprints, excludes loading, and is not live latency; compare timings on the same hardware only.\n\n| Configuration |");
    for l in ["zh", "ja", "en", "zh-en"] {
        let _ = write!(md, " {l} |");
    }
    md += " RTF p50 | Checks |\n|---|---:|---:|---:|---:|---:|---|\n";
    for r in &reports {
        let rate = |l: &str| {
            r["aggregates"]
                .as_array()
                .and_then(|a| a.iter().find(|x| x["language"] == l))
                .and_then(|x| x["rate"].as_f64())
                .map_or_else(|| "—".into(), |v| format!("{:.2}%", v * 100.0))
        };
        let mut rtf: Vec<f64> = r["details"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| d["rtf"].as_f64())
            .collect();
        rtf.sort_by(f64::total_cmp);
        let p50 = rtf
            .get(rtf.len() / 2)
            .map_or_else(|| "—".into(), |v| format!("{v:.3}"));
        let checks = if r["checks"]["passed"].as_bool() == Some(true) {
            "pass"
        } else {
            "fail"
        };
        let _ = writeln!(
            md,
            "| {} | {} | {} | {} | {} | {p50} | {checks} |",
            r["label"].as_str().unwrap_or("?"),
            rate("zh"),
            rate("ja"),
            rate("en"),
            rate("zh-en")
        );
    }
    if let Ok(summary) = std::env::var("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(summary)?
            .write_all(md.as_bytes())?;
    }
    print!("{md}");
    Ok(())
}
