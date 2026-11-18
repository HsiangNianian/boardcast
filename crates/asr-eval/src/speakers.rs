//! How well speakers are told apart, on the meeting recordings whose references say who spoke when.
//!
//! Each recording is heard as the app hears it, and every sentence's voiceprint is kept, in
//! `<output>/sentences/<id>.json`, so that other ways of grouping voices can be measured without hearing
//! it again. Sentences are then given to speakers by the app's own rule, at several thresholds, and each
//! is compared with the reference speaker it overlaps most:
//!
//! - purity: of each speaker found, how much is one real person;
//! - coverage: of each real person, how much ended up as one speaker found;
//! - F1 of the two, and how many speakers were found against how many there were;
//! - F1 over sentences of two seconds or more alone, as splitting must not make long turns worse;
//! - mixed: the share of sentences in which two real people each speak for a second or more.
//!
//! `--turns` sets whether sentences are split where the diarization model hears the speaker change, and at
//! which latency (slow, medium, fast or off; slow is the app's). Each has its own folder of sentences.

use crate::corpus::{self, Corpus};
use anyhow::{ensure, Context, Result};
use boardcast_audio::{Ear, Latency, Recognizers, Sentence, SAMPLE_RATE};
use boardcast_board::speakers::{identify_in_slot_at, Voice};
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
    /// The diarization model's speaker slot.
    #[serde(default)]
    slot: Option<usize>,
}

/// A sentence this long or longer counts as a long turn.
const LONG: f64 = 2.0;
/// Two people speaking this long each, in seconds, make a sentence mixed.
const MIXED: f64 = 1.0;

/// The `--turns` value: off, or a latency.
pub fn turns(value: &str) -> Result<Option<Latency>> {
    Ok(match value {
        "off" => None,
        "slow" => Some(Latency::Slow),
        "medium" => Some(Latency::Medium),
        "fast" => Some(Latency::Fast),
        other => anyhow::bail!("--turns is off, slow, medium or fast, not {other}"),
    })
}

/// The thresholds measured, the app's own among them.
const THRESHOLDS: [f64; 6] = [0.35, 0.4, 0.45, 0.5, 0.55, 0.6];
const APP: f64 = 0.5;
const NEW_VOICE: f64 = 1.5;

