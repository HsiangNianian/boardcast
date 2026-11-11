//! Compare glossary off/on on the exact same recognition of the user's local recordings (#117).
use crate::{corpus::sha256_of, run::Hearing, score::{error_rate, ErrorRate}};
use anyhow::{ensure, Context, Result};
use boardcast_agent::sound::Glossary;
use boardcast_audio::{Ear, Recognizers, SAMPLE_RATE};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sherpa_onnx::Wave;
use std::{collections::{BTreeMap, HashSet}, fmt::Write, path::Path};

#[derive(Deserialize)]
struct Sample {
    id: String,
    language: String,
    text: String,
}

#[derive(Serialize)]
struct Pair {
    raw: String,
    corrected: String,
    before: ErrorRate,
    after: ErrorRate,
}

fn compare(sample: &Sample, sentences: &[String], glossary: &Glossary) -> Pair {
    let raw = sentences.join(" ");
    // Correct each final sentence, just as the app does, without recognizing the audio a second time.
    let corrected = sentences.iter().map(|s| glossary.correct(s)).collect::<Vec<_>>().join(" ");
    Pair {
        before: error_rate(&sample.text, &raw, &sample.language),
        after: error_rate(&sample.text, &corrected, &sample.language),
        raw,
        corrected,
    }
}

#[derive(Default, Serialize)]
struct Total {
    samples: usize,
    units: usize,
    before_errors: usize,
    after_errors: usize,
    improved: usize,
    worsened: usize,
}

pub fn run(root: &Path) -> Result<()> {
    let set = root.join("eval/asr/own-voice");
    let samples: Vec<Sample> = serde_json::from_slice(&std::fs::read(set.join("sentences.json"))?)?;
    ensure!(!samples.is_empty(), "no reference sentences");
    let mut ids = HashSet::new();
    for sample in &samples {
        ensure!(!sample.id.is_empty() && sample.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') && ids.insert(&sample.id), "invalid or duplicate sample id");
        ensure!(["zh", "ja", "en", "zh-en"].contains(&sample.language.as_str()) && !sample.text.trim().is_empty(), "{} needs a language and reference", sample.id);
    }
    let missing: Vec<&str> = samples.iter().filter(|s| !set.join(format!("recordings/{}.wav", s.id)).is_file()).map(|s| s.id.as_str()).collect();
    ensure!(missing.is_empty(), "{} recordings missing ({}); run sh eval/asr/own-voice/record.sh first", missing.len(), missing.join(", "));
    let terms = std::fs::read_to_string(set.join("glossary.txt"))?;
    let glossary = Glossary::parse(&terms);
    ensure!(!glossary.is_empty(), "glossary.txt is empty");
    let models = root.join("resources/voice");
    let recognizers = Recognizers::load(&models);
    ensure!(!recognizers.is_empty(), "no speech models in {}", models.display());
    let mut hearing = Hearing::Ear(Box::new(Ear::with(&models, recognizers)?));
    let mut totals: BTreeMap<String, Total> = BTreeMap::new();
    let mut rows = vec![];
    let mut report = String::from("# Own-voice glossary comparison\n\nThe same recognized sentences are scored with correction off and on. These recordings and transcripts stay local.\n\n| Sample | Before errors / units | After errors / units |\n| --- | ---: | ---: |\n");
    let mut unheard = vec![];
    for sample in &samples {
        let path = set.join(format!("recordings/{}.wav", sample.id));
        let wave = Wave::read(&path.to_string_lossy()).with_context(|| format!("reading {}", sample.id))?;
        ensure!(wave.sample_rate() == SAMPLE_RATE && !wave.samples().is_empty(), "{} must contain 16 kHz audio", sample.id);
        let heard = hearing.hear(wave.samples());
        let sentences: Vec<String> = heard.iter().map(|(text, _, _)| text.clone()).collect();
        let pair = compare(sample, &sentences, &glossary);
        if pair.raw.trim().is_empty() {
            unheard.push(sample.id.clone());
        }
        let total = totals.entry(sample.language.clone()).or_default();
        total.samples += 1;
        total.units += pair.before.units;
        total.before_errors += pair.before.errors;
        total.after_errors += pair.after.errors;
        total.improved += usize::from(pair.after.errors < pair.before.errors);
        total.worsened += usize::from(pair.after.errors > pair.before.errors);
        writeln!(report, "| {} | {} / {} | {} / {} |", sample.id, pair.before.errors, pair.before.units, pair.after.errors, pair.after.units)?;
        rows.push(json!({"id":sample.id, "language":sample.language, "reference":sample.text, "audio_sha256":sha256_of(&path)?, "recognized_sentences":heard, "comparison":pair}));
    }
    report += "\n| Language | Samples | Before | After | Improved / worsened samples |\n| --- | ---: | ---: | ---: | ---: |\n";
    for (language, total) in &totals {
        ensure!(total.units > 0, "{language} has no reference units");
        writeln!(report, "| {language} | {} | {:.2}% | {:.2}% | {} / {} |", total.samples, 100.0 * total.before_errors as f64 / total.units as f64, 100.0 * total.after_errors as f64 / total.units as f64, total.improved, total.worsened)?;
    }
    let out = root.join("eval/reports/own-voice");
    std::fs::create_dir_all(&out)?;
    std::fs::write(out.join("report.json"), serde_json::to_vec_pretty(&json!({"glossary":terms, "totals":totals, "samples":rows, "unheard":unheard}))?)?;
    std::fs::write(out.join("report.md"), &report)?;
    println!("{report}\nReports in {}", out.display());
    ensure!(unheard.is_empty(), "speech was not recognized in {}", unheard.join(", "));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_comparison_preserves_raw_text_and_counts_helpful_and_harmful_changes() {
        let terms = Glossary::parse("鉴权; cache");
        let zh = Sample { id: "zh".into(), language: "zh".into(), text: "先做鉴权".into() };
        let improved = compare(&zh, &["先做健全".into()], &terms);
        assert_eq!(improved.raw, "先做健全");
        assert_eq!(improved.before.errors, 2);
        assert_eq!(improved.after.errors, 0);
        let en = Sample { id: "en".into(), language: "en".into(), text: "cash".into() };
        let worsened = compare(&en, &["cash".into()], &terms);
        assert_eq!(worsened.before.errors, 0);
        assert_eq!(worsened.after.errors, 1);
    }
}
