//! Make a recorded sample for the web demo from a real meeting: its manual transcript is said line by line
//! to the board model the app uses, and each reply is kept with how long it took and when the line is
//! heard in the recording.
//!
//!   demo-record <root> <sample-id> <out.json> <model> --title <tab> --name <description>
//!               --audio <file in landing/public/media> --credit <who made the recording>
//!
//! The sample is one of the speech evaluation's (eval/asr/meetings.json or corpus.json). Speakers are named
//! A, B, C… in the order they first speak. A turn longer than ten seconds is cut at its sentence ends, as the
//! app would hear it in pieces, each timed by its length. `<model>` is chosen as in every tool
//! (model_flags.rs), and no key is ever written.

use anyhow::{bail, Context, Result};
use boardcast_agent::{propose, CONTEXT_LINES};
use boardcast_board::graph::{parse_ops, Graph, Utterance};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

/// A turn longer than this is cut at its sentence ends.
const LONG: f64 = 10.0;

struct Line {
    speaker: String,
    text: String,
    start: f64,
    end: f64,
}

fn sample(root: &std::path::Path, id: &str) -> Result<Value> {
    for file in ["eval/asr/meetings.json", "eval/asr/corpus.json"] {
        let v: Value = serde_json::from_slice(&std::fs::read(root.join(file))?)?;
        if let Some(s) = v["samples"].as_array().into_iter().flatten().find(|s| s["id"] == id) {
            return Ok(s.clone());
        }
    }
    bail!("no sample {id} in the speech evaluation")
}

/// A turn as the app would hear it: cut after its sentences when it is long, each part timed by its length.
fn cut(speaker: &str, text: &str, start: f64, end: f64) -> Vec<Line> {
    let parts: Vec<String> = text.split_inclusive(['。', '？', '！', '.', '?', '!']).map(|p| p.trim().to_string()).filter(|p| p.chars().any(char::is_alphanumeric)).collect();
    if end - start <= LONG || parts.len() < 2 {
        return vec![Line { speaker: speaker.into(), text: text.trim().into(), start, end }];
    }
    let total: usize = parts.iter().map(|p| p.chars().count()).sum();
    let mut at = start;
    parts
        .into_iter()
        .map(|p| {
            let span = (end - start) * p.chars().count() as f64 / total.max(1) as f64;
            let line = Line { speaker: speaker.into(), text: p, start: at, end: at + span };
            at += span;
            line
        })
        .collect()
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("boardcast root")?);
    let id = args.next().context("sample id")?;
    let out = PathBuf::from(args.next().context("output .json")?);
    let mut model = crate::model_flags::ModelFlags::default();
    let (mut title, mut name, mut audio, mut credit) = (None, None, None, None);
    while let Some(flag) = args.next() {
        if model.take(&flag, &mut args)? {
            continue;
        }
        let mut value = || args.next().with_context(|| format!("a value for {flag}"));
        match flag.as_str() {
            "--title" => title = Some(value()?),
            "--name" => name = Some(value()?),
            "--audio" => audio = Some(value()?),
            "--credit" => credit = Some(value()?),
            other => bail!("unknown flag {other}"),
        }
    }
    let (provider, _) = model.chosen()?.context("a model: --deepseek, --remote <url> --model <name>, --saved or --llm <url>")?;
    let s = sample(&root, &id)?;
    let mut letters: HashMap<String, String> = HashMap::new();
    let mut lines = vec![];
    for r in s["reference"].as_array().context("the sample has no reference")? {
        let who = r["speaker"].as_str().unwrap_or("?").to_string();
        let next = (b'A' + letters.len() as u8) as char;
        let letter = letters.entry(who).or_insert_with(|| next.to_string()).clone();
        lines.extend(cut(&letter, r["text"].as_str().unwrap_or_default(), r["start"].as_f64().unwrap_or(0.0), r["end"].as_f64().unwrap_or(0.0)));
    }
    let guidance = provider.default_guidance();
    let (mut graph, mut history, mut said) = (Graph::default(), Vec::<Utterance>::new(), vec![]);
    for (i, line) in lines.iter().enumerate() {
        let next = Utterance { speaker: Some(line.speaker.clone()), text: line.text.clone(), pointing: vec![] };
        let recent = &history[history.len().saturating_sub(CONTEXT_LINES)..];
        let reply = propose(&provider, &graph, recent, &next, guidance, None).with_context(|| format!("line {}", i + 1))?;
        let ops = reply.ops.trim().to_string();
        let ops = (!ops.is_empty() && ops != "-").then_some(ops);
        if let Some(ops) = &ops {
            graph.apply(&parse_ops(ops));
        }
        eprintln!("{}/{} {} ms {}: {}", i + 1, lines.len(), reply.ms, line.text.chars().take(30).collect::<String>(), ops.as_deref().unwrap_or("-").replace('\n', " | "));
        said.push(json!({ "speaker": line.speaker, "text": line.text, "ms": reply.ms, "ops": ops, "start": (line.start * 1000.0).round() / 1000.0, "end": (line.end * 1000.0).round() / 1000.0 }));
        history.push(next);
    }
    let sample = json!({
        "id": format!("{}-recorded", id), "title": title, "name": name, "lang": s["language"], "audio": audio, "credit": credit, "lines": said,
    });
    let mut text = serde_json::to_string_pretty(&sample)?;
    text.push('\n');
    std::fs::write(&out, text)?;
    println!("{} lines, {} with replies, written to {}", said.len(), said.iter().filter(|l| !l["ops"].is_null()).count(), out.display());
    Ok(())
}
