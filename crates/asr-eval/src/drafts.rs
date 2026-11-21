//! How good the drafts shown while someone speaks are (#291), on the meeting recordings. Each is heard as the
//! app hears it, without splitting by speaker, which drafts do not depend on, and for every stretch of speech:
//!
//! - first text: from the moment speech began to be loud to the first draft, in seconds of input;
//! - error: the last draft before the stretch ended against the text of the sentences heard from it, as a
//!   character or word error rate in the meeting's language, so how near a draft is to what replaces it;
//! - rewritten: of the characters drafts showed, the share a later draft took back, which reads as flicker;
//! - missed: stretches whose sentences had text but which no draft showed;
//! - cost: time spent drafting over the length of the recording.

use crate::corpus::{self, Corpus};
use crate::score::error_rate;
use anyhow::{bail, ensure, Context, Result};
use alcyo_audio::{load_parakeet, load_reazon, Draft, DraftWith, Ear, Language, Recognizers, Sentence, SAMPLE_RATE};
use sherpa_onnx::Wave;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// `reread`, `stream:<folder of a streaming model>`, or `bylanguage:<folder>`, the app's: the stream for
/// Chinese and reading again for the rest.
pub fn with(value: &str) -> Result<DraftWith> {
    Ok(match value.split_once(':') {
        None if value == "reread" => DraftWith::Reread,
        Some(("stream", dir)) => DraftWith::Stream(PathBuf::from(dir)),
        Some(("bylanguage", dir)) => DraftWith::ByLanguage(PathBuf::from(dir)),
        _ => bail!("--drafts is reread, stream:<folder> or bylanguage:<folder>, not {value}"),
    })
}

#[derive(Default)]
struct Totals {
    stretches: usize,
    drafted: usize,
    first: Vec<f64>,
    errors: usize,
    units: usize,
    shown: usize,
    taken_back: usize,
    drafting: f64,
    audio: f64,
}

