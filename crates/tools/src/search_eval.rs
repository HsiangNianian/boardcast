//! How well meeting search finds the passage a question is about.
//!
//!   search-eval <root> <model.gguf> <gemma|qwen3> [--out <report.json>]
//!
//! The meetings are the speech evaluation's manual transcripts (eval/asr/meetings.json and corpus.json); the
//! questions and the words a right answer contains are in eval/search/queries.json. Every passage of every
//! meeting is ranked for each question, as the app searches all meetings at once. The passages are
//! embedded once, then each vector size and each weight for shared words is scored on the same vectors.

use anyhow::{bail, Context, Result};
use boardcast_search::chunk::{transcript, Said};
use boardcast_search::embed::cut;
use boardcast_search::{rank, Embedder, Family, Index};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const PORT: u16 = 18631;
const DIMS: [usize; 4] = [0, 512, 256, 128];
const WEIGHTS: [f32; 4] = [0.0, 0.1, 0.2, 0.3];

/// A llama-server serving embeddings on the CPU, stopped when dropped.
struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn serve(root: &Path, model: &Path) -> Result<Server> {
    let exe = root.join("resources/llama").join(if cfg!(windows) { "llama-server.exe" } else { "llama-server" });
    let child = Command::new(&exe)
        .arg("-m")
        .arg(model)
        .args(["--embeddings", "--host", "127.0.0.1", "--port", &PORT.to_string()])
        .args(["-c", "8192", "-np", "4", "-b", "2048", "-ub", "2048", "--device", "none", "-ngl", "0"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("starting {}", exe.display()))?;
    let server = Server(child);
    let health = format!("http://127.0.0.1:{PORT}/health");
    for _ in 0..240 {
        if ureq::get(&health).call().is_ok_and(|r| r.status() == 200) {
            return Ok(server);
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    bail!("the embedding server did not come up")
}

/// A meeting's transcript lines: a list of `{speaker, text}`, or one long text cut at its sentences.
fn lines_of(sample: &Value) -> Vec<(Option<String>, String)> {
    let refs = sample["reference"].as_array().cloned().unwrap_or_default();
    let mut out = vec![];
    for r in refs {
        let speaker = r["speaker"].as_str().map(str::to_string);
        let text = r["text"].as_str().or(r.as_str()).unwrap_or_default();
        if text.chars().count() > 600 {
            for s in text.split_inclusive('。').filter(|s| !s.trim().is_empty()) {
                out.push((speaker.clone(), s.trim().to_string()));
            }
        } else {
            out.push((speaker, text.to_string()));
        }
    }
    out
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("boardcast root")?);
    let model = PathBuf::from(args.next().context("model .gguf")?);
    let family = Family::parse(&args.next().context("gemma or qwen3")?).context("the family is gemma or qwen3")?;
    let mut out = None;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--out" => out = Some(PathBuf::from(args.next().context("report path")?)),
            other => bail!("unknown flag {other}"),
        }
    }
    let queries: Vec<Value> = serde_json::from_slice(&std::fs::read(root.join("eval/search/queries.json"))?)?;
    let mut samples: BTreeMap<String, Value> = BTreeMap::new();
    for file in ["eval/asr/meetings.json", "eval/asr/corpus.json"] {
        let v: Value = serde_json::from_slice(&std::fs::read(root.join(file))?)?;
        for s in v["samples"].as_array().into_iter().flatten() {
            samples.insert(s["id"].as_str().unwrap_or_default().to_string(), s.clone());
        }
    }
    let meetings: Vec<String> = {
        let mut ids: Vec<String> = queries.iter().filter_map(|q| q["meeting"].as_str().map(str::to_string)).collect();
        ids.sort();
        ids.dedup();
        ids
    };

    let _server = serve(&root, &model)?;
    let embedder = Embedder::new(&format!("http://127.0.0.1:{PORT}"), family, usize::MAX);

    // Every meeting's passages, embedded at full size.
    let started = Instant::now();
    let mut passages = vec![];
    let mut characters = 0;
    for id in &meetings {
        let sample = samples.get(id).with_context(|| format!("no meeting {id}"))?;
        let lines = lines_of(sample);
        let said: Vec<Said> = lines.iter().map(|(s, t)| Said { speaker: s.as_deref(), text: t }).collect();
        let chunks = transcript(&said);
        let title = sample["title"].as_str().unwrap_or_default();
        let texts: Vec<String> = chunks.iter().map(|c| family.passage(title, &c.text)).collect();
        characters += chunks.iter().map(|c| c.text.chars().count()).sum::<usize>();
        let vectors = embedder.embed(&texts)?;
        passages.push((id.clone(), chunks, vectors));
    }
    let index_secs = started.elapsed().as_secs_f64();
    let count: usize = passages.iter().map(|p| p.1.len()).sum();
    let started = Instant::now();
    let asked: Vec<String> = queries.iter().map(|q| family.query(q["q"].as_str().unwrap_or_default())).collect();
    let query_vectors = embedder.embed(&asked)?;
    let query_ms = started.elapsed().as_secs_f64() * 1000.0 / queries.len().max(1) as f64;
    let full = query_vectors.first().map_or(0, Vec::len);
    println!("{} meetings, {count} passages, {characters} characters embedded in {index_secs:.1} s ({:.0} characters/s); {query_ms:.0} ms a query; {full} dimensions", meetings.len(), characters as f64 / index_secs);

    let mut results = vec![];
    println!("{:>5} {:>6} {:>6} {:>6} {:>6}   paraphrase  keyword  cross   (R@5)", "dims", "weight", "R@1", "R@5", "MRR10");
    for dims in DIMS.map(|d| if d == 0 { full } else { d }) {
        let indexes: Vec<(String, Index)> = passages.iter().map(|(id, chunks, vectors)| (id.clone(), Index::new(String::new(), dims, chunks.clone(), vectors.iter().map(|v| cut(v, dims)).collect()))).collect();
        for weight in WEIGHTS {
            let (mut r1, mut r5, mut mrr) = (0.0, 0.0, 0.0);
            let mut by_kind: BTreeMap<String, (f64, f64)> = BTreeMap::new();
            let mut misses = vec![];
            for (q, qv) in queries.iter().zip(&query_vectors) {
                let hits = rank(&indexes, &cut(qv, dims), q["q"].as_str().unwrap_or_default(), weight, 10);
                let gold: Vec<&str> = q["gold"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                let at = hits.iter().position(|h| h.meeting == q["meeting"].as_str().unwrap_or_default() && gold.iter().any(|g| h.chunk.text.contains(g)));
                let kind = q["kind"].as_str().unwrap_or("other").to_string();
                let entry = by_kind.entry(kind).or_default();
                entry.1 += 1.0;
                if at == Some(0) {
                    r1 += 1.0;
                }
                if at.is_some_and(|i| i < 5) {
                    r5 += 1.0;
                    entry.0 += 1.0;
                } else {
                    misses.push(q["id"].as_str().unwrap_or_default().to_string());
                }
                if let Some(i) = at {
                    mrr += 1.0 / (i as f64 + 1.0);
                }
            }
            let n = queries.len() as f64;
            let kind = |k: &str| by_kind.get(k).map_or(0.0, |(hit, all)| hit / all.max(1.0));
            println!("{dims:>5} {weight:>6.1} {:>6.3} {:>6.3} {:>6.3}   {:>10.3} {:>8.3} {:>6.3}", r1 / n, r5 / n, mrr / n, kind("paraphrase"), kind("keyword"), kind("cross"));
            results.push(json!({ "dims": dims, "weight": weight, "recall1": r1 / n, "recall5": r5 / n, "mrr10": mrr / n,
                "recall5_by_kind": by_kind.iter().map(|(k, (h, a))| (k.clone(), json!(h / a))).collect::<serde_json::Map<_, _>>(), "misses_at5": misses }));
        }
    }
    // Where to stop showing hits: the scores of right answers against the best score from meetings that
    // hold no answer, at the size and weight the app uses.
    {
        let dims = boardcast_search::DIMS;
        let indexes: Vec<(String, Index)> = passages.iter().map(|(id, chunks, vectors)| (id.clone(), Index::new(String::new(), dims, chunks.clone(), vectors.iter().map(|v| cut(v, dims)).collect()))).collect();
        let (mut right, mut elsewhere) = (vec![], vec![]);
        // Each query's best score, with its right answer's and the best unrelated one's.
        let mut trios: Vec<(f32, Option<f32>, Option<f32>)> = vec![];
        for (q, qv) in queries.iter().zip(&query_vectors) {
            let meeting = q["meeting"].as_str().unwrap_or_default();
            let gold: Vec<&str> = q["gold"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            let hits = rank(&indexes, &cut(qv, dims), q["q"].as_str().unwrap_or_default(), boardcast_search::WORD_WEIGHT, usize::MAX);
            if let Some(h) = hits.iter().find(|h| h.meeting == meeting && gold.iter().any(|g| h.chunk.text.contains(g))) {
                right.push(h.score);
            }
            if let Some(h) = hits.iter().find(|h| h.meeting != meeting) {
                elsewhere.push(h.score);
            }
            let r = hits.iter().find(|h| h.meeting == meeting && gold.iter().any(|g| h.chunk.text.contains(g))).map(|h| h.score);
            let u = hits.iter().find(|h| h.meeting != meeting).map(|h| h.score);
            trios.push((hits.first().map_or(0.0, |h| h.score), r, u));
        }
        for floor in [0.28f32, 0.30, 0.32] {
            for gap in [0.06f32, 0.08, 0.10, 0.12, 0.15] {
                let shown = |s: f32, best: f32| s >= floor && s >= best - gap;
                let kept = trios.iter().filter(|(b, r, _)| r.is_some_and(|r| shown(r, *b))).count() as f64 / trios.iter().filter(|t| t.1.is_some()).count() as f64;
                let leaked = trios.iter().filter(|(b, _, u)| u.is_some_and(|u| shown(u, *b))).count() as f64 / trios.len() as f64;
                println!("  floor {floor:.2} and within {gap:.2} of the best: keeps {:.0}% of right answers, shows the best unrelated for {:.0}% of queries", kept * 100.0, leaked * 100.0);
            }
        }
        let at = |v: &mut Vec<f32>, p: f64| {
            v.sort_by(f32::total_cmp);
            v[((v.len() - 1) as f64 * p).round() as usize]
        };
        println!(
            "scores at {dims}: right answers p05 {:.3} p10 {:.3} p25 {:.3} p50 {:.3}; best from other meetings p50 {:.3} p75 {:.3} p90 {:.3} p95 {:.3} max {:.3}",
            at(&mut right, 0.05), at(&mut right, 0.10), at(&mut right, 0.25), at(&mut right, 0.5),
            at(&mut elsewhere, 0.5), at(&mut elsewhere, 0.75), at(&mut elsewhere, 0.9), at(&mut elsewhere, 0.95), at(&mut elsewhere, 1.0)
        );
        for cut_at in [0.30f32, 0.35, 0.40, 0.45, 0.50] {
            let kept = right.iter().filter(|s| **s >= cut_at).count() as f64 / right.len() as f64;
            let shown = elsewhere.iter().filter(|s| **s >= cut_at).count() as f64 / elsewhere.len() as f64;
            println!("  threshold {cut_at:.2}: keeps {:.0}% of right answers, lets through {:.0}% of best unrelated", kept * 100.0, shown * 100.0);
        }
    }
    if let Some(out) = out {
        let report = json!({ "model": model.file_name().map(|f| f.to_string_lossy().to_string()), "family": format!("{family:?}"), "meetings": meetings, "passages": count,
            "characters": characters, "index_seconds": index_secs, "query_ms": query_ms, "dimensions": full, "results": results });
        std::fs::write(&out, serde_json::to_string_pretty(&report)?)?;
    }
    Ok(())
}