pub fn run(root: &Path, only: Option<&[String]>, output: &Path, reuse: bool, latency: Option<Latency>) -> Result<()> {
    let mut corpus: Corpus = corpus::load(root, "meetings")?;
    corpus.samples.retain(|s| s.reference.iter().any(|t| !t.speaker.is_empty()) && only.is_none_or(|o| o.contains(&s.id)));
    ensure!(!corpus.samples.is_empty(), "no meeting with speakers in its reference");
    let cache = output.join(match latency {
        None => "sentences",
        Some(Latency::Slow) => "sentences-turns-slow",
        Some(Latency::Medium) => "sentences-turns-medium",
        Some(Latency::Fast) => "sentences-turns-fast",
    });
    std::fs::create_dir_all(&cache)?;
    let models = root.join("resources/voice");
    let mut rows = vec![];
    for s in &corpus.samples {
        let kept = cache.join(format!("{}.json", s.id));
        let heard: Vec<Heard> = if reuse && kept.is_file() {
            serde_json::from_slice(&std::fs::read(&kept)?)?
        } else {
            let path = corpus::audio_dir(root).join(&s.file);
            ensure!(path.is_file(), "{} is missing; run `asr-eval download . --suite meetings` first", path.display());
            let wave = Wave::read(&path.to_string_lossy()).with_context(|| format!("reading {}", s.file))?;
            ensure!(wave.sample_rate() == SAMPLE_RATE, "{} is not 16 kHz", s.file);
            // A new ear for each recording: an ear counts time from the first sample it was given.
            let mut ear = Ear::with(&models, Recognizers::load(&models))?.turns(&models, latency);
            let mut out: Vec<Heard> = vec![];
            let mut keep = |s: Sentence| out.push(Heard { start: s.start, seconds: s.seconds, voiceprint: s.voiceprint, slot: s.slot });
            for chunk in wave.samples().chunks(1600) {
                ear.feed(chunk, &mut keep);
            }
            ear.finish(&mut keep);
            std::fs::write(&kept, serde_json::to_vec(&out)?)?;
            out
        };
        let turns: Vec<(f64, f64, String)> = s.reference.iter().filter(|t| !t.speaker.is_empty()).map(|t| (t.start, t.end, t.speaker.clone())).collect();
        let truth: Vec<Option<String>> = heard.iter().map(|h| speaker_at(&turns, h.start, h.start + h.seconds)).collect();
        let long: Vec<bool> = heard.iter().map(|h| h.seconds >= LONG).collect();
        let mixed = heard.iter().filter(|h| is_mixed(&turns, h.start, h.start + h.seconds)).count() as f64 / heard.len().max(1) as f64;
        let people = { let mut p: Vec<&String> = turns.iter().map(|t| &t.2).collect(); p.sort(); p.dedup(); p.len() };
        for same in THRESHOLDS {
            let found = assign(&heard, same, NEW_VOICE);
            let score = measure(&truth, &found);
            let keep = |v: &[Option<String>]| v.iter().zip(&long).filter(|(_, l)| **l).map(|(x, _)| x.clone()).collect::<Vec<_>>();
            let long_score = measure(&keep(&truth), &keep(&found));
            rows.push(Row { same, people, score, long: long_score, mixed });
        }
        println!("{}: {} sentences, {people} people, {:.0}% mixed", s.id, heard.len(), mixed * 100.0);
    }
    let report = report(&rows, latency);
    std::fs::write(output.join("speakers.md"), &report)?;
    println!("{report}");
    if let Some(summary) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        let _ = std::fs::OpenOptions::new().append(true).open(summary).and_then(|mut f| std::io::Write::write_all(&mut f, report.as_bytes()));
    }
    Ok(())
}

/// Whether two reference speakers each speak `MIXED` seconds or more within a span.
fn is_mixed(turns: &[(f64, f64, String)], start: f64, end: f64) -> bool {
    let mut talk: HashMap<&str, f64> = HashMap::new();
    for (a, b, who) in turns {
        let o = end.min(*b) - start.max(*a);
        if o > 0.0 {
            *talk.entry(who).or_default() += o;
        }
    }
    talk.values().filter(|t| **t >= MIXED).count() >= 2
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
    overlap.into_iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|(w, _)| w.to_string())
}

/// Speakers as the app gives them: by voice where a sentence has one; otherwise by the name its diarization
/// slot last had; otherwise the last speaker.
fn assign(heard: &[Heard], same: f64, new_voice: f64) -> Vec<Option<String>> {
    let mut voices: Vec<Voice> = vec![];
    let mut last: Option<String> = None;
    let mut slots: HashMap<usize, String> = HashMap::new();
    heard
        .iter()
        .map(|h| {
            let slot_name = h.slot.and_then(|slot| slots.get(&slot)).cloned();
            let by_voice = h.voiceprint.as_ref().and_then(|print| {
                let print: Vec<f64> = print.iter().map(|x| f64::from(*x)).collect();
                identify_in_slot_at(&mut voices, &print, h.seconds, &[], slot_name.as_deref(), same, new_voice)
            });
            match (by_voice, h.slot) {
                (Some(name), slot) => {
                    if let Some(slot) = slot {
                        slots.insert(slot, name.clone());
                    }
                    last = Some(name);
                }
                (None, Some(slot)) => {
                    if let Some(name) = slots.get(&slot) {
                        last = Some(name.clone());
                    }
                }
                (None, None) => {}
            }
            last.clone()
        })
        .collect()
}

