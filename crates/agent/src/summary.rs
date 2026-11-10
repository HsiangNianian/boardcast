//! The meeting summary, ported from src/engine/summary.ts: conclusions, decisions, to-dos and open
//! questions. A long meeting is noted part by part, then merged with the board.
//! The Markdown here is what a meeting stores, so `blocks` reads it for display and to tick to-dos.

use crate::Provider;
use anyhow::{anyhow, bail, Result};
use boardcast_board::graph::{is_english, Graph, Kind};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

/// The most text in one part; a small local model's context is limited.
const CHUNK: usize = 3000;
const BOARD_LIMIT: usize = 1500;

struct Words {
    sections: [&'static str; 4],
    none: &'static str,
    board: &'static str,
    page_mark: fn(u64) -> String,
    colon: &'static str,
    no_lines: &'static str,
    transcript: &'static str,
    fragment: &'static str,
    board_intro: &'static str,
    part: fn(usize) -> String,
    too_long: &'static str,
    partial: &'static str,
    system: &'static str,
    facts: &'static str,
    rules: &'static str,
}

const ZH: Words = Words {
    sections: ["结论", "决定", "待办", "待讨论"],
    none: "无",
    board: "第 {} 张板",
    page_mark: |n| format!("【第 {n} 张板】"),
    colon: "：",
    no_lines: "（没有记录）",
    transcript: "会议记录：",
    fragment: "会议记录（片段）：",
    board_intro: "白板上的内容：",
    part: |n| format!("第 {n} 段："),
    too_long: "会议记录太长，下面是逐段记下的要点：",
    partial: "你在帮人整理会议记录。这是一场长会议中的一段，记下这一段里的要点：说了什么结论、定了什么、谁要做什么、还有什么没定。每条一行，以「- 」开头。",
    system: "你在帮人写会议摘要。",
    facts: "保留原话的主体、否定和进展，区分已有成果与后续工作。\n只把明确达成的共识列为决定，把建议、希望和未定事项放在待讨论。\n结论和决定只写会上确认过的事；只是猜测的（「应该……吧」「是不是……」），哪怕说的是数量或标准，也只写进待讨论。\n一个问题后面有人回答、大家接受了（如「之后怎么办？」「就缩小字号」「好」），就把答案写进结论或决定，这个问题从待讨论里去掉，也不要换个说法再问一遍。\n只是「也许」「可能」「得验证一下」的回答不算定下来：这个问题照原样留在待讨论，可写上提到的办法。\n会上明确同意要做的选择（「同意」「就用」「改用」「定了」）写进决定，撤回的方案不写成决定。\n只把明确承诺或分配的工作列为待办，保留已说明的负责人和日期，不自行补充。说话人是 A、B、C 这样的字母时，那是自动编号，不是人名：待办不写负责人括号。\n只总结会议记录（或分段笔记）与白板文字中明确写出的事实，包括手动修正和批注；不从划线等标记推测决定。",
    rules: "只根据给出的内容写，不要编造；没有的部分写「无」。\n用简体中文，每条一行，以「- 」开头，简短。\n待办写成「- [ ] 事情（负责人：某某）」，没提到负责人就不写括号。\n只在记录明确标注「第 N 张板」时引用板号，不把不同话题当成不同板。\n严格按下面的格式输出，不要别的话：",
};

const EN: Words = Words {
    sections: ["Conclusions", "Decisions", "To-dos", "Open questions"],
    none: "None",
    board: "Board {}",
    page_mark: |n| format!("[Board {n}]"),
    colon: ": ",
    no_lines: "(no transcript)",
    transcript: "Transcript:",
    fragment: "Transcript (excerpt):",
    board_intro: "On the whiteboard:",
    part: |n| format!("Part {n}:"),
    too_long: "The transcript is too long; these are the notes taken part by part:",
    partial: "You are helping someone tidy up meeting notes. This is one part of a long meeting; note its key points: conclusions reached, what was decided, who will do what, and what is still open. One point per line, starting with '- '.",
    system: "You are helping someone write a meeting summary.",
    facts: "Preserve subjects, negations and progress; distinguish existing work from future work.\nPut only explicit agreements in Decisions; keep suggestions, hopes and unresolved matters under Open questions.\nConclusions and Decisions hold only what the meeting confirmed; a guess (\"it should be two, right?\"), even about a number or a criterion, goes under Open questions.\nA question that is answered and accepted later in the meeting (\"what then?\" \"then we shrink the font\" \"good\") is settled: put the answer under Conclusions or Decisions and leave the question out of Open questions, not even reworded.\nAn answer given as a maybe (\"perhaps\", \"we could\", \"needs checking\") settles nothing: keep the question as asked under Open questions, with the idea mentioned.\nA choice the meeting explicitly agreed on (\"agreed\", \"let's go with\", \"we'll use\") goes under Decisions; a withdrawn plan is no decision.\nList only explicitly promised or assigned work as To-dos, retaining stated owners and dates without inventing them. Speakers named by a single letter such as A, B or C are numbered automatically, not named: give such to-dos no owner.\nUse explicitly stated facts from the transcript (or its notes) and whiteboard text, including manual corrections and annotations; do not infer decisions from marks such as strikethroughs.",
    rules: "Write only from what is given, never make things up; write \"None\" for an empty section.\nWrite in English, one short point per line, starting with \"- \".\nWrite to-dos as \"- [ ] task (owner: name)\"; leave out the parentheses if no owner was mentioned.\nMention a board number only when explicitly labeled in the source; do not infer board numbers from topics.\nOutput exactly the format below and nothing else:",
};

fn words(english: bool) -> &'static Words {
    if english { &EN } else { &ZH }
}

