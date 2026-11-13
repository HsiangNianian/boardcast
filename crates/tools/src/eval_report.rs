//! A board evaluation as a page to read or print: runs side by side, what they cost, how the scores moved over
//! time, and every case. It reads the summaries `agent-eval` writes next to its reports.
//!
//!   eval-report <root> <out.html> [--history <dir>] [--price <label>=<hit>,<miss>,<out>]... <summary.json>...
//!
//! Summaries with the same label are one run, so a model's dev and pointing runs read as one. `--price` gives a
//! run's price per million tokens (input from cache, input not cached, output, in USD) to show its cost.
//! `--history` is a folder of earlier reports, such as CI's eval artifacts: the dev runs in it, one a day, draw
//! the trend, whether they are summaries or the Markdown reports from before summaries existed. Chrome prints
//! the page: `google-chrome --headless --print-to-pdf=report.pdf report.html`.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const METRICS: [(&str, &str); 7] = [("node", "Node F1"), ("edge", "Edge F1"), ("topic", "Topic recall"), ("item", "Item F1"), ("strike", "Strike F1"), ("mark", "Marks found"), ("group", "Groups right")];
/// The metrics a case table and the trend show.
const SHOWN: [(&str, &str); 4] = [("node", "node"), ("edge", "edge"), ("item", "item"), ("mark", "marks")];
/// Categorical colours, in a fixed order, checked for colour-blind separation as a set of three.
const SERIES: [&str; 6] = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#6250d6"];
/// A sequential blue ramp, light to dark, for a case's score.
const RAMP: [&str; 13] = ["#cde2fb", "#b7d3f6", "#9ec5f4", "#86b6ef", "#6da7ec", "#5598e7", "#3987e5", "#2a78d6", "#256abf", "#1c5cab", "#184f95", "#104281", "#0d366b"];
/// A busy meeting: one finished line every six seconds.
const LINES_PER_HOUR: f64 = 600.0;

/// One model's run: one or more summaries with the same label.
struct Run {
    label: String,
    passes: u64,
    cases: BTreeMap<String, Value>,
    /// The case order as the summaries list them.
    order: Vec<String>,
    p50: Option<f64>,
    p95: Option<f64>,
    failed: u64,
    tokens: (u64, u64, u64),
    lines: u64,
    commit: Option<String>,
    stamp: u64,
}

impl Run {
    /// A metric's mean over the cases that measure it.
    fn mean(&self, metric: &str) -> Option<(f64, usize)> {
        let xs: Vec<f64> = self.cases.values().filter_map(|c| c["scores"][metric].as_f64()).collect();
        (!xs.is_empty()).then(|| (xs.iter().sum::<f64>() / xs.len() as f64, xs.len()))
    }
}

/// A day's dev scores for the trend.
struct Point {
    day: String,
    model: String,
    scores: BTreeMap<&'static str, f64>,
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("boardcast root")?);
    let out = PathBuf::from(args.next().context("output .html")?);
    let (mut history, mut prices, mut files) = (None::<PathBuf>, BTreeMap::<String, (f64, f64, f64)>::new(), vec![]);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--history" => history = Some(PathBuf::from(args.next().context("a folder for --history")?)),
            "--price" => {
                let spec = args.next().context("--price <label>=<hit>,<miss>,<out>")?;
                let (label, p) = spec.split_once('=').context("--price <label>=<hit>,<miss>,<out>")?;
                let p: Vec<f64> = p.split(',').map(str::parse).collect::<Result<_, _>>().context("prices are numbers")?;
                let [hit, miss, out] = p[..] else { bail!("--price takes three prices: cache hit, cache miss, output") };
                prices.insert(label.to_string(), (hit, miss, out));
            }
            other if other.starts_with("--") => bail!("unknown flag {other}"),
            file => files.push(PathBuf::from(file)),
        }
    }
    if files.is_empty() {
        bail!("no summaries: pass the .json files agent-eval wrote");
    }
    let runs = load(&files)?;
    let trend = history.as_deref().map(|dir| trend(dir, &runs)).transpose()?.unwrap_or_default();
    std::fs::write(&out, page(&root, &runs, &prices, &trend))?;
    println!("{}", out.display());
    Ok(())
}

