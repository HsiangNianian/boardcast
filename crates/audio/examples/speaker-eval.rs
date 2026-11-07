//! How well voices are told apart on a recording whose speakers are known.
//! Usage: speaker-eval <voice-models> <recording.wav> <reference.json>
//! The reference is a list of `{ start, end, speaker }` in seconds, as in eval/asr/corpus.json.

use boardcast_audio::{Ear, Sentence, SAMPLE_RATE};
use boardcast_board::speakers::{identify, Voice};
use serde_json::Value;
use sherpa_onnx::Wave;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

struct Said {
    named: String,
    truth: Option<String>,
}

/// The reference speaker talking most during this span.
fn truth_at(reference: &[Value], start: f64, end: f64) -> Option<String> {
    let mut talking: BTreeMap<&str, f64> = BTreeMap::new();
    for turn in reference {
        let shared = end.min(turn["end"].as_f64()?) - start.max(turn["start"].as_f64()?);
        if shared > 0.0 {
            *talking.entry(turn["speaker"].as_str()?).or_default() += shared;
        }
    }
    talking.into_iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|(name, _)| name.to_string())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [models, recording, reference] = args.as_slice() else { anyhow::bail!("usage: speaker-eval <voice-models> <recording.wav> <reference.json>") };
    let reference: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(reference)?)?;
    let wave = Wave::read(recording).ok_or_else(|| anyhow::anyhow!("cannot read {recording}"))?;
    let mut ear = Ear::load(Path::new(models))?;
    let mut sentences: Vec<Sentence> = vec![];
    let mut heard = |sentence: Sentence| sentences.push(sentence);
    // In small pieces, as a microphone does: the speech detector only holds so much at once.
    for chunk in wave.samples().chunks(320) {
        ear.feed(chunk, &mut heard);
    }
    ear.finish(&mut heard);

    let total = wave.samples().len() as f64 / f64::from(SAMPLE_RATE);
    let mut voices: Vec<Voice> = vec![];
    let mut last = String::new();
    let mut said = vec![];
    for (i, sentence) in sentences.iter().enumerate() {
        let end = sentences.get(i + 1).map_or(total, |next| next.start);
        let named = match &sentence.voiceprint {
            Some(print) => identify(&mut voices, &print.iter().map(|x| f64::from(*x)).collect::<Vec<_>>(), &[]),
            None => last.clone(),
        };
        last.clone_from(&named);
        said.push(Said { named, truth: truth_at(&reference, sentence.start, end) });
    }

    let scored: Vec<&Said> = said.iter().filter(|s| s.truth.is_some() && !s.named.is_empty()).collect();
    let mut pairs: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for s in &scored {
        *pairs.entry((s.named.as_str(), s.truth.as_deref().unwrap_or_default())).or_default() += 1;
    }
    // Each name counts as the person it most often was, and each person as the name they most often got.
    let best = |counts: &BTreeMap<&str, BTreeMap<&str, usize>>| -> usize { counts.values().map(|inner| inner.values().copied().max().unwrap_or(0)).sum() };
    let (mut by_name, mut by_truth): (BTreeMap<&str, BTreeMap<&str, usize>>, BTreeMap<&str, BTreeMap<&str, usize>>) = (BTreeMap::new(), BTreeMap::new());
    for ((name, truth), n) in &pairs {
        by_name.entry(name).or_default().insert(truth, *n);
        by_truth.entry(truth).or_default().insert(name, *n);
    }
    let people: BTreeSet<&str> = reference.iter().filter_map(|t| t["speaker"].as_str()).collect();
    let count = scored.len().max(1) as f64;
    println!("sentences {} (without a voiceprint {})", said.len(), sentences.iter().filter(|s| s.voiceprint.is_none()).count());
    println!("people {} voices found {}", people.len(), voices.len());
    println!("purity {:.3} (a name holds one person)", best(&by_name) as f64 / count);
    println!("coverage {:.3} (a person keeps one name)", best(&by_truth) as f64 / count);
    for ((name, truth), n) in &pairs {
        println!("  {name} = {truth}: {n}");
    }
    Ok(())
}