/// One spoken line, as the summary sees it. Manual notes are left out before this point.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub speaker: Option<String>,
    pub text: String,
    pub page: u64,
    /// Older meetings may not say which board a line was on, and the fingerprint must agree with them.
    pub has_page: bool,
}

impl Line {
    /// The spoken lines of a stored transcript.
    pub fn spoken(lines: &[Value]) -> Vec<Self> {
        lines
            .iter()
            .filter(|l| !l["manual"].as_bool().unwrap_or(false))
            .map(|l| Self {
                speaker: l["speaker"].as_str().map(str::to_string),
                text: l["text"].as_str().unwrap_or_default().to_string(),
                page: l["page"].as_u64().unwrap_or(0),
                has_page: l["page"].is_u64(),
            })
            .collect()
    }
}

/// What a summary was made from, to tell later whether the meeting has moved on. It must match
/// the earlier app's summary.ts, because meetings store it: JSON text, hashed over UTF-16 units.
pub fn fingerprint(boards: &[Graph], lines: &[Line]) -> String {
    let rows: Vec<Value> = lines
        .iter()
        .map(|l| {
            let mut row = json!({ "text": l.text });
            if let Some(speaker) = &l.speaker {
                row["speaker"] = json!(speaker);
            }
            if l.has_page {
                row["page"] = json!(l.page);
            }
            row
        })
        .collect();
    let source = serde_json::to_string(&json!({ "boards": boards, "lines": rows })).expect("JSON serializes");
    let (mut hash, mut length) = (2_166_136_261_u32, 0_usize);
    for unit in source.encode_utf16() {
        hash = (hash ^ u32::from(unit)).wrapping_mul(16_777_619);
        length += 1;
    }
    format!("{}-{length}", to_base36(hash))
}

fn to_base36(mut n: u32) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut digits = vec![];
    while n > 0 {
        digits.push(char::from_digit(n % 36, 36).expect("under 36"));
        n /= 36;
    }
    digits.iter().rev().collect()
}