fn load(files: &[PathBuf]) -> Result<Vec<Run>> {
    let mut runs: Vec<Run> = vec![];
    for f in files {
        let v: Value = serde_json::from_slice(&std::fs::read(f).with_context(|| format!("reading {}", f.display()))?)?;
        let label = v["label"].as_str().context("a summary names its label")?.to_string();
        let at = match runs.iter().position(|r| r.label == label) {
            Some(i) => i,
            None => {
                runs.push(Run { label, passes: 0, cases: BTreeMap::new(), order: vec![], p50: None, p95: None, failed: 0, tokens: (0, 0, 0), lines: 0, commit: None, stamp: 0 });
                runs.len() - 1
            }
        };
        let r = &mut runs[at];
        let lines = v["lines"].as_u64().unwrap_or(0);
        // The latency of the larger part stands for the run.
        if lines >= r.lines {
            r.p50 = v["latency_ms"]["p50"].as_f64();
            r.p95 = v["latency_ms"]["p95"].as_f64();
        }
        r.passes = r.passes.max(v["passes"].as_u64().unwrap_or(1));
        r.failed += v["failed_requests"].as_u64().unwrap_or(0);
        r.tokens.0 += v["tokens"]["in"].as_u64().unwrap_or(0);
        r.tokens.1 += v["tokens"]["cached"].as_u64().unwrap_or(0);
        r.tokens.2 += v["tokens"]["out"].as_u64().unwrap_or(0);
        r.lines += lines;
        r.commit = r.commit.take().or_else(|| v["commit"].as_str().map(str::to_string));
        r.stamp = r.stamp.max(v["stamp"].as_u64().unwrap_or(0));
        for c in v["cases"].as_array().into_iter().flatten() {
            let id = c["id"].as_str().unwrap_or_default().to_string();
            if !r.cases.contains_key(&id) {
                r.order.push(id.clone());
            }
            r.cases.insert(id, c.clone());
        }
    }
    Ok(runs)
}

