//! Runs every eval case through a board model with no window, so the layout skill can be measured.
//! Writes each case's final board for eval/score.ts, and what the checker found.
//! `--deepseek` reads its key from DEEPSEEK_API_KEY; the key is never written anywhere.

use anyhow::{Context, Result};
use boardcast_agent::{propose, Guidance, Provider};
use boardcast_board::board_layout::StableLayout;
use boardcast_board::check::check_and_repair;
use boardcast_board::glyphs::Glyphs;
use boardcast_board::graph::{parse_ops, Graph, Utterance};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Deserialize)]
struct Case {
    id: String,
    turns: Vec<Turn>,
}

#[derive(Deserialize)]
struct Turn {
    speaker: Option<String>,
    text: String,
}

struct Options {
    guidance: Guidance,
    provider: Provider,
}

fn run_case(case: &Case, root: &Path, options: &Options) -> Result<Value> {
    let mut glyphs = Glyphs::load(root)?;
    let mut graph = Graph::default();
    let mut board = StableLayout::default();
    let mut history: Vec<Utterance> = vec![];
    let mut hint: Option<String> = None;
    let (mut hard_steps, mut hints_sent, mut model_ms, mut soft_total) = (0, 0, 0_u128, 0);
    let (mut tokens_in, mut tokens_out, mut empty) = (0_u64, 0_u64, 0);
    for turn in &case.turns {
        let next = Utterance { speaker: turn.speaker.clone(), text: turn.text.clone() };
        let recent = &history[history.len().saturating_sub(2)..];
        if options.guidance.hint && hint.is_some() {
            hints_sent += 1;
        }
        let reply = propose(&options.provider, &graph, recent, &next, options.guidance, hint.as_deref())?;
        model_ms += reply.ms;
        tokens_in += reply.tokens_in;
        tokens_out += reply.tokens_out;
        let ops = reply.ops;
        empty += usize::from(ops.trim().is_empty());
        graph.apply(&parse_ops(&ops));
        let (found, repaired) = check_and_repair(&graph, &mut board, &mut glyphs);
        hard_steps += usize::from(!found.hard.is_empty());
        let current = repaired.unwrap_or(found);
        soft_total += current.soft.len();
        hint = current.hint();
        history.push(next);
    }
    let (found, repaired) = check_and_repair(&graph, &mut board, &mut glyphs);
    let last = repaired.unwrap_or(found);
    println!("{} soft={} hard_steps={hard_steps} hints={hints_sent} empty={empty} {model_ms}ms", case.id, last.soft.len());
    Ok(json!({
        "id": case.id,
        "graph": graph,
        "final_soft": last.soft,
        "final_hard": last.hard,
        "fit_zoom": last.fit_zoom,
        "hard_steps": hard_steps,
        "soft_over_turns": soft_total,
        "hints_sent": hints_sent,
        "model_ms": model_ms,
        "turns": case.turns.len(),
        "tokens_in": tokens_in,
        "tokens_out": tokens_out,
        "empty_replies": empty,
    }))
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("boardcast root")?);
    let output = args.next().context("output file")?;
    let mut guidance = Guidance::default();
    let mut provider = Provider::Local { base: "http://127.0.0.1:18765".into() };
    let mut set = "all".to_string();
    let mut only: Option<String> = None;
    let mut workers = 1;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--skill" => (guidance.rules, guidance.hint) = (true, true),
            "--rules-only" => guidance.rules = true,
            "--hints-only" => guidance.hint = true,
            "--only-dev" => set = "dev".into(),
            "--only-holdout" => set = "holdout".into(),
            "--only" => only = args.next(),
            "--workers" => workers = args.next().context("count")?.parse()?,
            "--llm" => provider = Provider::Local { base: args.next().context("url")? },
            "--deepseek" => provider = Provider::deepseek_from_env()?,
            other => anyhow::bail!("unknown flag {other}"),
        }
    }
    let options = Options { guidance, provider };
    let mut cases: Vec<Case> = vec![];
    if set != "holdout" {
        cases.extend(serde_json::from_slice::<Vec<Case>>(&std::fs::read(root.join("eval/cases.json"))?)?);
    }
    if set != "dev" {
        cases.extend(serde_json::from_slice::<Vec<Case>>(&std::fs::read(root.join("eval/holdout.json"))?)?);
    }
    cases.retain(|c| only.as_ref().is_none_or(|id| &c.id == id));
    let next = AtomicUsize::new(0);
    let results = Mutex::new(vec![]);
    let failures = Mutex::new(vec![]);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(case) = cases.get(next.fetch_add(1, Ordering::Relaxed)) {
                    match run_case(case, &root, &options) {
                        Ok(row) => results.lock().expect("results").push(row),
                        Err(e) => failures.lock().expect("failures").push(format!("{}: {e:#}", case.id)),
                    }
                }
            });
        }
    });
    let failures = failures.into_inner().expect("failures");
    let results = results.into_inner().expect("results");
    std::fs::write(&output, serde_json::to_vec(&results)?)?;
    if !failures.is_empty() {
        anyhow::bail!("{} of {} cases failed:\n{}", failures.len(), cases.len(), failures.join("\n"));
    }
    Ok(())
}