fn transcript(lines: &[Line], english: bool) -> String {
    let w = words(english);
    let many = lines.iter().map(|l| l.page).collect::<HashSet<_>>().len() > 1;
    let mut out = vec![];
    let mut page = None;
    for l in lines {
        if many && page != Some(l.page) {
            page = Some(l.page);
            out.push((w.page_mark)(l.page + 1));
        }
        out.push(l.speaker.as_ref().map_or_else(|| l.text.clone(), |s| format!("{s}{}{}", w.colon, l.text)));
    }
    out.join("\n")
}

/// Split by length, never inside a sentence.
fn chunks(lines: &[Line], size: usize) -> Vec<Vec<Line>> {
    let mut out = vec![];
    let (mut current, mut n): (Vec<Line>, usize) = (vec![], 0);
    for l in lines {
        let length = l.text.encode_utf16().count();
        if !current.is_empty() && n + length > size {
            out.push(std::mem::take(&mut current));
            n = 0;
        }
        current.push(l.clone());
        n += length + 1;
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// A board as the summary sees it: imported text nobody touched is left out, being long and undiscussed.
fn board_text(g: &Graph) -> String {
    let mut touched: HashSet<&str> = g.nodes.iter().filter_map(|n| n.parent.as_deref()).collect();
    for e in &g.edges {
        touched.insert(&e.from);
        touched.insert(&e.to);
    }
    let mut out = g.clone();
    out.nodes.retain(|n| n.kind != Kind::Region || !n.marks.is_empty() || n.extra.get("spans").and_then(Value::as_array).is_some_and(|s| !s.is_empty()) || touched.contains(n.id.as_str()));
    out.describe()
}

fn cut(text: String, max: usize) -> String {
    if text.encode_utf16().count() > max {
        let kept: Vec<u16> = text.encode_utf16().take(max).collect();
        format!("{}\n……", String::from_utf16_lossy(&kept))
    } else {
        text
    }
}

fn boards_text(boards: &[Graph], english: bool) -> String {
    let w = words(english);
    let live: Vec<&Graph> = boards.iter().filter(|g| !g.nodes.is_empty()).collect();
    if live.len() <= 1 {
        return cut(board_text(live.first().copied().or(boards.first()).unwrap_or(&Graph::default())), BOARD_LIMIT);
    }
    let parts: Vec<String> = boards
        .iter()
        .enumerate()
        .filter(|(_, g)| !g.nodes.is_empty())
        .map(|(i, g)| format!("{}{}\n{}", w.board.replace("{}", &(i + 1).to_string()), w.colon.trim(), cut(board_text(g), BOARD_LIMIT)))
        .collect();
    parts.join("\n\n")
}

/// The topics of the template a meeting was started from, kept on its first board.
pub fn template_topics(boards: &[Graph]) -> Vec<String> {
    boards.iter().find_map(|g| g.extra.get("template")).and_then(|t| serde_json::from_value(t.clone()).ok()).unwrap_or_default()
}

/// How a meeting started from a template has its conclusions laid out: one part per topic of the template.
fn template_rule(topics: &[String], english: bool) -> Option<String> {
    if topics.is_empty() {
        return None;
    }
    Some(if english {
        format!("This meeting follows a template with the topics {}. Under Conclusions, give each topic that was discussed its own part, starting with \"#### topic\"; leave out topics nobody spoke to.", topics.join(" / "))
    } else {
        format!("这场会按模板开，话题是：{}。「结论」里按这些话题分小节，每个小节以「#### 话题」开头；没人说到的话题不写。", topics.join(" / "))
    })
}

fn messages(boards: &[Graph], said: &str, partial: bool, english: bool) -> Vec<Value> {
    let w = words(english);
    let structure = template_rule(&template_topics(boards), english).map_or_else(String::new, |r| format!("{r}\n"));
    let rules = format!("{structure}{}\n{}", w.rules, w.sections.map(|s| format!("### {s}")).join("\n"));
    let system = if partial { format!("{}\n{}", w.partial, w.facts) } else { format!("{}\n{}\n{rules}", w.system, w.facts) };
    let user = if partial { format!("{}\n{said}", w.fragment) } else { format!("{}\n{}\n\n{said}", w.board_intro, boards_text(boards, english)) };
    vec![json!({ "role": "system", "content": system }), json!({ "role": "user", "content": user })]
}

static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(?:#{1,6}\s*)?(?:\*\*|【)?\s*(结论|决定|待办|待办事项|待讨论)\s*(?:\*\*|】)?\s*[:：]?\s*(?:\*\*)?\s*$").expect("valid"));
static HEADING_EN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^\s*(?:#{1,6}\s*)?(?:\*\*)?\s*(conclusions?|decisions?( made)?|to-?dos?|action items?|next steps|open questions?|open issues|to discuss)\s*(?:\*\*)?\s*:?\s*(?:\*\*)?\s*$").expect("valid"));
static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)```[a-z]*\n?").expect("valid"));

fn canonical_en(heading: &str) -> &'static str {
    let k = heading.to_lowercase();
    if k.starts_with("conclusion") {
        "Conclusions"
    } else if k.starts_with("decision") {
        "Decisions"
    } else if k.starts_with("open") || k == "to discuss" {
        "Open questions"
    } else {
        "To-dos"
    }
}

/// Clean up a reply: no fences or preamble, headings made uniform, missing sections filled in. If no
/// section can be recognized it stays as written, which beats four empty ones.
pub fn tidy(text: &str, english: bool) -> String {
    let w = words(english);
    let unfenced = FENCE.replace_all(text, "");
    let mut found = 0;
    let rows: Vec<String> = unfenced
        .trim()
        .split('\n')
        .map(|l| {
            let name = if english {
                HEADING_EN.captures(l).map(|c| canonical_en(&c[1]).to_string())
            } else {
                HEADING.captures(l).map(|c| if &c[1] == "待办事项" { "待办".to_string() } else { c[1].to_string() })
            };
            match name {
                Some(name) => {
                    found += 1;
                    format!("### {name}")
                }
                None => l.to_string(),
            }
        })
        .collect();
    let mut out = rows.join("\n").trim().to_string();
    if found == 0 {
        return out;
    }
    if let Some(first) = out.find("### ").filter(|at| *at > 0) {
        out = out[first..].to_string();
    }
    for section in w.sections {
        if !out.lines().any(|l| l == format!("### {section}")) {
            out.push_str(&format!("\n\n### {section}\n- {}", w.none));
        }
    }
    out.trim().to_string()
}

static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b([ng]\d+)\b(?:\s*[(（]([^)）\n]*)[)）])?").expect("valid"));

/// The board reaches the model with its ids, as in `n6 [话题] Login`, and a model sometimes copies them
/// into the summary. Each id becomes its node's or group's text; one already followed by that text, as in
/// `n6 (Login)` or `n6 Login`, is simply dropped.
pub fn name_ids(text: &str, boards: &[Graph]) -> String {
    let label = |id: &str| {
        boards.iter().find_map(|g| {
            g.nodes.iter().find(|n| n.id == id).map(|n| n.label.clone()).or_else(|| g.groups.iter().find(|x| x.id == id).map(|x| x.label.clone()))
        })
    };
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for c in ID.captures_iter(text) {
        let whole = c.get(0).expect("a match");
        let Some(name) = label(&c[1]) else { continue };
        out.push_str(&text[last..whole.start()]);
        let rest = &text[whole.end()..];
        match c.get(2) {
            // The text in brackets is usually the name again; otherwise it is a note on it, and stays.
            Some(said) if said.as_str().trim() == name => out.push_str(&name),
            Some(said) if !said.as_str().trim().is_empty() => out.push_str(&format!("{name}{}", text[c.get(1).expect("an id").end()..whole.end()].trim_start())),
            _ if rest.trim_start().starts_with(&name) => {}
            _ => out.push_str(&name),
        }
        last = whole.end();
        if c.get(2).is_none() && rest.trim_start().starts_with(&name) {
            last += rest.len() - rest.trim_start().len();
        }
    }
    out.push_str(&text[last..]);
    out
}

static LETTER_OWNER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*[(（]\s*(?:负责人|owner)\s*[:：]\s*(?:[A-Z]|无|未定|未知|待定|(?i:none|n/a|tbd|unknown))\s*[)）]").expect("valid"));

/// Speakers nobody named are lettered A, B, C as they come, so a letter says who spoke, not who owns the
/// work. Such an owner is taken off a to-do.
pub fn no_letter_owners(text: &str) -> String {
    LETTER_OWNER.replace_all(text, "").into_owned()
}

static BOLD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*(.+?)\*\*").expect("valid"));
static STRUCK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"~~(.+?)~~").expect("valid"));
static CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]+)`").expect("valid"));
static BLANKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").expect("valid"));
static MD_HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#{1,6}\s*").expect("valid"));
static DONE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)[-*+]\s+\[[xX]\]\s*").expect("valid"));
static OPEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)[-*+]\s+\[ \]\s*").expect("valid"));
static BULLET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\s*)[-*+]\s+").expect("valid"));