/// Days since 1970 as a date, without a calendar library (Howard Hinnant's civil_from_days).
fn day_of(stamp: u64) -> String {
    let z = (stamp / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{m:02}-{d:02}", yoe + era * 400 + i64::from(m <= 2))
}

/// The dev runs in a folder of earlier reports, the last of each day, then today's runs.
fn trend(dir: &Path, runs: &[Run]) -> Result<Vec<Point>> {
    let mut found: Vec<(String, u64, Point)> = vec![];
    let mut walk = vec![dir.to_path_buf()];
    while let Some(d) = walk.pop() {
        for entry in std::fs::read_dir(&d).with_context(|| format!("reading {}", d.display()))? {
            let path = entry?.path();
            if path.is_dir() {
                walk.push(path);
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            if name.contains("pointing") || name.contains("revisions") || name.contains("holdout") {
                continue;
            }
            if let Some(found_one) = read_point(&path, &name) {
                found.push(found_one);
            }
        }
    }
    // The last run of each day.
    found.sort_by_key(|(_, stamp, _)| *stamp);
    let mut by_day: BTreeMap<String, Point> = BTreeMap::new();
    for (_, _, p) in found {
        by_day.insert(p.day.clone(), p);
    }
    for r in runs {
        let dev: Vec<&Value> = r.cases.values().filter(|c| c["set"] == "dev").collect();
        if dev.is_empty() || r.stamp == 0 {
            continue;
        }
        let mut scores = BTreeMap::new();
        for (key, _) in SHOWN {
            let xs: Vec<f64> = dev.iter().filter_map(|c| c["scores"][key].as_f64()).collect();
            if !xs.is_empty() {
                scores.insert(key, xs.iter().sum::<f64>() / xs.len() as f64);
            }
        }
        // Today's point for the model CI tracks; another model's would read as a jump in its line.
        if by_day.values().any(|p| p.model == model_of(&r.label)) || by_day.is_empty() {
            by_day.insert(day_of(r.stamp), Point { day: day_of(r.stamp), model: model_of(&r.label), scores });
        }
    }
    Ok(by_day.into_values().collect())
}

/// A label without where it ran: qwen3.5-4b-q4km-cpu reads as qwen3.5-4b.
fn model_of(label: &str) -> String {
    let parts: Vec<&str> = label.split('-').collect();
    if parts.len() >= 2 { format!("{}-{}", parts[0], parts[1]) } else { label.to_string() }
}

/// A dev run's scores from a summary, or from a Markdown report's headline, in English or the earlier Chinese.
fn read_point(path: &Path, name: &str) -> Option<(String, u64, Point)> {
    let text = std::fs::read_to_string(path).ok()?;
    let (label, stamp, scores) = if name.ends_with(".json") {
        let v: Value = serde_json::from_str(&text).ok()?;
        if v["set"] != "dev" {
            return None;
        }
        let mut scores = BTreeMap::new();
        for (key, _) in SHOWN {
            let per: Vec<f64> = v["metrics"][key]["passes"].as_array()?.iter().filter_map(Value::as_f64).collect();
            if !per.is_empty() {
                scores.insert(key, per.iter().sum::<f64>() / per.len() as f64);
            }
        }
        (v["label"].as_str()?.to_string(), v["stamp"].as_u64()?, scores)
    } else if name.ends_with(".md") {
        let stamp = stamp_of(name)?;
        let label = name.split("-dev-").next().filter(|l| *l != name).map(str::to_string).or_else(|| name.find("-202").map(|at| name[..at].to_string()))?;
        let mut scores = BTreeMap::new();
        for line in text.lines().take(30) {
            let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
            let [title, value] = cells[..] else { continue };
            let key = match title {
                t if t.starts_with("Node F1") || t.starts_with("节点 F1") => "node",
                t if t.starts_with("Edge F1") || t.starts_with("连线 F1") => "edge",
                t if t.starts_with("Item F1") || t.starts_with("要点 F1") => "item",
                t if t.starts_with("Marks found") || t.starts_with("标记命中") => "mark",
                _ => continue,
            };
            if let Some(v) = value.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()) {
                scores.insert(key, v);
            }
        }
        (label, stamp, scores)
    } else {
        return None;
    };
    if scores.is_empty() {
        return None;
    }
    Some((label.clone(), stamp, Point { day: day_of(stamp), model: model_of(&label), scores }))
}

/// When a report was written, from its name: seconds since 1970, or the earlier 2026-10-30T11-47-56-439Z.
fn stamp_of(name: &str) -> Option<u64> {
    let stem = name.trim_end_matches(".md");
    if let Some(n) = stem.rsplit('-').next().and_then(|n| n.parse::<u64>().ok()).filter(|n| *n > 1_000_000_000) {
        return Some(n);
    }
    let at = stem.find("-20")? + 1;
    let date = &stem[at..at + 10];
    let (y, m, d): (i64, i64, i64) = (date[..4].parse().ok()?, date[5..7].parse().ok()?, date[8..10].parse().ok()?);
    // Days from a civil date (the inverse of day_of), at noon.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(((era * 146_097 + doe - 719_468) * 86_400 + 43_200) as u64)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn fmt(v: Option<f64>) -> String {
    v.map_or_else(|| "–".into(), |v| format!("{v:.2}"))
}

fn colour(i: usize) -> &'static str {
    SERIES[i % SERIES.len()]
}

