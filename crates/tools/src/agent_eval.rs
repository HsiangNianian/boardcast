//! Runs eval cases through the board model the app uses, line by line with no window, and scores each final
//! board against its answer. It replaces eval/run.ts, which measured the earlier app's TypeScript engine.
//!
//!   agent-eval <root> <output.json> [--set dev|holdout|pointing|all] [--repeat N] [--label name] [--only id,id]
//!                [<model>] [--skill | --rules-only | --hints-only] [--workers N]
//!
//! The output file keeps each case's final board and what the layout checker found. A Markdown report goes to
//! eval/reports/ and, in GitHub Actions, the job summary. A failed request fails the run, since a case it
//! breaks scores like a bad answer. `<model>` is the same in every tool (model_flags.rs): `--llm <url>`,
//! `--deepseek`, `--remote <url> --model <name>` or `--saved`; without one, the app's llama-server port. A run is
//! named after its model unless `--label` names it; no key is ever written.

use crate::score::{f1, score, Gold};
use anyhow::{bail, Context, Result};
use boardcast_agent::{propose, Guidance, Provider, CONTEXT_LINES};
use boardcast_board::board_layout::StableLayout;
use boardcast_board::check::check_and_repair;
use boardcast_board::glyphs::Glyphs;
use boardcast_board::graph::{parse_ops, Graph, Utterance};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

#[derive(Deserialize)]
struct Case {
    id: String,
    #[serde(default)]
    name: String,
    /// Where the meeting starts, for cases about a board already drawn; otherwise empty.
    #[serde(default)]
    board: Option<Graph>,
    /// Lines the model has already heard.
    #[serde(default)]
    history: Vec<Turn>,
    turns: Vec<Turn>,
    #[serde(default)]
    gold: Gold,
}

#[derive(Deserialize, Clone)]
struct Turn {
    speaker: Option<String>,
    text: String,
    /// What was circled while this was said.
    #[serde(default)]
    pointing: Vec<String>,
}

impl Turn {
    fn utterance(&self) -> Utterance {
        Utterance { speaker: self.speaker.clone(), text: self.text.clone(), pointing: self.pointing.clone() }
    }
}

struct Options {
    guidance: Guidance,
    provider: Provider,
}

const SETS: [(&str, &[&str]); 4] = [
    ("dev", &["cases.json"]),
    ("holdout", &["holdout.json"]),
    ("pointing", &["pointing.json"]),
    ("all", &["cases.json", "holdout.json", "pointing.json"]),
];

const METRICS: [&str; 7] = ["node", "edge", "topic", "item", "strike", "mark", "group"];

/// One case, once: its metrics, what was off, and the model's work.
struct Run {
    metrics: [Option<f64>; 7],
    violations: usize,
    detail: String,
    latencies: Vec<u128>,
    tokens_out: Vec<u64>,
    /// Per line, as llama-server reports them: prompt tokens read after its cache, and milliseconds reading
    /// and writing.
    prompt_read: Vec<u64>,
    prompt_ms: Vec<u64>,
    predicted_ms: Vec<u64>,
    errors: usize,
    row: Value,
}