fn strip_inline(text: &str) -> String {
    let text = BOLD.replace_all(text, "$1");
    let text = STRUCK.replace_all(&text, "$1");
    CODE.replace_all(&text, "$1").into_owned()
}

/// The "text only" copy: Markdown marks removed, to-dos as boxes, lists as dots.
pub fn plain_text(markdown: &str) -> String {
    let rows: Vec<String> = markdown
        .split('\n')
        .enumerate()
        .map(|(i, l)| {
            // Sections are set apart by a blank line.
            let l = MD_HEADING.replace(l, if i > 0 { "\n" } else { "" });
            let l = DONE.replace(&l, "${1}☑ ");
            let l = OPEN.replace(&l, "${1}☐ ");
            let l = BULLET.replace(&l, "${1}• ");
            strip_inline(&l)
        })
        .collect();
    BLANKS.replace_all(&rows.join("\n"), "\n\n").trim().to_string()
}

/// A summary line, read for display.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading(String),
    /// `line` is where it sits in the text, for ticking it.
    Todo { done: bool, text: String, line: usize },
    Bullet(String),
    Paragraph(String),
}

pub fn blocks(markdown: &str) -> Vec<Block> {
    let mut out = vec![];
    for (line, raw) in markdown.split('\n').enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        let block = if MD_HEADING.is_match(raw) {
            Block::Heading(strip_inline(&MD_HEADING.replace(raw, "")))
        } else if DONE.is_match(raw) {
            Block::Todo { done: true, text: strip_inline(&DONE.replace(raw, "")), line }
        } else if OPEN.is_match(raw) {
            Block::Todo { done: false, text: strip_inline(&OPEN.replace(raw, "")), line }
        } else if BULLET.is_match(raw) {
            Block::Bullet(strip_inline(&BULLET.replace(raw, "")))
        } else {
            Block::Paragraph(strip_inline(raw))
        };
        out.push(block);
    }
    out
}

