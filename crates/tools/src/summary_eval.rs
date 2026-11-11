//! Runs the summary model over short meetings in eval/summary-cases.json and checks how sure each section is:
//! a guess stays an open question, an answered question leaves them, a withdrawn plan is no decision, a
//! lettered speaker owns nothing (#68). Checks are words that must or must not appear in named sections.
//!
//!   summary-eval <root> [--llm <url> | --deepseek] [--cases path] [--repeat N] [--only id,id] [--label name]
//!
//! `--deepseek` reads its key from DEEPSEEK_API_KEY and never writes it. A report goes to
//! eval/reports/summary-<label>-<time>.md, with every answer in the adjacent JSON. Failed checks fail the command.

use anyhow::{bail, Context, Result};
use boardcast_agent::summary::{blocks, summarize, Block, Line};
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
    /// When present, apply the checks separately to each list item or paragraph containing one of these words.
    #[serde(default)]
    each_with: Vec<String>,
    /// At least one of these must appear there.
    #[serde(default)]
    any: Vec<String>,
    /// None of these may.
    #[serde(default)]
    absent: Vec<String>,
    /// Standalone statements that lose uncertainty; a question containing these words can be valid.
    #[serde(default)]
    absent_items: Vec<String>,
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
        let texts = if check.each_with.is_empty() { vec![text] } else {
            blocks(&text).into_iter().filter_map(|block| match block {
                Block::Bullet(text) | Block::Paragraph(text) | Block::Todo { text, .. }
                    if check.each_with.iter().any(|w| text.contains(&w.to_lowercase())) => Some(text),
                _ => None,
            }).collect()
        };
        for text in texts {
            let mut place = check.sections.join("/");
            if !check.each_with.is_empty() { let _ = write!(place, " item {text:?}"); }
            if !check.any.is_empty() && !check.any.iter().any(|w| text.contains(&w.to_lowercase())) {
                out.push(format!("{place} lacks any of {:?}", check.any));
            }
            for w in check.absent.iter().filter(|w| text.contains(&w.to_lowercase())) {
                out.push(format!("{place} has \"{w}\""));
            }
            for item in blocks(&text) {
                let (Block::Bullet(item) | Block::Paragraph(item) | Block::Todo { text: item, .. }) = item else { continue };
                for w in check.absent_items.iter().filter(|w| item.trim().trim_end_matches(['.', '。']).eq_ignore_ascii_case(w)) {
                    out.push(format!("{place} has standalone \"{w}\""));
                }
            }
        }
    }
    out
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("boardcast root")?);
    let mut provider = Provider::Local { base: "http://127.0.0.1:18765".into() };
    let mut cases_path = PathBuf::from("eval/summary-cases.json");
    let (mut repeat, mut only, mut label) = (1, None::<String>, "local".to_string());
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--llm" => provider = Provider::Local { base: args.next().context("url")? },
            "--deepseek" => (provider, label) = (Provider::deepseek_from_env()?, "deepseek".into()),
            "--repeat" => repeat = args.next().context("count")?.parse::<usize>()?.max(1),
            "--cases" => cases_path = args.next().context("cases path")?.into(),
            "--only" => only = args.next(),
            "--label" => label = args.next().context("label")?,
            other => bail!("unknown flag {other}"),
        }
    }
    if label.is_empty() || !label.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) {
        bail!("label must contain only letters, digits, -, _ or .");
    }
    let input = std::fs::read(root.join(&cases_path)).with_context(|| format!("reading {}", cases_path.display()))?;
    let mut cases: Vec<Case> = serde_json::from_slice(&input).with_context(|| format!("parsing {}", cases_path.display()))?;
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
            let started = std::time::Instant::now();
            let (summary, failed) = match summarize(&provider, &case.boards, &lines, &cancel, &mut |_| {}) {
                Ok(summary) => {
                    let failed = failures(&summary, &case.checks);
                    (summary, failed)
                }
                Err(error) => (String::new(), vec![format!("generation failed: {error:#}")]),
            };
            answers.push(json!({ "case": case.id, "run": run + 1, "summary": summary, "failures": failed, "elapsed_ms": started.elapsed().as_millis() }));
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
        "checker_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("summary_eval.rs"))),
        "summary_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("../../agent/src/summary.rs"))),
        "agent_source_sha256": format!("{:x}", Sha256::digest(include_bytes!("../../agent/src/lib.rs"))),
        "answers": answers,
    }))?)?;
    // Deliberate model failures in unit tests should not enter the workflow's evaluation summary.
    #[cfg(not(test))]
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
    fn ci_model_labels_work_but_paths_are_rejected() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for (label, expected) in [("qwen3.5-4b-q4km-cpu", "no matching cases"), ("../outside", "label must contain"), ("", "label must contain")] {
            let args = [root.to_string_lossy().into_owned(), "--label".into(), label.into(), "--only".into(), "no-such-case".into()];
            assert!(run(args.into_iter()).unwrap_err().to_string().contains(expected));
        }
    }

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
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../../eval/summary-context-cases.json")).unwrap();
        assert_eq!(cases.len(), 4);
        assert!(cases.iter().all(|c| !c.checks.is_empty() && !c.lines.is_empty()));
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../../eval/summary-long-transcript-cases.json")).unwrap();
        assert_eq!(cases.len(), 2);
        assert!(cases.iter().all(|c| !c.checks.is_empty() && c.lines.len() > 80));
    }

    #[test]
    fn a_missing_section_cannot_pass_an_absence_check() {
        let checks: Vec<Check> = serde_json::from_str(r#"[{"in":["Decisions"],"absent":["two"]}]"#).unwrap();
        assert!(!failures("No headings or decisions at all", &checks).is_empty());
        assert!(failures("### Decisions\n- None", &checks).is_empty());
    }

    #[test]
    fn a_generation_failure_still_writes_the_evaluation_report() {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("boardcast-summary-eval-{}-{stamp}", std::process::id()));
        let cases = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../eval/summary-cases.json");
        let args = [root.to_string_lossy().into_owned(), "--cases".into(), cases.to_string_lossy().into_owned(),
            "--only".into(), "zh-guess-stays-open".into(), "--llm".into(), "http://127.0.0.1:0".into()];
        assert!(run(args.into_iter()).unwrap_err().to_string().contains("1 of 1 summaries failed"));
        let report = std::fs::read_dir(root.join("eval/reports")).unwrap().flatten().map(|f| f.path())
            .find(|p| p.extension().is_some_and(|x| x == "json")).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
        assert_eq!(value["answers"].as_array().unwrap().len(), 1);
        assert_eq!(value["answers"][0]["summary"], "");
        assert!(value["answers"][0]["failures"][0].as_str().unwrap().starts_with("generation failed:"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_partial_answer_keeps_the_unresolved_question_in_its_section() {
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../../eval/summary-cases.json")).unwrap();
        let checks = &cases.iter().find(|c| c.id == "zh-partial-answer-stays-open").unwrap().checks;
        let valid = "### 结论\n- 保留十分钟\n### 决定\n- 保留十分钟\n### 待办\n- 无\n### 待讨论\n- 缓存过期后由谁刷新（下次讨论）";
        assert!(failures(valid, checks).is_empty());
        assert!(!failures(&valid.replace("缓存过期后由谁刷新（下次讨论）", "无"), checks).is_empty());
        assert!(!failures(&valid.replace("缓存过期后由谁刷新（下次讨论）", "缓存保留多久，过期后谁来刷新？"), checks).is_empty());
        let checks = &cases.iter().find(|c| c.id == "en-partial-answer-stays-open").unwrap().checks;
        let valid = "### Conclusions\n- Ten minutes\n### Decisions\n- None\n### To-dos\n- None\n### Open questions\n- Who will refresh the cache?";
        assert!(failures(valid, checks).is_empty());
        assert!(!failures(&valid.replace("Who will refresh the cache?", "How long should the cache live, and who will refresh it?"), checks).is_empty());
    }

    #[test]
    fn uncertainty_checks_also_keep_other_facts_and_reject_presuppositions() {
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../../eval/summary-cases.json")).unwrap();
        let checks = &cases.iter().find(|c| c.id == "en-guess-stays-open").unwrap().checks;
        let valid = "### Conclusions\n- Ship on Wednesday.\n### Decisions\n- None\n### Open questions\n- Should there be two failure criteria?";
        assert!(failures(valid, checks).is_empty());
        assert!(!failures(&valid.replace("Ship on Wednesday.", "None"), checks).is_empty());
        assert!(!failures(&valid.replace("Should there be two failure criteria?", "What are the two failure criteria?"), checks).is_empty());
        assert!(!failures(&valid.replace("Should there be two failure criteria?", "**Two failure criteria**"), checks).is_empty());
        assert!(failures(&valid.replace("Should there be two failure criteria?", "Whether there should be two failure criteria"), checks).is_empty());
        let checks = &cases.iter().find(|c| c.id == "zh-guess-stays-open").unwrap().checks;
        let wrong = "### 结论\n失败标准应包含两项\n部署定于本周三上线\n### 决定\n无\n### 待讨论\n失败标准具体有哪些？";
        assert!(!failures(wrong, checks).is_empty());
        assert!(failures(&wrong.replace("失败标准应包含两项\n", ""), checks).is_empty());
    }

    #[test]
    fn an_open_date_label_needs_no_qualifier_but_a_stated_date_does() {
        let cases: Vec<Case> = serde_json::from_str(include_str!("../../../eval/summary-cases.json")).unwrap();
        let checks = &cases.iter().find(|c| c.id == "zh-stated-date-beside-tentative-date").unwrap().checks;
        let summary = |open: &str| format!("### 结论\n- 桌面版周四发布\n### 决定\n- 无\n### 待办\n- 无\n### 待讨论\n{open}");
        for valid in ["- 移动版发布日期", "移动版发布日期", "- 移动版是否周五发布", "- 移动版周五发布尚未确定"] {
            assert!(failures(&summary(valid), checks).is_empty(), "{valid}");
        }
        for invalid in ["- 移动版周五发布", "移动版星期五发布", "- 移动版周五发布\n- 预算待确认",
            "- 移动版周五发布尚未确定\n- 移动版星期五发布"] {
            assert!(!failures(&summary(invalid), checks).is_empty(), "{invalid}");
        }
        assert!(!failures(&summary("- 无"), checks).is_empty(), "the topic is still required");
    }
}