struct Row {
    same: f64,
    people: usize,
    score: Score,
    long: Score,
    mixed: f64,
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
        if self.purity + self.coverage == 0.0 { 0.0 } else { 2.0 * self.purity * self.coverage / (self.purity + self.coverage) }
    }
}

/// Purity and coverage over sentences that have both a real speaker and a found one.
fn measure(truth: &[Option<String>], found: &[Option<String>]) -> Score {
    let pairs: Vec<(&str, &str)> = truth.iter().zip(found).filter_map(|(t, f)| Some((t.as_deref()?, f.as_deref()?))).collect();
    if pairs.is_empty() {
        return Score::default();
    }
    let majority = |by_first: bool| -> f64 {
        let mut groups: HashMap<&str, HashMap<&str, usize>> = HashMap::new();
        for (t, f) in &pairs {
            let (key, value) = if by_first { (*f, *t) } else { (*t, *f) };
            *groups.entry(key).or_default().entry(value).or_default() += 1;
        }
        groups.values().map(|g| *g.values().max().unwrap_or(&0)).sum::<usize>() as f64 / pairs.len() as f64
    };
    let mut names: Vec<&str> = pairs.iter().map(|p| p.1).collect();
    names.sort();
    names.dedup();
    Score { purity: majority(true), coverage: majority(false), found: names.len(), sentences: pairs.len() }
}

fn report(rows: &[Row], latency: Option<Latency>) -> String {
    let split = match latency {
        None => "not split".to_string(),
        Some(l) => format!("split where the speaker changes ({l:?} latency)"),
    };
    let mut out = format!("## Telling speakers apart\n\nSentences {split}, given to speakers by the app's rule and compared with who the reference says spoke. Purity: how much of each speaker found is one person; coverage: how much of each person is one speaker found. Long: sentences of {LONG} s or more. Mixed: sentences in which two people each speak {MIXED} s or more.\n\n| Threshold | Sentences | Mixed | Purity | Coverage | F1 | F1, long | Speakers found / real, per meeting |\n|---:|---:|---:|---:|---:|---:|---:|---|\n");
    for same in THRESHOLDS {
        let these: Vec<&Row> = rows.iter().filter(|r| r.same == same).collect();
        let total: usize = these.iter().map(|r| r.score.sentences).sum::<usize>().max(1);
        let weighted = |f: &dyn Fn(&Score) -> f64| these.iter().map(|r| f(&r.score) * r.score.sentences as f64).sum::<f64>() / total as f64;
        let long_total: usize = these.iter().map(|r| r.long.sentences).sum::<usize>().max(1);
        let long_f1 = these.iter().map(|r| r.long.f1() * r.long.sentences as f64).sum::<f64>() / long_total as f64;
        let mixed = these.iter().map(|r| r.mixed * r.score.sentences as f64).sum::<f64>() / total as f64;
        let counts: Vec<String> = these.iter().map(|r| format!("{}/{}", r.score.found, r.people)).collect();
        let mark = if same == APP { " (app)" } else { "" };
        let _ = writeln!(out, "| {same:.2}{mark} | {total} | {:.1}% | {:.3} | {:.3} | {:.3} | {long_f1:.3} | {} |", mixed * 100.0, weighted(&|s| s.purity), weighted(&|s| s.coverage), weighted(&|s| s.f1()), counts.join(" "));
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
        assert_eq!((perfect.purity, perfect.coverage, perfect.found), (1.0, 1.0, 2));
        let merged = measure(&t(&["x", "x", "y", "y"]), &t(&["A", "A", "A", "A"]));
        assert_eq!((merged.purity, merged.coverage), (0.5, 1.0));
        let split = measure(&t(&["x", "x", "x", "x"]), &t(&["A", "B", "A", "C"]));
        assert_eq!((split.purity, split.coverage), (1.0, 0.5));
        assert_eq!(speaker_at(&[(0.0, 2.0, "x".into()), (1.5, 5.0, "y".into())], 1.0, 4.0).as_deref(), Some("y"));
    }
}