/// Tick or untick the to-do on a line of the text.
pub fn toggle_todo(markdown: &str, line: usize) -> String {
    let rows: Vec<String> = markdown
        .split('\n')
        .enumerate()
        .map(|(i, raw)| {
            if i != line {
                raw.to_string()
            } else if DONE.is_match(raw) {
                raw.replacen("[x]", "[ ]", 1).replacen("[X]", "[ ]", 1)
            } else if OPEN.is_match(raw) {
                raw.replacen("[ ]", "[x]", 1)
            } else {
                raw.to_string()
            }
        })
        .collect();
    rows.join("\n")
}

pub enum Step {
    /// Part `done` of `total` is being written; the last step is the merge.
    Writing { done: usize, total: usize },
}

/// Ask once; a reasoning model can spend everything on thinking and leave the answer empty, so an
/// empty reply gets one retry with double the room.
/// Take out lines the model has already written, when it has gone round in a loop. Headings and blank
/// lines may repeat, and only list items and sentences of some length count, as `- 无` can fairly recur.
fn drop_repeated_lines(text: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    let kept: Vec<&str> = text
        .split('\n')
        .filter(|line| {
            let line = line.trim();
            line.starts_with('#') || line.chars().count() < 8 || seen.insert(line.to_string())
        })
        .collect();
    kept.join("\n")
}

