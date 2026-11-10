//! Runs the summary model over short meetings in eval/summary-cases.json and checks how sure each section is:
//! a guess stays an open question, an answered question leaves them, a withdrawn plan is no decision, a
//! lettered speaker owns nothing (#68). Checks are words that must or must not appear in named sections.
//!
//!   summary-eval <root> [--llm <url> | --deepseek] [--repeat N] [--only id,id]
//!
//! `--deepseek` reads its key from DEEPSEEK_API_KEY and never writes it. A report goes to
//! eval/reports/summary.md.

use anyhow::{bail, Context, Result};
use boardcast_agent::summary::{summarize, Line};
use boardcast_agent::Provider;
use serde::Deserialize;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

#[derive(Deserialize)]
struct Case {
    id: String,
    why: String,
    lines: Vec<(String, String)>,
    checks: Vec<Check>,
}

#[derive(Deserialize)]
struct Check {
    /// The sections looked in, by heading.
    #[serde(rename = "in")]
    sections: Vec<String>,
    /// At least one of these must appear there.
    #[serde(default)]
    any: Vec<String>,
    /// None of these may.
    #[serde(default)]
    absent: Vec<String>,
}

/// The text under each `### heading` of a summary.
fn sections(summary: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vec![];
    for line in summary.lines() {
        match line.strip_prefix("### ") {
            Some(name) => out.push((name.trim().to_string(), String::new())),
            None => {
                if let Some((_, body)) = out.last_mut() {
                    body.push_str(line);
                    body.push('\n');
                }
            }
        }
    }
    out
}

/// What a summary got wrong against a case's checks; empty when it passed.
fn failures(summary: &str, checks: &[Check]) -> Vec<String> {
    let parts = sections(summary);
    let mut out = vec![];
    for check in checks {
        let text: String = parts.iter().filter(|(name, _)| check.sections.contains(name)).map(|(_, body)| body.to_lowercase()).collect();
        let place = check.sections.join("/");
        if !check.any.is_empty() && !check.any.iter().any(|w| text.contains(&w.to_lowercase())) {
            out.push(format!("{place} lacks any of {:?}", check.any));
        }
        for w in check.absent.iter().filter(|w| text.contains(&w.to_lowercase())) {
            out.push(format!("{place} has \"{w}\""));
        }
    }
    out
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("boardcast root")?);
    let mut provider = Provider::Local { base: "http://127.0.0.1:18765".into() };
    let (mut repeat, mut only, mut label) = (1, None::<String>, "local".to_string());
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--llm" => provider = Provider::Local { base: args.next().context("url")? },
            "--deepseek" => (provider, label) = (Provider::deepseek_from_env()?, "deepseek".into()),
            "--repeat" => repeat = args.next().context("count")?.parse::<usize>()?.max(1),
            "--only" => only = args.next(),
            other => bail!("unknown flag {other}"),
        }
    }
    let mut cases: Vec<Case> = serde_json::from_slice(&std::fs::read(root.join("eval/summary-cases.json"))?).context("reading eval/summary-cases.json")?;
    cases.retain(|c| only.as_ref().is_none_or(|ids| ids.split(',').any(|id| id == c.id)));
    if cases.is_empty() {
        bail!("no matching cases");
    }
    let cancel = AtomicBool::new(false);
    let mut report = format!("# Summary checks ({label}, {repeat} run{})\n\n| Case | Passed | What went wrong |\n| --- | --- | --- |\n", if repeat == 1 { "" } else { "s" });
    let (mut passed, mut runs) = (0, 0);
    for case in &cases {
        let lines: Vec<Line> = case.lines.iter().map(|(who, text)| Line { speaker: Some(who.clone()), text: text.clone(), page: 0, has_page: false }).collect();
        let (mut ok, mut wrong) = (0, vec![]);
        for _ in 0..repeat {
            let summary = summarize(&provider, &[], &lines, &cancel, &mut |_| {}).with_context(|| format!("summarizing {}", case.id))?;
            let failed = failures(&summary, &case.checks);
            if failed.is_empty() {
                ok += 1;
            } else {
                println!("{}: {}\n{summary}\n", case.id, failed.join("; "));
                wrong.extend(failed);
            }
        }
        wrong.sort();
        wrong.dedup();
        println!("{}: {ok}/{repeat}", case.id);
        let _ = writeln!(report, "| {} ({}) | {ok}/{repeat} | {} |", case.id, case.why, wrong.join("; "));
        (passed, runs) = (passed + ok, runs + repeat);
    }
    let _ = writeln!(report, "\n{passed} of {runs} summaries passed every check.");
    let out = root.join("eval/reports/summary.md");
    std::fs::create_dir_all(out.parent().expect("a folder"))?;
    std::fs::write(&out, &report)?;
    println!("{passed}/{runs} passed; report in {}", out.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_look_only_in_their_sections() {
        let checks: Vec<Check> = serde_json::from_str(r#"[{"in":["结论","决定"],"absent":["两个"]},{"in":["待讨论"],"any":["失败标准"]}]"#).unwrap();
        assert!(failures("### 结论\n- 无\n### 决定\n- 周三上线\n### 待办\n- 无\n### 待讨论\n- 失败标准是不是两个", &checks).is_empty());
        let wrong = failures("### 结论\n- 失败标准有两个\n### 待讨论\n- 无", &checks);
        assert_eq!(wrong.len(), 2, "{wrong:?}");
    }

    #[test]
    fn every_case_reads() {
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../../eval/summary-cases.json")).unwrap();
        assert!(cases.len() >= 8 && cases.iter().all(|c| !c.checks.is_empty() && !c.lines.is_empty()));
    }
}