/// Each metric a row, one dot per run; the lowest labelled, as the gap worth reading.
fn dot_plot(runs: &[Run]) -> String {
    let (w, row_h, left, right, top) = (640.0, 34.0, 120.0, 24.0, 30.0);
    let (lo, hi) = (0.5, 1.0);
    let h = top + row_h * METRICS.len() as f64 + 26.0;
    let x = |v: f64| left + (v.max(lo) - lo) / (hi - lo) * (w - left - right);
    let mut s = format!(r#"<svg viewBox="0 0 {w} {h}" width="100%" role="img" aria-label="Scores by metric and run">"#);
    for t in [0.5, 0.6, 0.7, 0.8, 0.9, 1.0] {
        let _ = write!(s, r#"<line x1="{0:.1}" x2="{0:.1}" y1="{1}" y2="{2}" class="grid"/><text x="{0:.1}" y="{3}" class="tick" text-anchor="middle">{t:.1}</text>"#, x(t), top - 8.0, h - 22.0, h - 6.0);
    }
    for (i, (key, name)) in METRICS.iter().enumerate() {
        let y = top + i as f64 * row_h + row_h / 2.0;
        let _ = write!(s, r#"<text x="{}" y="{}" class="lab" text-anchor="end">{name}</text>"#, left - 12.0, y + 4.0);
        let vals: Vec<(usize, f64)> = runs.iter().enumerate().filter_map(|(j, r)| r.mean(key).map(|(v, _)| (j, v))).collect();
        if vals.len() > 1 {
            let xs: Vec<f64> = vals.iter().map(|(_, v)| x(*v)).collect();
            let _ = write!(s, r#"<line x1="{:.1}" x2="{:.1}" y1="{y}" y2="{y}" class="range"/>"#, xs.iter().cloned().fold(f64::MAX, f64::min), xs.iter().cloned().fold(f64::MIN, f64::max));
        }
        for (j, v) in &vals {
            let _ = write!(s, r##"<circle cx="{:.1}" cy="{y}" r="6" fill="{}" stroke="#fff" stroke-width="2"><title>{} · {name}: {v:.3}</title></circle>"##, x(*v), colour(*j), esc(&runs[*j].label));
        }
        if let Some((j, v)) = vals.iter().cloned().min_by(|a, b| a.1.total_cmp(&b.1)).filter(|(_, v)| *v < 0.98) {
            let _ = write!(s, r#"<text x="{:.1}" y="{}" class="val" text-anchor="end">{} {v:.2}</text>"#, x(v) - 10.0, y - 9.0, esc(&runs[j].label));
        }
    }
    let _ = write!(s, r#"<text x="{}" y="{}" class="tick">scores below 0.5 are drawn at 0.5</text></svg>"#, x(0.5), top - 14.0);
    s
}

fn latency_bars(runs: &[Run]) -> String {
    let timed: Vec<(usize, f64, f64)> = runs.iter().enumerate().filter_map(|(i, r)| Some((i, r.p50?, r.p95?))).collect();
    let Some(most) = timed.iter().map(|t| t.2).reduce(f64::max).filter(|m| *m > 0.0) else { return String::new() };
    let (w, row_h, left, right) = (640.0, 30.0, 160.0, 110.0);
    let h = row_h * timed.len() as f64 + 30.0;
    let x = |v: f64| left + v / most * (w - left - right);
    let mut s = format!(r#"<svg viewBox="0 0 {w} {h}" width="100%" role="img" aria-label="Latency per line">"#);
    for (row, (i, p50, p95)) in timed.iter().enumerate() {
        let y = 8.0 + row as f64 * row_h;
        let _ = write!(s, r#"<text x="{}" y="{}" class="lab" text-anchor="end">{}</text>"#, left - 12.0, y + 15.0, esc(&runs[*i].label));
        let _ = write!(s, r#"<rect x="{left}" y="{}" width="{:.1}" height="16" rx="3" fill="{}" opacity=".28"/><rect x="{left}" y="{}" width="{:.1}" height="16" rx="3" fill="{}"/>"#, y + 4.0, x(*p95) - left, colour(*i), y + 4.0, x(*p50) - left, colour(*i));
        let _ = write!(s, r#"<text x="{:.1}" y="{}" class="val">{:.2} s / {:.2} s</text>"#, x(*p95) + 8.0, y + 16.0, p50 / 1000.0, p95 / 1000.0);
    }
    let _ = write!(s, r#"<text x="{left}" y="{}" class="tick">solid: median line · pale: 95th percentile</text></svg>"#, h - 4.0);
    s
}

fn trend_panel(points: &[Point], key: &str, title: &str, models: &[String]) -> String {
    let (w, h, left, right, top, bottom) = (300.0, 170.0, 34.0, 12.0, 22.0, 26.0);
    let n = points.len();
    let x = |i: usize| left + i as f64 / (n.max(2) - 1) as f64 * (w - left - right);
    let y = |v: f64| top + (1.0 - v) * (h - top - bottom);
    let mut s = format!(r#"<svg viewBox="0 0 {w} {h}" width="100%" role="img" aria-label="{title} over time"><text x="{left}" y="13" class="lab strong">{title}</text>"#);
    for t in [0.0, 0.5, 1.0] {
        let _ = write!(s, r#"<line x1="{left}" x2="{}" y1="{:.1}" y2="{:.1}" class="grid"/><text x="{}" y="{:.1}" class="tick" text-anchor="end">{t}</text>"#, w - right, y(t), y(t), left - 6.0, y(t) + 3.0);
    }
    for (m, model) in models.iter().enumerate() {
        let pts: Vec<(usize, f64)> = points.iter().enumerate().filter(|(_, p)| &p.model == model).filter_map(|(i, p)| p.scores.get(key).map(|v| (i, *v))).collect();
        if pts.is_empty() {
            continue;
        }
        let line: Vec<String> = pts.iter().map(|(i, v)| format!("{:.1},{:.1}", x(*i), y(*v))).collect();
        let _ = write!(s, r#"<polyline fill="none" stroke="{}" stroke-width="2" points="{}"/>"#, colour(m), line.join(" "));
        for (i, v) in &pts {
            let _ = write!(s, r##"<circle cx="{:.1}" cy="{:.1}" r="4" fill="{}" stroke="#fff" stroke-width="2"><title>{} · {model} · {v:.3}</title></circle>"##, x(*i), y(*v), colour(m), points[*i].day);
        }
        let (i, v) = pts[pts.len() - 1];
        let _ = write!(s, r#"<text x="{:.1}" y="{:.1}" class="val" text-anchor="end">{v:.2}</text>"#, x(i), y(v) - 8.0);
    }
    if let (Some(first), Some(last)) = (points.first(), points.last()) {
        let _ = write!(s, r#"<text x="{}" y="{}" class="tick">{}</text><text x="{}" y="{}" class="tick" text-anchor="end">{}</text>"#, x(0), h - 8.0, &first.day[5..], x(n - 1), h - 8.0, &last.day[5..]);
    }
    s.push_str("</svg>");
    s
}

fn heat(v: Option<f64>) -> String {
    match v {
        None => r#"<td class="na"></td>"#.into(),
        Some(v) => {
            let step = RAMP[((v * (RAMP.len() - 1) as f64).round() as usize).min(RAMP.len() - 1)];
            let ink = if v >= 0.55 { "#fff" } else { "#0b0b0b" };
            format!(r#"<td class="hm" style="background:{step};color:{ink}">{v:.2}</td>"#)
        }
    }
}

fn page(root: &Path, runs: &[Run], prices: &BTreeMap<String, (f64, f64, f64)>, trend: &[Point]) -> String {
    let fonts = root.join("landing/fonts").canonicalize().unwrap_or_else(|_| root.join("landing/fonts"));
    let fonts = fonts.display();
    let stamp = runs.iter().map(|r| r.stamp).max().unwrap_or(0);
    let commit = runs.iter().find_map(|r| r.commit.clone()).map_or_else(String::new, |c| format!(" · {}", &c[..c.len().min(7)]));
    let lines = runs.iter().map(|r| r.lines).max().unwrap_or(0);
    let legend: String = runs.iter().enumerate().map(|(i, r)| format!(r#"<span><i class="sw" style="background:{}"></i>{}</span>"#, colour(i), esc(&r.label))).collect();

    let mut summary = String::new();
    for (i, r) in runs.iter().enumerate() {
        let _ = write!(summary, r#"<tr><td><i class="sw" style="background:{}"></i>{}</td>"#, colour(i), esc(&r.label));
        for (key, _) in METRICS {
            let _ = write!(summary, r#"<td class="num">{}</td>"#, fmt(r.mean(key).map(|m| m.0)));
        }
        let latency = r.p50.zip(r.p95).map_or_else(|| "–".into(), |(a, b)| format!("{:.2} / {:.2}", a / 1000.0, b / 1000.0));
        let _ = write!(summary, r#"<td class="num">{latency}</td><td class="num">{}</td><td class="num">{}</td></tr>"#, r.passes, r.failed);
    }
    let counts: String = METRICS.iter().map(|(key, _)| format!(r#"<td class="num muted">{}</td>"#, runs.iter().filter_map(|r| r.mean(key)).map(|m| m.1).max().unwrap_or(0))).collect();
    let failed = runs.iter().filter(|r| r.failed > 0).map(|r| esc(&r.label)).collect::<Vec<_>>();
    let failed_note = if failed.is_empty() { String::new() } else { format!(r#"<p class="note"><b>Failed requests in {}</b>: those lines count as unanswered, so the scores understate the model.</p>"#, failed.join(", ")) };

    let mut cost = String::new();
    for (i, r) in runs.iter().enumerate() {
        let Some((hit, miss, out)) = prices.get(&r.label) else { continue };
        let (tin, tcached, tout) = r.tokens;
        if tin == 0 && tout == 0 {
            continue;
        }
        let dollars = (tcached as f64 * hit + (tin - tcached.min(tin)) as f64 * miss + tout as f64 * out) / 1e6;
        let per_line = dollars / r.lines.max(1) as f64;
        let _ = write!(cost, r#"<tr><td><i class="sw" style="background:{}"></i>{}</td><td class="num">{tin}</td><td class="num">{tcached} ({:.0}%)</td><td class="num">{tout}</td><td class="num">${dollars:.4}</td><td class="num">${per_line:.6}</td><td class="num">${:.3}</td></tr>"#, colour(i), esc(&r.label), tcached as f64 * 100.0 / tin.max(1) as f64, per_line * LINES_PER_HOUR);
    }
    let cost_section = if cost.is_empty() {
        String::new()
    } else {
        let given: Vec<String> = prices.iter().map(|(l, (h, m, o))| format!("{}: cache hit ${h}, cache miss ${m}, output ${o}", esc(l))).collect();
        format!(r#"<h2>Cost</h2><table><thead><tr><th>Run</th><th class="num">Input tokens</th><th class="num">of which cached</th><th class="num">Output tokens</th><th class="num">One pass</th><th class="num">Per line</th><th class="num">Hour of meeting</th></tr></thead><tbody>{cost}</tbody></table><p class="note">Prices per 1M tokens, as given to the report: {}. An hour of meeting is {LINES_PER_HOUR} finished lines. Local models cost no API charge and are left out.</p>"#, given.join("; "))
    };

    let trend_section = if trend.len() < 2 {
        String::new()
    } else {
        let mut models: Vec<String> = vec![];
        for p in trend {
            if !models.contains(&p.model) {
                models.push(p.model.clone());
            }
        }
        let key: String = models.iter().enumerate().map(|(i, m)| format!(r#"<span><i class="sw" style="background:{}"></i>{}</span>"#, colour(i), esc(m))).collect();
        let panels: String = [("node", "Node F1"), ("edge", "Edge F1"), ("item", "Item F1"), ("mark", "Marks found")].iter().map(|(k, t)| trend_panel(trend, k, t, &models)).collect();
        let rows: String = trend
            .iter()
            .map(|p| format!("<tr><td>{}</td><td>{}</td>{}</tr>", p.day, esc(&p.model), SHOWN.iter().map(|(k, _)| format!(r#"<td class="num">{}</td>"#, fmt(p.scores.get(k).copied()))).collect::<String>()))
            .collect();
        format!(r#"<h2 class="page">Trend: the dev set over time</h2><p>The last dev run of each day, from earlier reports, and this one. The dev set grows as cases are added, so a step can come from new cases as well as from the agent.</p><div class="legend">{key}</div><div class="trend">{panels}</div><table><thead><tr><th>Day</th><th>Model</th><th class="num">Node</th><th class="num">Edge</th><th class="num">Item</th><th class="num">Marks</th></tr></thead><tbody>{rows}</tbody></table>"#)
    };

    // Every case, a table per set, a column group per run.
    let mut order: Vec<(String, String)> = vec![];
    for r in runs {
        for id in &r.order {
            let set = r.cases[id]["set"].as_str().unwrap_or("other").to_string();
            if !order.iter().any(|(i, _)| i == id) {
                order.push((id.clone(), set));
            }
        }
    }
    let mut sets: Vec<String> = vec![];
    for (_, set) in &order {
        if !sets.contains(set) {
            sets.push(set.clone());
        }
    }
    let head = format!(
        "<thead><tr><th rowspan=\"2\">Case</th><th rowspan=\"2\">lines</th>{}</tr><tr>{}</tr></thead>",
        runs.iter().enumerate().map(|(i, r)| format!(r#"<th colspan="4" class="grp"><i style="background:{}"></i>{}</th>"#, colour(i), esc(&r.label))).collect::<String>(),
        runs.iter().map(|_| SHOWN.iter().map(|(_, n)| format!("<th>{n}</th>")).collect::<String>()).collect::<String>()
    );
    let mut cases = String::new();
    for set in &sets {
        let mut body = String::new();
        for (id, _) in order.iter().filter(|(_, s)| s == set) {
            let any = runs.iter().find_map(|r| r.cases.get(id)).expect("listed from a run");
            let moved = runs.iter().any(|r| r.cases.get(id).and_then(|c| c["moved"].as_f64()).is_some_and(|m| m >= 0.3));
            let flag = if moved { r#" <span class="flag">⚠</span>"# } else { "" };
            let _ = write!(body, r#"<tr><td class="id">{}{flag}<div class="nm">{}</div></td><td class="num">{}</td>"#, esc(id), esc(any["name"].as_str().unwrap_or_default()), any["lines"]);
            for r in runs {
                let c = r.cases.get(id);
                for (key, _) in SHOWN {
                    body.push_str(&heat(c.and_then(|c| c["scores"][key].as_f64())));
                }
            }
            body.push_str("</tr>");
        }
        let _ = write!(cases, r#"<h2 class="page">All cases · {}</h2><p>Darker is better. ⚠: a score moved by 0.3 or more between passes.</p><table class="cases">{head}<tbody>{body}</tbody></table>"#, esc(set));
    }

    format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><title>Board eval · {day}</title><style>
@font-face {{ font-family: 'DM Sans'; src: url('file://{fonts}/dm-sans-latin-wght-normal.woff2') format('woff2-variations'); font-weight: 100 1000; }}
@font-face {{ font-family: 'JetBrains Mono'; src: url('file://{fonts}/jetbrains-mono-latin-wght-normal.woff2') format('woff2-variations'); font-weight: 100 800; }}
@page {{ size: A4; margin: 16mm 14mm; }}
:root {{ --ink: #0b0b0b; --ink2: #52514e; --hair: #e4e3df; }}
* {{ box-sizing: border-box; }}
body {{ margin: 0; font: 400 10pt/1.5 'DM Sans', 'Noto Sans CJK SC', sans-serif; color: var(--ink); background: #fff; }}
h1 {{ font-size: 22pt; font-weight: 500; margin: 0 0 2mm; }} h2 {{ font-size: 13pt; font-weight: 500; margin: 8mm 0 2mm; break-after: avoid; }}
p {{ margin: 0 0 2.5mm; color: var(--ink2); }}
.num, .tick, td.id, .mono {{ font-family: 'JetBrains Mono', monospace; }}
table {{ border-collapse: collapse; width: 100%; font-size: 8.6pt; }}
th, td {{ padding: 1.4mm 1.6mm; text-align: left; border-bottom: 1px solid var(--hair); vertical-align: top; }}
th {{ font-weight: 600; color: var(--ink2); font-size: 8pt; }} td.num {{ text-align: right; font-size: 8.2pt; }} .muted {{ color: var(--ink2); }}
.sw, th.grp i {{ display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 6px; vertical-align: 1px; }}
th.grp {{ text-align: center; border-bottom: 0; }}
.cases td {{ padding: .8mm 1mm; }} .cases td.hm {{ text-align: center; font: 500 7.4pt 'JetBrains Mono', monospace; border: 1px solid #fff; }}
.cases td.id {{ font-size: 7.6pt; }} .cases .nm {{ font-family: 'DM Sans', 'Noto Sans CJK SC', sans-serif; color: var(--ink2); font-size: 7pt; }}
.cases thead {{ display: table-header-group; }} .cases tr {{ break-inside: avoid; }} .flag {{ color: #b8860b; }}
svg text {{ font-family: 'DM Sans', sans-serif; fill: var(--ink); }} svg .tick {{ font: 8px 'JetBrains Mono', monospace; fill: var(--ink2); }}
svg .lab {{ font-size: 10px; fill: var(--ink2); }} svg .strong {{ fill: var(--ink); font-weight: 600; }} svg .val {{ font: 500 9px 'JetBrains Mono', monospace; }}
svg .grid {{ stroke: var(--hair); }} svg .range {{ stroke: #c8c7c1; stroke-width: 2; }}
.legend {{ display: flex; flex-wrap: wrap; gap: 6mm; font-size: 8.6pt; color: var(--ink2); margin: 1mm 0 2mm; }}
.trend {{ display: grid; grid-template-columns: 1fr 1fr; gap: 3mm 6mm; }} .page {{ break-before: page; }} .note {{ font-size: 8pt; }}
</style></head><body>
<h1>Board eval</h1>
<p class="mono">{day}{commit} · {n} run{s} · up to {lines} lines</p>
<h2>Scores by metric</h2>
<div class="legend">{legend}</div>
{dots}
<table><thead><tr><th>Run</th>{heads}<th class="num">Latency p50 / p95 s</th><th class="num">Passes</th><th class="num">Failed</th></tr></thead><tbody>{summary}<tr><td class="muted">cases scored</td>{counts}<td></td><td></td><td></td></tr></tbody></table>
<p class="note">Each score is the mean over the cases that measure it, averaged over a run's passes. Runs that cover different cases are not directly comparable.</p>
{failed_note}
<h2>Latency per line</h2>
{latency}
{cost_section}
{trend_section}
{cases}
</body></html>"#,
        day = day_of(stamp),
        n = runs.len(),
        s = if runs.len() == 1 { "" } else { "s" },
        dots = dot_plot(runs),
        heads = METRICS.iter().map(|(_, n)| format!(r#"<th class="num">{n}</th>"#)).collect::<String>(),
        latency = latency_bars(runs),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_from_stamps_and_report_names() {
        assert_eq!(day_of(0), "1970-01-01");
        assert_eq!(day_of(1_794_558_649), "2026-11-13");
        assert_eq!(day_of(stamp_of("qwen3.5-2b-q4-cpu-dev-2026-10-30T11-47-56-439Z.md").unwrap()), "2026-10-30");
        assert_eq!(day_of(stamp_of("qwen3.5-2b-q4-cpu-2026-10-28T11-25-20-677Z.md").unwrap()), "2026-10-28");
        assert_eq!(stamp_of("qwen3.5-4b-q4km-cpu-dev-1794338305.md"), Some(1_794_338_305));
    }

    #[test]
    fn a_markdown_report_gives_its_day_and_scores_in_either_language() {
        let dir = std::env::temp_dir().join(format!("eval-report-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let zh = dir.join("qwen3.5-2b-q4-cpu-dev-2026-10-31T11-17-42-560Z.md");
        std::fs::write(&zh, "## 板书评测\n\n| 指标 | 值 |\n|---|---|\n| 节点 F1 | 0.892 (16 段) |\n| 连线 F1 | 0.314 (17 段) |\n").unwrap();
        let en = dir.join("qwen3.5-4b-q4km-cpu-dev-1794338305.md");
        std::fs::write(&en, "## Board eval\n\n| Metric | Value |\n|---|---|\n| Node F1 | 0.864 (28 cases) |\n| Edge F1 | 0.552 ± 0.010 (28 cases) |\n").unwrap();
        std::fs::write(dir.join("qwen3.5-4b-q4km-cpu-pointing-1794338423.md"), "| Node F1 | 1.000 (1 cases) |").unwrap();
        let points = trend(&dir, &[]).unwrap();
        assert_eq!(points.len(), 2, "the pointing report is not a dev run");
        assert_eq!((points[0].day.as_str(), points[0].model.as_str(), points[0].scores["edge"]), ("2026-10-31", "qwen3.5-2b", 0.314));
        assert_eq!((points[1].model.as_str(), points[1].scores["node"]), ("qwen3.5-4b", 0.864));
        // A name from before the set was in it: the label ends where the date begins, not at its seconds.
        let old = dir.join("qwen3.5-2b-q4-cpu-2026-10-28T11-25-20-677Z.md");
        std::fs::write(&old, "| 节点 F1 | 0.747 (16 段) |\n").unwrap();
        let (label, _, point) = read_point(&old, "qwen3.5-2b-q4-cpu-2026-10-28T11-25-20-677Z.md").unwrap();
        assert_eq!((label.as_str(), point.day.as_str()), ("qwen3.5-2b-q4-cpu", "2026-10-28"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