fn run_case(case: &Case, root: &Path, options: &Options) -> Result<Run> {
    let mut glyphs = Glyphs::from_checkout(root)?;
    let mut graph = case.board.clone().unwrap_or_default();
    let mut board = StableLayout::default();
    let mut history: Vec<Utterance> = case.history.iter().map(Turn::utterance).collect();
    let mut hint: Option<String> = None;
    let (mut hard_steps, mut hints_sent, mut soft_total, mut empty, mut errors) = (0, 0, 0, 0, 0);
    let (mut latencies, mut tokens_out, mut log) = (vec![], vec![], String::new());
    // Input tokens, and how many of them the service had cached, for what a run costs.
    let (mut tokens_in, mut tokens_cached) = (0_u64, 0_u64);
    let (mut prompt_read, mut prompt_ms, mut predicted_ms) = (vec![], vec![], vec![]);
    for turn in &case.turns {
        let next = turn.utterance();
        let recent = &history[history.len().saturating_sub(CONTEXT_LINES)..];
        if options.guidance.hint && hint.is_some() {
            hints_sent += 1;
        }
        match propose(&options.provider, &graph, recent, &next, options.guidance, hint.as_deref()) {
            Ok(reply) => {
                latencies.push(reply.ms);
                tokens_out.push(reply.tokens_out);
                tokens_in += reply.tokens_in;
                tokens_cached += reply.tokens_cached;
                prompt_read.extend(reply.prompt_read);
                prompt_ms.extend(reply.prompt_ms.map(|ms| ms.round() as u64));
                predicted_ms.extend(reply.predicted_ms.map(|ms| ms.round() as u64));
                let ops = reply.ops.trim();
                empty += usize::from(ops.is_empty());
                let _ = writeln!(log, "> {}\n{}\n  ({} ms, {} tokens out)", next.text, if ops.is_empty() { "-" } else { ops }, reply.ms, reply.tokens_out);
                graph.apply(&parse_ops(&reply.ops));
            }
            Err(e) => {
                errors += 1;
                let _ = writeln!(log, "> {}\n! {e:#}", next.text);
            }
        }
        let (found, repaired) = check_and_repair(&graph, &mut board, &mut glyphs);
        hard_steps += usize::from(!found.hard.is_empty());
        let current = repaired.unwrap_or(found);
        soft_total += current.soft.len();
        hint = current.hint();
        history.push(next);
    }
    let (found, repaired) = check_and_repair(&graph, &mut board, &mut glyphs);
    let last = repaired.unwrap_or(found);
    let s = score(&graph, &case.gold);
    let nonempty = |v: Option<usize>| v.is_some_and(|n| n > 0);
    let metrics = [
        nonempty(case.gold.nodes.as_ref().map(Vec::len)).then(|| f1(s.node_p, s.node_r)),
        nonempty(case.gold.edges.as_ref().map(Vec::len)).then(|| f1(s.edge_p, s.edge_r)),
        s.topic_r,
        s.item_p.zip(s.item_r).map(|(p, r)| f1(p, r)),
        s.strike_p.zip(s.strike_r).map(|(p, r)| f1(p, r)),
        s.mark_acc,
        s.group_acc,
    ];
    let mut off: Vec<String> = vec![];
    if !s.missing.is_empty() {
        off.push(format!("missing: {}", s.missing.join(", ")));
    }
    if !s.extra.is_empty() {
        off.push(format!("extra: {}", s.extra.join(", ")));
    }
    off.extend(s.violations.iter().cloned());
    let listed = if off.is_empty() { "- all right".to_string() } else { off.iter().map(|x| format!("- {x}")).collect::<Vec<_>>().join("\n") };
    let detail = format!(
        "### {} · {}\n\n{listed}\n\n<details><summary>Each line's operations and the final board</summary>\n\n```\n{log}\n{}\n```\n</details>\n",
        case.id,
        case.name,
        graph.describe(),
    );
    let row = json!({
        "id": case.id,
        "graph": graph,
        "final_soft": last.soft,
        "final_hard": last.hard,
        "fit_zoom": last.fit_zoom,
        "hard_steps": hard_steps,
        "soft_over_turns": soft_total,
        "hints_sent": hints_sent,
        "model_ms": latencies.iter().sum::<u128>(),
        "turns": case.turns.len(),
        "tokens_in": tokens_in,
        "tokens_cached": tokens_cached,
        "tokens_out": tokens_out.iter().sum::<u64>(),
        "empty_replies": empty,
        "errors": errors,
    });
    Ok(Run { metrics, violations: s.violations.len(), detail, latencies, tokens_out, prompt_read, prompt_ms, predicted_ms, errors, row })
}

fn mean(xs: &[f64]) -> f64 {
    xs.iter().sum::<f64>() / xs.len() as f64
}

fn sd(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let m = mean(xs);
    (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() - 1) as f64).sqrt()
}

fn percentile<T: Copy + Ord>(xs: &[T], p: usize) -> Option<T> {
    let mut s = xs.to_vec();
    s.sort();
    s.get((p * s.len() / 100).min(s.len().saturating_sub(1))).copied()
}

