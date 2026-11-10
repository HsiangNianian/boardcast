//! How well speakers are told apart, on the meeting recordings whose references say who spoke when.
//!
//! Each recording is heard as the app hears it, and every sentence's voiceprint is kept, in
//! `<output>/sentences/<id>.json`, so that other ways of grouping voices can be measured without hearing
//! it again. Sentences are then given to speakers by the app's own rule, at several thresholds, and each
//! is compared with the reference speaker it overlaps most:
//!
//! - purity: of each speaker found, how much is one real person;
//! - coverage: of each real person, how much ended up as one speaker found;
//! - F1 of the two, and how many speakers were found against how many there were.

use crate::corpus::{self, Corpus};
use anyhow::{ensure, Context, Result};
use boardcast_audio::{Ear, Recognizers, Sentence, SAMPLE_RATE};
use boardcast_board::speakers::{identify_sentence_at, Voice};
use serde::{Deserialize, Serialize};
use sherpa_onnx::Wave;
use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct Heard {
    start: f64,
    seconds: f64,
    voiceprint: Option<Vec<f32>>,
}

/// The thresholds measured, the app's own among them.
const THRESHOLDS: [f64; 6] = [0.35, 0.4, 0.45, 0.5, 0.55, 0.6];
const APP: f64 = 0.45;
const NEW_VOICE: f64 = 1.5;

pub fn run(root: &Path, only: Option<&[String]>, output: &Path, reuse: bool) -> Result<()> {
    let mut corpus: Corpus = corpus::load(root, "meetings")?;
    corpus.samples.retain(|s| {
        s.reference.iter().any(|t| !t.speaker.is_empty()) && only.is_none_or(|o| o.contains(&s.id))
    });
    ensure!(
        !corpus.samples.is_empty(),
        "no meeting with speakers in its reference"
    );
    let cache = output.join("sentences");
    std::fs::create_dir_all(&cache)?;
    let models = root.join("resources/voice");
    let mut rows = vec![];
    for s in &corpus.samples {
        let kept = cache.join(format!("{}.json", s.id));
        let heard: Vec<Heard> = if reuse && kept.is_file() {
            serde_json::from_slice(&std::fs::read(&kept)?)?
        } else {
            let path = corpus::audio_dir(root).join(&s.file);
            ensure!(
                path.is_file(),
                "{} is missing; run `asr-eval download . --suite meetings` first",
                path.display()
            );
            let wave = Wave::read(&path.to_string_lossy())
                .with_context(|| format!("reading {}", s.file))?;
            ensure!(
                wave.sample_rate() == SAMPLE_RATE,
                "{} is not 16 kHz",
                s.file
            );
            // A new ear for each recording: an ear counts time from the first sample it was given.
            let mut ear = Ear::with(&models, Recognizers::load(&models))?;
            let mut out: Vec<Heard> = vec![];
            let mut keep = |s: Sentence| {
                out.push(Heard {
                    start: s.start,
                    seconds: s.seconds,
                    voiceprint: s.voiceprint,
                })
            };
            for chunk in wave.samples().chunks(1600) {
                ear.feed(chunk, &mut keep);
            }
            ear.finish(&mut keep);
            std::fs::write(&kept, serde_json::to_vec(&out)?)?;
            out
        };
        let turns: Vec<(f64, f64, String)> = s
            .reference
            .iter()
            .filter(|t| !t.speaker.is_empty())
            .map(|t| (t.start, t.end, t.speaker.clone()))
            .collect();
        let truth: Vec<Option<String>> = heard
            .iter()
            .map(|h| speaker_at(&turns, h.start, h.start + h.seconds))
            .collect();
        let people = {
            let mut p: Vec<&String> = turns.iter().map(|t| &t.2).collect();
            p.sort();
            p.dedup();
            p.len()
        };
        for same in THRESHOLDS {
            let found = assign(&heard, same, NEW_VOICE);
            let score = measure(&truth, &found);
            rows.push((s.id.clone(), same, people, score));
        }
        println!("{}: {} sentences, {people} people", s.id, heard.len());
    }
    let report = report(&rows);
    std::fs::write(output.join("speakers.md"), &report)?;
    println!("{report}");
    if let Some(summary) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        let _ = std::fs::OpenOptions::new()
            .append(true)
            .open(summary)
            .and_then(|mut f| std::io::Write::write_all(&mut f, report.as_bytes()));
    }
    Ok(())
}

/// The reference speaker a span overlaps most, if any.
fn speaker_at(turns: &[(f64, f64, String)], start: f64, end: f64) -> Option<String> {
    let mut overlap: HashMap<&str, f64> = HashMap::new();
    for (a, b, who) in turns {
        let o = end.min(*b) - start.max(*a);
        if o > 0.0 {
            *overlap.entry(who).or_default() += o;
        }
    }
    overlap
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(w, _)| w.to_string())
}

