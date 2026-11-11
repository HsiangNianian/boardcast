//! Runs the summary model over short meetings in eval/summary-cases.json and checks how sure each section is:
//! a guess stays an open question, an answered question leaves them, a withdrawn plan is no decision, a
//! lettered speaker owns nothing (#68). Checks are words that must or must not appear in named sections.
//!
//!   summary-eval <root> [--llm <url> | --deepseek] [--repeat N] [--only id,id] [--label name]
//!
//! `--deepseek` reads its key from DEEPSEEK_API_KEY and never writes it. A report goes to
//! eval/reports/summary-<label>-<time>.md, with every answer in the adjacent JSON. Failed checks fail the command.

use anyhow::{bail, Context, Result};
use boardcast_agent::summary::{summarize, Line};
use boardcast_agent::Provider;
use boardcast_board::graph::Graph;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

#[derive(Deserialize)]
struct Case {
    id: String,
    why: String,
    lines: Vec<(String, String)>,
    #[serde(default)]
    boards: Vec<Graph>,
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
        if !parts.iter().any(|(name, _)| check.sections.contains(name)) {
            out.push(format!("missing section {}", check.sections.join("/")));
        }
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
            "--label" => label = args.next().context("label")?,
            other => bail!("unknown flag {other}"),
        }
    }
    if label.is_empty() || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("label must contain only letters, digits, - or _");
    }
    let input = std::fs::read(root.join("eval/summary-cases.json"))?;
    let mut cases: Vec<Case> = serde_json::from_slice(&input).context("reading eval/summary-cases.json")?;
    cases.retain(|c| only.as_ref().is_none_or(|ids| ids.split(',').any(|id| id == c.id)));
    if cases.is_empty() {
        bail!("no matching cases");
    }
    let cancel = AtomicBool::new(false);
    let mut report = format!("# Summary checks ({label}, {repeat} run{})\n\n| Case | Passed | What went wrong |\n| --- | --- | --- |\n", if repeat == 1 { "" } else { "s" });
    let (mut passed, mut runs) = (0, 0);
    let mut answers = vec![];
    for case in &cases {
        let lines: Vec<Line> = case.lines.iter().map(|(who, text)| Line { speaker: Some(who.clone()), text: text.clone(), page: 0, has_page: false }).collect();
        let (mut ok, mut wrong) = (0, vec![]);
        for run in 0..repeat {
            let summary = summarize(&provider, &case.boards, &lines, &cancel, &mut |_| {}).with_context(|| format!("summarizing {}", case.id))?;
            let failed = failures(&summary, &case.checks);
            answers.push(json!({ "case": case.id, "run": run + 1, "summary": summary, "failures": failed }));
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
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos();
    let out = root.join(format!("eval/reports/summary-{label}-{stamp}.md"));
    std::fs::create_dir_all(out.parent().expect("a folder"))?;
    std::fs::write(&out, &report)?;
    std::fs::write(out.with_extension("json"), serde_json::to_vec_pretty(&json!({
        "label": label,
        "repeat": repeat,
        "cases_sha256": format!("{:x}", Sha256::digest(&input)),
        "summary_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("../../agent/src/summary.rs"))),
        "agent_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("../../agent/src/lib.rs"))),
        "answers": answers,
    }))?)?;
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        std::fs::OpenOptions::new().create(true).append(true).open(path)?.write_all(report.as_bytes())?;
    }
    println!("{passed}/{runs} passed; report in {}", out.display());
    if passed != runs {
        bail!("{} of {runs} summaries failed their checks", runs - passed);
    }
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

    #[test]
    fn a_missing_section_cannot_pass_an_absence_check() {
        let checks: Vec<Check> = serde_json::from_str(r#"[{"in":["Decisions"],"absent":["two"]}]"#).unwrap();
        assert!(!failures("No headings or decisions at all", &checks).is_empty());
        assert!(failures("### Decisions\n- None", &checks).is_empty());
    }

    #[test]
    fn a_partial_answer_keeps_the_unresolved_question_in_its_section() {
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../../eval/summary-cases.json")).unwrap();
        let checks = &cases.iter().find(|c| c.id == "zh-partial-answer-stays-open").unwrap().checks;
        let valid = "### 结论\n- 保留十分钟\n### 决定\n- 保留十分钟\n### 待办\n- 无\n### 待讨论\n- 缓存过期后由谁刷新（下次讨论）";
        assert!(failures(valid, checks).is_empty());
        assert!(!failures(&valid.replace("缓存过期后由谁刷新（下次讨论）", "无"), checks).is_empty());
    }

    #[test]
    fn uncertainty_checks_also_keep_other_facts_and_reject_presuppositions() {
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../../eval/summary-cases.json")).unwrap();
        let checks = &cases.iter().find(|c| c.id == "en-guess-stays-open").unwrap().checks;
        let valid = "### Conclusions\n- Ship on Wednesday.\n### Decisions\n- None\n### Open questions\n- Should there be two failure criteria?";
        assert!(failures(valid, checks).is_empty());
        assert!(!failures(&valid.replace("Ship on Wednesday.", "None"), checks).is_empty());
        assert!(!failures(&valid.replace("Should there be two failure criteria?", "What are the two failure criteria?"), checks).is_empty());
    }
}