fn cell(v: Option<f64>) -> String {
    v.map_or_else(String::new, |v| format!("{v:.2}"))
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("boardcast root")?);
    let output = args.next().context("output file")?;
    let mut guidance = Guidance::default();
    let mut model = crate::model_flags::ModelFlags::default();
    let (mut set, mut only, mut workers, mut repeat, mut label) = ("all".to_string(), None::<String>, 1, 1, None::<String>);
    while let Some(flag) = args.next() {
        if model.take(&flag, &mut args)? {
            continue;
        }
        match flag.as_str() {
            "--skill" => (guidance.rules, guidance.hint) = (true, true),
            "--rules-only" => guidance.rules = true,
            "--hints-only" => guidance.hint = true,
            "--set" => set = args.next().context("set")?,
            "--only-dev" => set = "dev".into(),
            "--only-holdout" => set = "holdout".into(),
            "--only" => only = args.next(),
            "--workers" => workers = args.next().context("count")?.parse()?,
            "--repeat" => repeat = args.next().context("count")?.parse::<usize>()?.max(1),
            "--label" => label = Some(args.next().context("label")?),
            other => bail!("unknown flag {other}"),
        }
    }
    // A run is named after its model unless the caller names it.
    let (provider, named) = model.or_local()?;
    let label = label.unwrap_or(named);
    let Some((_, files)) = SETS.iter().find(|(name, _)| *name == set) else {
        bail!("--set is one of {}", SETS.map(|(n, _)| n).join(", "));
    };
    let mut cases: Vec<Case> = vec![];
    for file in *files {
        let text = std::fs::read(root.join("eval").join(file))?;
        cases.extend(serde_json::from_slice::<Vec<Case>>(&text).with_context(|| format!("reading eval/{file}"))?);
    }
    cases.retain(|c| only.as_ref().is_none_or(|ids| ids.split(',').any(|id| id == c.id)));
    if cases.is_empty() {
        bail!("no matching eval cases");
    }
    let options = Options { guidance, provider };

    // One line first: a model that cannot be reached would otherwise score every case 0, which reads like a
    // result. It also puts the fixed prompt into the server's cache, so the first case's latency is typical.
    let hello = Utterance { speaker: None, text: "大家好。".into(), pointing: vec![] };
    propose(&options.provider, &Graph::default(), &[], &hello, options.guidance, None).context("the board model did not answer; start llama-server, or pass --llm <url>")?;

    let mut passes: Vec<Vec<Run>> = vec![];
    for pass in 0..repeat {
        let next = AtomicUsize::new(0);
        let results: Mutex<Vec<(usize, Run)>> = Mutex::new(vec![]);
        let failures = Mutex::new(vec![]);
        std::thread::scope(|scope| {
            for _ in 0..workers {
                scope.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(case) = cases.get(i) else { break };
                    match run_case(case, &root, &options) {
                        Ok(run) => {
                            let m = |k: usize| cell(run.metrics[k]);
                            let prefix = if repeat > 1 { format!("[{}/{repeat}] ", pass + 1) } else { String::new() };
                            eprintln!("{prefix}{}: node {} · edge {} · topic {} · item {} · mark {} · violations {}", case.id, m(0), m(1), m(2), m(3), m(5), run.violations);
                            results.lock().expect("results").push((i, run));
                        }
                        Err(e) => failures.lock().expect("failures").push(format!("{}: {e:#}", case.id)),
                    }
                });
            }
        });
        let failures = failures.into_inner().expect("failures");
        if !failures.is_empty() {
            bail!("{} of {} cases could not run:\n{}", failures.len(), cases.len(), failures.join("\n"));
        }
        let mut runs = results.into_inner().expect("results");
        runs.sort_by_key(|(i, _)| *i);
        passes.push(runs.into_iter().map(|(_, r)| r).collect());
    }

    std::fs::write(&output, serde_json::to_vec(&passes[0].iter().map(|r| &r.row).collect::<Vec<_>>())?)?;
    let report = report(&cases, &passes, &label, &set);
    let dir = root.join("eval/reports");
    std::fs::create_dir_all(&dir)?;
    // Seconds since 1970: they sort, and never collide within a run.
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    std::fs::write(dir.join(format!("{label}-{set}-{stamp}.md")), &report)?;
    if let Ok(summary) = std::env::var("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        std::fs::OpenOptions::new().append(true).create(true).open(summary)?.write_all(format!("{report}\n").as_bytes())?;
    }
    println!("{}", report.lines().take(18 + cases.len()).collect::<Vec<_>>().join("\n"));
    let errors: usize = passes.iter().flatten().map(|r| r.errors).sum();
    if errors > 0 {
        bail!("{errors} requests failed, so these scores do not measure the model");
    }
    Ok(())
}