/// Speakers as the app gives them: by voice where a sentence has one, otherwise the last speaker.
fn assign(heard: &[Heard], same: f64, new_voice: f64) -> Vec<Option<String>> {
    let mut voices: Vec<Voice> = vec![];
    let mut last: Option<String> = None;
    heard
        .iter()
        .map(|h| {
            if let Some(print) = &h.voiceprint {
                let print: Vec<f64> = print.iter().map(|x| f64::from(*x)).collect();
                if let Some(name) =
                    identify_sentence_at(&mut voices, &print, h.seconds, &[], same, new_voice)
                {
                    last = Some(name);
                }
            }
            last.clone()
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default)]
struct Score {
    purity: f64,
    coverage: f64,
    found: usize,
    sentences: usize,
}

impl Score {
    fn f1(&self) -> f64 {
        if self.purity + self.coverage == 0.0 {
            0.0
        } else {
            2.0 * self.purity * self.coverage / (self.purity + self.coverage)
        }
    }
}

/// Purity and coverage over sentences that have both a real speaker and a found one.
fn measure(truth: &[Option<String>], found: &[Option<String>]) -> Score {
    let pairs: Vec<(&str, &str)> = truth
        .iter()
        .zip(found)
        .filter_map(|(t, f)| Some((t.as_deref()?, f.as_deref()?)))
        .collect();
    if pairs.is_empty() {
        return Score::default();
    }
    let majority = |by_first: bool| -> f64 {
        let mut groups: HashMap<&str, HashMap<&str, usize>> = HashMap::new();
        for (t, f) in &pairs {
            let (key, value) = if by_first { (*f, *t) } else { (*t, *f) };
            *groups.entry(key).or_default().entry(value).or_default() += 1;
        }
        groups
            .values()
            .map(|g| *g.values().max().unwrap_or(&0))
            .sum::<usize>() as f64
            / pairs.len() as f64
    };
    let mut names: Vec<&str> = pairs.iter().map(|p| p.1).collect();
    names.sort();
    names.dedup();
    Score {
        purity: majority(true),
        coverage: majority(false),
        found: names.len(),
        sentences: pairs.len(),
    }
}

fn report(rows: &[(String, f64, usize, Score)]) -> String {
    let mut out = String::from("## Telling speakers apart\n\nSentences given to speakers by the app's rule, compared with who the reference says spoke. Purity: how much of each speaker found is one person; coverage: how much of each person is one speaker found.\n\n| Threshold | Purity | Coverage | F1 | Speakers found / real, per meeting |\n|---:|---:|---:|---:|---|\n");
    for same in THRESHOLDS {
        let these: Vec<&(String, f64, usize, Score)> =
            rows.iter().filter(|r| r.1 == same).collect();
        let total: usize = these.iter().map(|r| r.3.sentences).sum::<usize>().max(1);
        let weighted = |f: &dyn Fn(&Score) -> f64| {
            these
                .iter()
                .map(|r| f(&r.3) * r.3.sentences as f64)
                .sum::<f64>()
                / total as f64
        };
        let counts: Vec<String> = these
            .iter()
            .map(|r| format!("{}/{}", r.3.found, r.2))
            .collect();
        let mark = if same == APP { " (app)" } else { "" };
        let _ = writeln!(
            out,
            "| {same:.2}{mark} | {:.3} | {:.3} | {:.3} | {} |",
            weighted(&|s| s.purity),
            weighted(&|s| s.coverage),
            weighted(&|s| s.f1()),
            counts.join(" ")
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purity_and_coverage_see_splits_and_merges() {
        let t = |v: &[&str]| v.iter().map(|s| Some(s.to_string())).collect::<Vec<_>>();
        let perfect = measure(&t(&["x", "x", "y", "y"]), &t(&["A", "A", "B", "B"]));
        assert_eq!(
            (perfect.purity, perfect.coverage, perfect.found),
            (1.0, 1.0, 2)
        );
        let merged = measure(&t(&["x", "x", "y", "y"]), &t(&["A", "A", "A", "A"]));
        assert_eq!((merged.purity, merged.coverage), (0.5, 1.0));
        let split = measure(&t(&["x", "x", "x", "x"]), &t(&["A", "B", "A", "C"]));
        assert_eq!((split.purity, split.coverage), (1.0, 0.5));
        assert_eq!(
            speaker_at(&[(0.0, 2.0, "x".into()), (1.5, 5.0, "y".into())], 1.0, 4.0).as_deref(),
            Some("y")
        );
    }
}