/// Letters and digits only, lowercased: punctuation and spacing move as a draft grows, which is not a word
/// taken back.
fn words_of(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Characters `before` and `after` share at the start.
fn common_prefix(before: &str, after: &str) -> usize {
    before.chars().zip(after.chars()).take_while(|(a, b)| a == b).count()
}

/// The speech models a package carries, as packaging/models.sh says: `all`, or `en` (Parakeet only) and
/// `ja` (ReazonSpeech only), which have no SenseVoice and no streaming Chinese model, so they draft by
/// reading again with their one recognizer.
pub fn variant(value: &str) -> Result<String> {
    ensure!(["all", "en", "ja"].contains(&value), "--variant is all, en or ja, not {value}");
    Ok(value.to_string())
}

/// Only the recognizers the package carries are loaded, so measuring one needs no more memory than it.
fn recognizers(models: &Path, variant: &str) -> Recognizers {
    let only = |english, japanese| Recognizers { system: None, sense_voice: None, english, japanese, sense_voice_dir: None, chinese: None, language: Language::Auto };
    match variant {
        "en" => only(load_parakeet(&models.join("parakeet-en")), None),
        "ja" => only(None, load_reazon(&models.join("reazon-ja"))),
        _ => Recognizers::load(models),
    }
}

/// Whether a package for one language hears this meeting: each is measured on its own language.
fn heard_by(variant: &str, language: &str) -> bool {
    match variant {
        "en" => language == "en",
        "ja" => language == "ja",
        _ => true,
    }
}

pub fn run(root: &Path, only: Option<&[String]>, output: &Path, drafts: DraftWith, variant: &str) -> Result<()> {
    // A package for one language has no streaming model, so it reads again whatever was asked.
    let drafts = if variant == "all" { drafts } else { DraftWith::Reread };
    let mut corpus: Corpus = corpus::load(root, "meetings")?;
    corpus.samples.retain(|s| s.language != "silence" && heard_by(variant, &s.language) && only.is_none_or(|o| o.contains(&s.id)));
    ensure!(!corpus.samples.is_empty(), "no meeting to draft");
    // Each package's results go to a folder of their own, so measuring several never mixes them.
    let output = &output.join(variant);
    std::fs::create_dir_all(output)?;
    let models = root.join("resources/voice");
    let mut rows = String::new();
    let mut all = Totals::default();
    for s in &corpus.samples {
        let path = corpus::audio_dir(root).join(&s.file);
        ensure!(path.is_file(), "{} is missing; run `asr-eval download . --suite meetings` first", path.display());
        let wave = Wave::read(&path.to_string_lossy()).with_context(|| format!("reading {}", s.file))?;
        ensure!(wave.sample_rate() == SAMPLE_RATE, "{} is not 16 kHz", s.file);
        let got: Arc<Mutex<Vec<Draft>>> = Arc::default();
        let keep = got.clone();
        let mut ear = Ear::with(&models, recognizers(&models, variant))?.turns(&models, None).drafts(drafts.clone(), Box::new(move |d| keep.lock().expect("drafts").push(d)));
        let mut said: BTreeMap<usize, String> = BTreeMap::new();
        let mut hear = |sentence: Sentence| {
            let text = said.entry(sentence.utterance).or_default();
            if !text.is_empty() && !sentence.text.is_empty() {
                text.push(' ');
            }
            text.push_str(&sentence.text);
        };
        for chunk in wave.samples().chunks(1600) {
            ear.feed(chunk, &mut hear);
        }
        ear.finish(&mut hear);
        let drafts = got.lock().expect("drafts").clone();
        let mut t = Totals { audio: wave.samples().len() as f64 / f64::from(SAMPLE_RATE), drafting: ear.drafting_seconds(), ..Totals::default() };
        let language = if s.language == "zh-en" { "zh" } else { s.language.as_str() };
        let mut pairs = vec![];
        for (utterance, text) in said.iter().filter(|(_, text)| !text.trim().is_empty()) {
            let these: Vec<&Draft> = drafts.iter().filter(|d| d.utterance == *utterance).collect();
            pairs.push(serde_json::json!({ "utterance": utterance, "said": text, "drafts": these.iter().map(|d| serde_json::json!({ "at": d.at, "text": d.text })).collect::<Vec<_>>() }));
            t.stretches += 1;
            let Some(last) = these.last() else { continue };
            t.drafted += 1;
            t.first.push(these[0].at - these[0].spoken_from);
            let e = error_rate(text, &last.text, language);
            t.errors += e.errors;
            t.units += e.units;
            let mut before = String::new();
            for d in &these {
                let now = words_of(&d.text);
                let kept = common_prefix(&before, &now);
                t.taken_back += before.chars().count() - kept;
                t.shown += now.chars().count().saturating_sub(kept);
                before = now;
            }
        }
        // Each stretch's drafts beside the text that replaced them, to read.
        std::fs::write(output.join(format!("{}.json", s.id)), serde_json::to_vec_pretty(&pairs)?)?;
        let line = summary(&s.id, &t);
        println!("{line}");
        rows.push_str(&line);
        rows.push('\n');
        all.stretches += t.stretches;
        all.drafted += t.drafted;
        all.first.extend(t.first);
        all.errors += t.errors;
        all.units += t.units;
        all.shown += t.shown;
        all.taken_back += t.taken_back;
        all.drafting += t.drafting;
        all.audio += t.audio;
    }
    let how = match &drafts {
        DraftWith::Reread => "reread by the bundled recognizer".to_string(),
        DraftWith::Stream(dir) => format!("streamed by {}", dir.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())),
        DraftWith::ByLanguage(dir) => format!("by language: Chinese streamed by {}, the rest reread", dir.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())),
    };
    let mut report = format!(
        "## Drafts ({how}, {variant} package)\n\nPer stretch of speech: when the first draft came after speech began to be loud (median, 90th percentile), how far the last draft was from the text that replaced it (CER for Chinese and Japanese, WER for English), the share of shown characters a later draft took back, the stretches with text but no draft, and drafting time over audio time.\n\n| Meeting | Stretches | First text, median / P90 | Error | Rewritten | Missed | Cost |\n|---|---:|---:|---:|---:|---:|---:|\n{rows}"
    );
    let _ = writeln!(report, "{}", summary("**all**", &all));
    std::fs::write(output.join("drafts.md"), &report)?;
    println!("{report}");
    if let Some(summary) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        let _ = std::fs::OpenOptions::new().append(true).open(summary).and_then(|mut f| std::io::Write::write_all(&mut f, report.as_bytes()));
    }
    Ok(())
}

fn summary(name: &str, t: &Totals) -> String {
    let mut first = t.first.clone();
    first.sort_by(f64::total_cmp);
    let at = |q: f64| first.get(((first.len() as f64 - 1.0) * q).round() as usize).copied().unwrap_or(f64::NAN);
    let share = |a: usize, b: usize| if b == 0 { f64::NAN } else { a as f64 / b as f64 * 100.0 };
    format!(
        "| {name} | {} | {:.2} / {:.2} s | {:.1}% | {:.1}% | {} | {:.3} |",
        t.stretches,
        at(0.5),
        at(0.9),
        share(t.errors, t.units),
        share(t.taken_back, t.shown),
        t.stretches - t.drafted,
        t.drafting / t.audio.max(1e-9)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taken_back_counts_what_a_later_draft_changed() {
        assert_eq!(common_prefix("今天天气", "今天天气很好"), 4);
        assert_eq!(common_prefix("今天天汽", "今天天气很好"), 3);
        assert_eq!(common_prefix(&words_of("今天天气。"), &words_of("今天天气，很好。")), 4);
        assert_eq!(words_of("Hello, World"), "helloworld");
        assert!(matches!(with("reread").unwrap(), DraftWith::Reread));
        assert!(matches!(with("stream:/m").unwrap(), DraftWith::Stream(p) if p == Path::new("/m")));
        assert!(with("zipformer").is_err());
        assert!(heard_by("all", "zh") && heard_by("en", "en") && !heard_by("en", "zh") && !heard_by("ja", "en"));
        assert!(variant("en").is_ok() && variant("zh").is_err());
    }
}