fn report(cases: &[Case], passes: &[Vec<Run>], label: &str, set: &str) -> String {
    let repeat = passes.len();
    // Each pass's mean of a metric over the cases that have it.
    let pass_mean = |pass: &[Run], k: usize| {
        let xs: Vec<f64> = pass.iter().filter_map(|r| r.metrics[k]).collect();
        (!xs.is_empty()).then(|| mean(&xs))
    };
    let overall = |k: usize| {
        let per: Vec<f64> = passes.iter().filter_map(|p| pass_mean(p, k)).collect();
        if per.is_empty() {
            return "-".to_string();
        }
        let counted = passes[0].iter().filter(|r| r.metrics[k].is_some()).count();
        let spread = if repeat > 1 { format!(" ± {:.3}", sd(&per)) } else { String::new() };
        format!("{:.3}{spread} ({counted} cases)", mean(&per))
    };
    // A case's metric across passes: its mean and range.
    let per_case = |i: usize, k: usize| {
        let xs: Vec<f64> = passes.iter().filter_map(|p| p[i].metrics[k]).collect();
        let (lo, hi) = xs.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        (!xs.is_empty()).then(|| (mean(&xs), hi - lo))
    };
    let unstable = |i: usize| (0..METRICS.len()).any(|k| per_case(i, k).is_some_and(|(_, range)| range >= 0.3));
    let latencies: Vec<u128> = passes.iter().flatten().flat_map(|r| r.latencies.iter().copied()).collect();
    let tokens_out: Vec<u64> = passes.iter().flatten().flat_map(|r| r.tokens_out.iter().copied()).collect();
    let all = |pick: fn(&Run) -> &Vec<u64>| passes.iter().flatten().flat_map(|r| pick(r).iter().copied()).collect::<Vec<u64>>();
    let p50 = |xs: Vec<u64>| percentile(&xs, 50).map_or_else(|| "-".into(), |v: u64| v.to_string());
    let errors: usize = passes.iter().flatten().map(|r| r.errors).sum();
    let violations: Vec<f64> = passes.iter().map(|p| p.iter().map(|r| r.violations).sum::<usize>() as f64).collect();
    let shaky: Vec<&str> = (0..cases.len()).filter(|&i| unstable(i)).map(|i| cases[i].id.as_str()).collect();
    let ms = |p| percentile(&latencies, p).map_or_else(|| "-".into(), |v| v.to_string());

    let mut out = String::new();
    let passes_note = if repeat > 1 { format!(" · {repeat} passes") } else { String::new() };
    let _ = writeln!(out, "## Board eval · {label} · {set}{passes_note}\n");
    let _ = writeln!(out, "| Metric | Value{} |\n|---|---|", if repeat > 1 { " (mean ± standard deviation between passes)" } else { "" });
    for (name, k) in [
        ("Node F1", 0),
        ("Edge F1", 1),
        ("Topic recall", 2),
        ("Item F1 (under the right topic)", 3),
        ("Strike F1 (per case, counting strikes by mistake)", 4),
        ("Marks found (strike, emphasis, question, formula)", 5),
    ] {
        let _ = writeln!(out, "| {name} | {} |", overall(k));
    }
    let _ = writeln!(out, "| Failed requests (the scores do not count while this is not zero) | {errors} |");
    let _ = writeln!(out, "| Groups right | {} |", overall(6));
    let _ = writeln!(out, "| Violations (left in, or a correction not made), per pass | {:.1} |", mean(&violations));
    let _ = writeln!(out, "| Latency per line p50 / p95 | {} / {} ms |", ms(50), ms(95));
    let _ = writeln!(out, "| Output tokens per line p50 | {} |", percentile(&tokens_out, 50).map_or_else(|| "-".into(), |v| v.to_string()));
    let _ = writeln!(out, "| Prompt tokens read per line p50 (after the prefix cache) | {} |", p50(all(|r| &r.prompt_read)));
    let _ = writeln!(out, "| Reading / writing per line p50 | {} / {} ms |", p50(all(|r| &r.prompt_ms)), p50(all(|r| &r.predicted_ms)));
    if repeat > 1 {
        let _ = writeln!(out, "| Unstable cases (a score varies by 0.3 or more between passes) | {} |", if shaky.is_empty() { "none".into() } else { shaky.join(", ") });
    }
    let _ = writeln!(out, "\n| Case | Node | Edge | Topic | Item | Strike F1 | Marks | Groups | p50 ms |\n|---|---|---|---|---|---|---|---|---|");
    for (i, c) in cases.iter().enumerate() {
        let cells: Vec<String> = (0..METRICS.len()).map(|k| cell(per_case(i, k).map(|(m, _)| m))).collect();
        let case_ms: Vec<u128> = passes.iter().flat_map(|p| p[i].latencies.iter().copied()).collect();
        let p50 = percentile(&case_ms, 50).map_or_else(String::new, |v| v.to_string());
        let _ = writeln!(out, "| {}{} | {} | {p50} |", c.id, if unstable(i) { " ⚠" } else { "" }, cells.join(" | "));
    }
    out.push('\n');
    for r in &passes[0] {
        out.push_str(&r.detail);
        out.push('\n');
    }
    out
}