fn ask(provider: &Provider, messages: &[Value], tokens: u32, cancel: &AtomicBool) -> Result<String> {
    let reply = provider.chat(messages, tokens)?;
    if !reply.trim().is_empty() {
        return Ok(drop_repeated_lines(&reply));
    }
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    let again = provider.chat(messages, (tokens * 2).min(8192))?;
    if again.trim().is_empty() {
        return Err(anyhow!("The model returned no text."));
    }
    Ok(drop_repeated_lines(&again))
}

/// Write the summary. Between requests it notices `cancel`; a request already sent cannot be recalled,
/// so the caller also ignores anything arriving after it cancelled.
pub fn summarize(provider: &Provider, boards: &[Graph], lines: &[Line], cancel: &AtomicBool, progress: &mut dyn FnMut(Step)) -> Result<String> {
    let english = is_english(lines.iter().map(|l| l.text.as_str()));
    let w = words(english);
    // A remote model may reason before writing, so it gets more room.
    let remote = matches!(provider, Provider::Remote { .. });
    let parts = chunks(lines, CHUNK);
    let text = transcript(lines, english);
    let mut said = format!("{}\n{}", w.transcript, if text.is_empty() { w.no_lines } else { &text });
    if parts.len() > 1 {
        let mut notes = vec![];
        for (i, part) in parts.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                bail!("cancelled");
            }
            progress(Step::Writing { done: i, total: parts.len() + 1 });
            let reply = ask(provider, &messages(boards, &transcript(part, english), true, english), if remote { 2048 } else { 600 }, cancel)?;
            notes.push(format!("{}\n{}", (w.part)(i + 1), reply.trim()));
        }
        said = format!("{}\n{}", w.too_long, notes.join("\n\n"));
    }
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    let total = if parts.len() > 1 { parts.len() + 1 } else { 1 };
    progress(Step::Writing { done: total - 1, total });
    let reply = ask(provider, &messages(boards, &said, false, english), if remote { 4096 } else { 1024 }, cancel)?;
    let tidied = no_letter_owners(&name_ids(&tidy(&reply, english), boards));
    if tidied.is_empty() {
        bail!("The model returned no summary.");
    }
    Ok(tidied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn goldens() -> Value {
        serde_json::from_str(include_str!("../../board/fixtures/goldens.json")).unwrap()
    }

    fn lines_of(v: &Value) -> Vec<Line> {
        Line::spoken(v.as_array().unwrap())
    }

    #[test]
    fn a_template_meeting_is_summed_up_by_its_topics() {
        let mut board = Graph::default();
        board.extra.insert("template".into(), json!(["做得好的", "要改进的", "行动项"]));
        let asked = messages(&[board], "A：上线很顺利", false, false);
        let system = asked[0]["content"].as_str().unwrap();
        assert!(system.contains("做得好的 / 要改进的 / 行动项") && system.contains("#### 话题"), "{system}");
        assert!(!messages(&[Graph::default()], "A：好", false, false)[0]["content"].as_str().unwrap().contains("模板"));
        // A topic part survives tidying and reads as a heading.
        let tidied = tidy("### 结论\n#### 做得好的\n- 上线顺利\n### 决定\n- 无", false);
        assert!(tidied.contains("#### 做得好的"), "{tidied}");
        assert_eq!(blocks(&tidied)[1], Block::Heading("做得好的".into()));
    }

    #[test]
    fn a_lettered_speaker_is_no_owner() {
        assert_eq!(no_letter_owners("- [ ] 写完重试逻辑（负责人：B）"), "- [ ] 写完重试逻辑");
        assert_eq!(no_letter_owners("- [ ] 缩小字号并验证效果（负责人：无）"), "- [ ] 缩小字号并验证效果");
        assert_eq!(no_letter_owners("- [ ] Check the font (owner: none)"), "- [ ] Check the font");
        assert_eq!(no_letter_owners("- [ ] Finish the retry logic (owner: A)"), "- [ ] Finish the retry logic");
        assert_eq!(no_letter_owners("- [ ] 写文档（负责人：王芳）"), "- [ ] 写文档（负责人：王芳）");
        assert_eq!(no_letter_owners("- [ ] Ship it (owner: Maya)"), "- [ ] Ship it (owner: Maya)");
    }

    #[test]
    fn node_ids_in_a_reply_become_their_text() {
        let mut g = Graph::default();
        g.apply(&boardcast_board::graph::parse_ops("T 「登录流程」\nL 「登录流程」 「短信验证」\nT 「Release checklist」"));
        let boards = [g];
        assert_eq!(name_ids("- 确认 n2 的超时", &boards), "- 确认 短信验证 的超时");
        assert_eq!(name_ids("- n2（短信验证）要加限流", &boards), "- 短信验证要加限流");
        assert_eq!(name_ids("- Ship n3 Release checklist first", &boards), "- Ship Release checklist first");
        assert_eq!(name_ids("- n1: 先画完", &boards), "- 登录流程: 先画完");
        assert_eq!(name_ids("- n2（超时问题）", &boards), "- 短信验证（超时问题）");
        // Not an id on this board, or not an id at all.
        assert_eq!(name_ids("- n99 and win32 and n2x stay", &boards), "- n99 and win32 and n2x stay");
        assert!(!Regex::new(r"\bn\d+\b").unwrap().is_match(&name_ids("### 结论\n- n1 已定，n2 待办", &boards)));
    }

    #[test]
    fn a_model_going_round_in_a_loop_is_cut_back_to_one_pass() {
        let looped = "### 待办\n- 固定切分在第四步切翻的原因\n- 提示词中语气词的处理逻辑\n- 无\n\n- 固定切分在第四步切翻的原因\n- 提示词中语气词的处理逻辑\n- 无\n### 待办\n- 另一件新的事情要做";
        assert_eq!(drop_repeated_lines(looped), "### 待办\n- 固定切分在第四步切翻的原因\n- 提示词中语气词的处理逻辑\n- 无\n\n- 无\n### 待办\n- 另一件新的事情要做");
    }

    #[test]
    fn everything_matches_the_typescript_engine_on_recorded_meetings() {
        let g = goldens();
        let cases = g["summaries"].as_array().unwrap();
        assert!(cases.len() >= 20);
        for case in cases {
            let at = case["file"].as_str().unwrap();
            let boards: Vec<Graph> = serde_json::from_value(case["boards"].clone()).unwrap();
            let lines = lines_of(&case["lines"]);
            let english = case["lang"] == "en";
            assert_eq!(fingerprint(&boards, &lines), case["fingerprint"], "fingerprint, {at}");
            assert_eq!(transcript(&lines, english), case["transcript"], "transcript, {at}");
            assert_eq!(boards_text(&boards, english), case["boardsText"], "board text, {at}");
            let said = format!("{}\n{}", words(english).transcript, case["transcript"].as_str().unwrap());
            assert_eq!(json!(messages(&boards, &said, false, english)), case["messages"], "messages, {at}");
            assert_eq!(json!(messages(&boards, case["transcript"].as_str().unwrap(), true, english)), case["partial"], "partial messages, {at}");
            let parts: Vec<usize> = chunks(&lines, 40).iter().map(Vec::len).collect();
            assert_eq!(json!(parts), case["chunks"], "chunks, {at}");
        }
    }

    #[test]
    fn replies_are_tidied_and_copied_as_the_typescript_engine_does() {
        let g = goldens();
        for case in g["tidy"].as_array().unwrap() {
            assert_eq!(tidy(case["input"].as_str().unwrap(), case["lang"] == "en"), case["output"], "{}", case["input"]);
        }
        for case in g["plain"].as_array().unwrap() {
            assert_eq!(plain_text(case["input"].as_str().unwrap()), case["output"], "{}", case["input"]);
        }
    }

    #[test]
    fn blocks_read_the_summary_and_todos_toggle_in_place() {
        let text = "### 结论\n- **用 Go**\n\n### 待办\n- [ ] 写爬虫（负责人：小王）\n- [x] 定方案\n说明";
        let read = blocks(text);
        assert_eq!(read[0], Block::Heading("结论".into()));
        assert_eq!(read[1], Block::Bullet("用 Go".into()));
        assert_eq!(read[3], Block::Todo { done: false, text: "写爬虫（负责人：小王）".into(), line: 4 });
        assert_eq!(read[4], Block::Todo { done: true, text: "定方案".into(), line: 5 });
        assert_eq!(read[5], Block::Paragraph("说明".into()));
        let ticked = toggle_todo(text, 4);
        assert!(ticked.contains("- [x] 写爬虫"));
        assert_eq!(toggle_todo(&ticked, 4), text);
        assert!(toggle_todo(text, 5).contains("- [ ] 定方案"));
        assert_eq!(toggle_todo(text, 0), text, "only a to-do can be ticked");
        assert_eq!(toggle_todo(text, 99), text);
    }

    fn line(text: &str) -> Line {
        Line { speaker: None, text: text.into(), page: 0, has_page: true }
    }

    fn scripted(reply: impl Fn(&str) -> Result<String> + Send + Sync + 'static) -> Provider {
        Provider::Scripted(Arc::new(reply))
    }

    #[test]
    fn a_short_meeting_takes_one_request() {
        let calls = Arc::new(Mutex::new(0));
        let count = calls.clone();
        let provider = scripted(move |_| {
            *count.lock().unwrap() += 1;
            Ok("### 结论\n- 好".into())
        });
        let out = summarize(&provider, &[Graph::default()], &[line("先做登录")], &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(*calls.lock().unwrap(), 1);
        assert!(out.contains("### 待讨论\n- 无"));
    }

    #[test]
    fn a_long_meeting_is_noted_part_by_part_then_merged() {
        let seen = Arc::new(Mutex::new(vec![]));
        let log = seen.clone();
        let provider = scripted(move |said| {
            log.lock().unwrap().push(said.to_string());
            Ok("### 结论\n- 好".into())
        });
        let long: Vec<Line> = (0..3).map(|i| line(&format!("{i}{}", "长".repeat(CHUNK - 2)))).collect();
        let mut steps = vec![];
        summarize(&provider, &[Graph::default()], &long, &AtomicBool::new(false), &mut |Step::Writing { done, total }| steps.push((done, total))).unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 4, "three parts and the merge");
        assert!(seen[3].contains("逐段记下的要点"));
        assert_eq!(steps, [(0, 4), (1, 4), (2, 4), (3, 4)]);
    }

    #[test]
    fn an_empty_reply_gets_one_retry_then_an_error() {
        let calls = Arc::new(Mutex::new(0));
        let count = calls.clone();
        let provider = scripted(move |_| {
            *count.lock().unwrap() += 1;
            Ok(String::new())
        });
        assert!(summarize(&provider, &[Graph::default()], &[line("先做登录")], &AtomicBool::new(false), &mut |_| {}).is_err());
        assert_eq!(*calls.lock().unwrap(), 2);
    }

    #[test]
    fn cancelling_stops_before_the_next_request() {
        let calls = Arc::new(Mutex::new(0));
        let count = calls.clone();
        let provider = scripted(move |_| {
            *count.lock().unwrap() += 1;
            Ok("x".into())
        });
        let long: Vec<Line> = (0..3).map(|i| line(&format!("{i}{}", "长".repeat(CHUNK - 2)))).collect();
        assert!(summarize(&provider, &[Graph::default()], &long, &AtomicBool::new(true), &mut |_| {}).is_err());
        assert_eq!(*calls.lock().unwrap(), 0);
    }
}
