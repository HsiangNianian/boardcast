//! The meeting summary, ported from src/engine/summary.ts: conclusions, decisions, to-dos and open
//! questions. A long meeting is noted part by part, then merged with the board.
//! The Markdown here is what a meeting stores, so `blocks` reads it for display and to tick to-dos.

use crate::Provider;
use anyhow::{anyhow, bail, ensure, Result};
use boardcast_board::graph::{is_english, Graph, Kind};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

/// The most text in one part; a small local model's context is limited.
const CHUNK: usize = 3000;

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
    partial: "你在帮人整理会议记录。这是一场长会议中的一段，记下这一段里的要点：说了什么结论、定了什么、谁要做什么、还有什么没定。每条一行，以「- 」开头。这只是部分材料，没看到的内容不等于未解决的问题。只记录本段明确提出的未决事项，不因缺少上下文而补问题。保留事实所属的主体、组别和日期，不从片段编号推断板号或组别，也不自行合计数量。",
    system: "你在帮人写会议摘要。",
    facts: "保留原话的主体、否定和进展，区分已有成果与后续工作。\n只把明确达成的共识列为决定，把建议、希望和未定事项放在待讨论。\n结论和决定只写会上确认过的事；只是猜测的（「应该……吧」「是不是……」），哪怕说的是数量或标准，也只写进待讨论。\n一个问题后面有人回答、大家接受了（如「之后怎么办？」「就缩小字号」「好」），就把答案写进结论或决定，这个问题从待讨论里去掉，也不要换个说法再问一遍。\n只是「也许」「可能」「得验证一下」的回答不算定下来：这个问题照原样留在待讨论，可写上提到的办法。\n会上明确同意要做的选择（「同意」「就用」「改用」「定了」）写进决定，撤回的方案不写成决定。\n只把明确承诺或分配的工作列为待办，保留已说明的负责人和日期，不自行补充。说话人是 A、B、C 这样的字母时，那是自动编号，不是人名：摘要不把字母当成人名；待办仍完整保留事情和已说明的截止时间，只去掉负责人。\n只总结会议记录（或分段笔记）与白板文字中明确写出的事实，包括手动修正和批注；不从划线等标记推测决定。\n按发言顺序处理确认、否定和撤回；较晚的明确回答优先于白板上的旧说法。问题只回答了一部分时，记录已确认的部分，剩余部分仍放待讨论。例如：已确认周五发布，但上线负责人下次讨论，应写决定「周五发布」、待讨论「上线负责人是谁」，不能只写待办「确定上线负责人」而清空待讨论。未确定的事项不自动变成待办，当场已经做过的操作也不变成会后任务。",
    rules: "只根据给出的内容写，不要编造；下面四个主栏目没有内容时写「无」。\n用简体中文，每条一行，以「- 」开头，简短。\n待办每条以「- [ ] 」开头，保留事情、已说明的截止时间和明确的人名；发言编号不写成负责人。没有时间或人名就省略，不填代称。\n只在记录明确标注「第 N 张板」时引用板号，不把不同话题当成不同板。\n严格按下面的格式输出，不要别的话：",
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
    partial: "You are helping someone tidy up meeting notes. This is one part of a long meeting; note its key points: conclusions reached, what was decided, who will do what, and what is still open. One point per line, starting with '- '. This is only part of the source: missing context is not an unresolved question. Record only explicitly raised open issues; do not invent questions about unseen material. Preserve the subject, group and date of each fact; a fragment number is not a board or group number. Do not calculate additional totals.",
    system: "You are helping someone write a meeting summary.",
    facts: "Preserve subjects, negations and progress; distinguish existing work from future work.\nPut only explicit agreements in Decisions; keep suggestions, hopes and unresolved matters under Open questions.\nConclusions and Decisions hold only what the meeting confirmed; a guess (\"it should be two, right?\"), even about a number or a criterion, goes under Open questions.\nA question that is answered and accepted later in the meeting (\"what then?\" \"then we shrink the font\" \"good\") is settled: put the answer under Conclusions or Decisions and leave the question out of Open questions, not even reworded.\nAn answer given as a maybe (\"perhaps\", \"we could\", \"needs checking\") settles nothing: keep the question as asked under Open questions, with the idea mentioned.\nA choice the meeting explicitly agreed on (\"agreed\", \"let's go with\", \"we'll use\") goes under Decisions; a withdrawn plan is no decision.\nList only explicitly promised or assigned work as To-dos, retaining stated owners and dates without inventing them. Speakers named by a single letter such as A, B or C are numbered automatically, not named: never use those letters as people's names in the summary. Keep the entire task and its stated deadline, omitting only the owner.\nUse explicitly stated facts from the transcript (or its notes) and whiteboard text, including manual corrections and annotations; do not infer decisions from marks such as strikethroughs.\nFollow the order of the transcript through confirmations, denials and withdrawals; a later explicit answer takes precedence over an older board statement. A partial answer settles only that part: keep the unanswered part under Open questions. For example, if Friday is agreed for launch but its owner is left for the next discussion, Decisions says \"Launch on Friday\" and Open questions asks \"Who owns the launch?\"; a To-do to choose the owner cannot replace that open question. An unresolved question is not automatically a To-do, nor is an action already done during the meeting.",
    rules: "Write only from what is given, never make things up; write \"None\" only for an empty main section among the four below.\nWrite in English, one short point per line, starting with \"- \".\nStart each To-do with \"- [ ] \", retaining the task, its stated deadline and explicitly named people. Omit missing dates or names; do not substitute speaker numbers or unnamed roles.\nMention a board number only when explicitly labeled in the source; do not infer board numbers from topics.\nOutput exactly the format below and nothing else:",
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

// Automatic speaker letters are not names. Leave them out of the model input, keeping the saved transcript intact.
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
        let text = l.speaker.as_ref().filter(|s| !(s.len() == 1 && s.as_bytes()[0].is_ascii_uppercase())).map_or_else(|| l.text.clone(), |s| format!("{s}{}{}", w.colon, l.text));
        out.push(format!("> {}", text.replace('\n', "\n> ")));
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
    let topics = template_topics(std::slice::from_ref(g));
    let mut out = g.clone();
    // Blank template topics are an agenda, not statements made in the meeting.
    out.nodes.retain(|n| !(n.kind == Kind::Topic && topics.contains(&n.label) && n.marks.is_empty() && !touched.contains(n.id.as_str())));
    out.nodes.retain(|n| n.kind != Kind::Region || !n.marks.is_empty() || n.extra.get("spans").and_then(Value::as_array).is_some_and(|s| !s.is_empty()) || touched.contains(n.id.as_str()));
    out.describe()
}

fn boards_text(boards: &[Graph], english: bool) -> String {
    let w = words(english);
    let live: Vec<&Graph> = boards.iter().filter(|g| !g.nodes.is_empty()).collect();
    if live.len() <= 1 {
        return board_text(live.first().copied().or(boards.first()).unwrap_or(&Graph::default()));
    }
    let parts: Vec<String> = boards
        .iter()
        .enumerate()
        .filter(|(_, g)| !g.nodes.is_empty())
        .map(|(i, g)| format!("{}{}\n{}", w.board.replace("{}", &(i + 1).to_string()), w.colon.trim(), board_text(g)))
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
        format!("Keep the four sections above. Within Conclusions, group the confirmed facts under this meeting's template topics: {}. Write each discussed topic as a separate heading line \"#### topic\", followed by its facts on separate bullet lines. A topic's name on the board alone is not evidence it was discussed. Omit undiscussed topics from every section, including Open questions, headings and empty placeholders.", topics.join(" / "))
    } else {
        format!("保留上面的四个栏目。这场会按模板开，话题是：{}。仅在「结论」里，将已确认的事实按讨论过的话题分小节。每个「#### 话题」必须独占一行，事实另起一行以「- 」开头。板上只有话题名称不算讨论过；没讨论过的话题在任何栏目都不要写，不要凭模板添加待讨论事项，也不要填「无」。", topics.join(" / "))
    })
}

fn messages(boards: &[Graph], said: &str, partial: bool, english: bool) -> Vec<Value> {
    let w = words(english);
    let structure = template_rule(&template_topics(boards), english).map_or_else(String::new, |r| format!("\n{r}"));
    let rules = format!("{}\n{}{structure}", w.rules, w.sections.map(|s| format!("### {s}")).join("\n"));
    let system = if partial { format!("{}\n{}", w.partial, w.facts) } else { format!("{}\n{}\n{rules}", w.system, w.facts) };
    let user = if partial { format!("{}\n{said}", w.fragment) } else { format!("{}\n{}\n\n{said}", w.board_intro, boards_text(boards, english)) };
    vec![json!({ "role": "system", "content": system }), json!({ "role": "user", "content": user })]
}

/// An unused template topic sometimes survives as a heading followed only by "None".
fn no_empty_topics(text: &str, topics: &[String], english: bool) -> String {
    if topics.is_empty() { return text.to_string(); }
    let rows: Vec<&str> = text.lines().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < rows.len() {
        let line = rows[i];
        if line.strip_prefix("#### ").is_some_and(|t| topics.iter().any(|topic| topic == t.trim())) {
            let end = (i + 1..rows.len()).find(|&j| rows[j].starts_with('#')).unwrap_or(rows.len());
            if rows[i + 1..end].iter().all(|l| matches!(l.trim(), "" | "None" | "- None" | "无" | "- 无")) {
                i = end;
                continue;
            }
        }
        out.push(line);
        i += 1;
    }
    for section in words(english).sections {
        let heading = format!("### {section}");
        if let Some(start) = out.iter().position(|l| *l == heading) {
            let end = (start + 1..out.len()).find(|&j| out[j].starts_with("### ")).unwrap_or(out.len());
            if out[start + 1..end].iter().all(|l| l.trim().is_empty()) {
                out.insert(start + 1, if english { "- None" } else { "- 无" });
            }
        }
    }
    out.join("\n")
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
    let mut section = String::new();
    let kept: Vec<&str> = text
        .split('\n')
        .filter(|line| {
            let line = line.trim();
            if line.starts_with('#') { section = line.to_string(); return true; }
            line.chars().count() < 8 || seen.insert((section.clone(), line.to_string()))
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
    let mut retry = (tokens * 2).min(8192);
    if let Some(capacity) = provider.context_size()? {
        retry = retry.min(u32::try_from(capacity.saturating_sub(provider.prompt_tokens(messages)?)).unwrap_or(u32::MAX));
        ensure!(retry > 0, "No room for a summary reply in the local model's context.");
    }
    let again = provider.chat(messages, retry)?;
    if again.trim().is_empty() {
        return Err(anyhow!("The model returned no text."));
    }
    Ok(drop_repeated_lines(&again))
}

/// A collection of boards or intermediate notes can outgrow the context even after transcript chunking.
fn compact(provider: &Provider, request: &mut [Value], output: u32, capacity: usize, english: bool, preserve: &str, cancel: &AtomicBool, progress: &mut dyn FnMut(Step)) -> Result<()> {
    let w = words(english);
    let mut size = provider.prompt_tokens(request)?;
    if size.saturating_add(output as usize) <= capacity { return Ok(()); }
    let mut fixed = request.to_vec();
    fixed[1]["content"] = json!(preserve);
    // Keep a short original transcript verbatim; second-hand notes can change a question's certainty.
    let preserve = if provider.prompt_tokens(&fixed)?.saturating_add(output as usize + 600) <= capacity { preserve } else { "" };
    while size.saturating_add(output as usize) > capacity {
        if cancel.load(Ordering::Relaxed) { bail!("cancelled"); }
        let source = request[1]["content"].as_str().expect("summary user text").strip_suffix(preserve).expect("original transcript suffix").to_string();
        let mut empty = request.to_vec();
        empty[1]["content"] = json!(preserve);
        ensure!(provider.prompt_tokens(&empty)?.saturating_add(output as usize) < capacity,
            "The summary instructions alone exceed the local model's context. Shorten the meeting template.");
        let mut batches = vec![];
        let mut rest = source.as_str();
        while !rest.is_empty() {
            if cancel.load(Ordering::Relaxed) { bail!("cancelled"); }
            let mut end = rest.len();
            let mut part = messages(&[], "", true, english);
            loop {
                part[1]["content"] = json!(format!("{}\n{}", w.fragment, &rest[..end]));
                if provider.prompt_tokens(&part)?.saturating_add(600) <= capacity { break; }
                end = rest[..end].char_indices().nth(rest[..end].chars().count() / 2).map_or(0, |(i, _)| i);
                ensure!(end > 0, "A summary fragment cannot fit in the local model's context.");
                // Keep complete source lines together where possible; never drop a UTF-8 fragment.
                if let Some(at) = rest[..end].rfind("\n\n").filter(|i| *i >= end / 2) { end = at + 2; }
                else if let Some(at) = rest[..end].rfind('\n').filter(|i| *i > 0) { end = at + 1; }
            }
            batches.push(part);
            rest = &rest[end..];
        }
        let mut notes = vec![];
        for (i, batch) in batches.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) { bail!("cancelled"); }
            progress(Step::Writing { done: i, total: batches.len() + 1 });
            let note = ask(provider, batch, 600, cancel)?;
            notes.push(format!("{}\n{}", (w.part)(i + 1), note));
        }
        request[1]["content"] = json!(format!("{}\n{}\n\n{preserve}", w.too_long, notes.join("\n\n")));
        let reduced = provider.prompt_tokens(request)?;
        ensure!(reduced < size, "The model did not shorten the meeting notes enough to fit its context.");
        size = reduced;
        progress(Step::Writing { done: batches.len(), total: batches.len() + 1 });
    }
    Ok(())
}

/// Write the summary. Between requests it notices `cancel`; a request already sent cannot be recalled,
/// so the caller also ignores anything arriving after it cancelled.
pub fn summarize(provider: &Provider, boards: &[Graph], lines: &[Line], cancel: &AtomicBool, progress: &mut dyn FnMut(Step)) -> Result<String> {
    if cancel.load(Ordering::Relaxed) { bail!("cancelled"); }
    let english = is_english(lines.iter().map(|l| l.text.as_str()));
    let w = words(english);
    let capacity = provider.context_size()?;
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
            let tokens = if remote { 2048 } else { 600 };
            let mut request = messages(boards, &transcript(part, english), true, english);
            if let Some(capacity) = capacity { compact(provider, &mut request, tokens, capacity, english, "", cancel, progress)?; }
            let reply = ask(provider, &request, tokens, cancel)?;
            notes.push(format!("{}\n{}", (w.part)(i + 1), reply.trim()));
        }
        said = format!("{}\n{}", w.too_long, notes.join("\n\n"));
    }
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    let total = if parts.len() > 1 { parts.len() + 1 } else { 1 };
    progress(Step::Writing { done: total - 1, total });
    let tokens = if remote { 4096 } else { 1024 };
    let mut request = messages(boards, &said, false, english);
    if let Some(capacity) = capacity { compact(provider, &mut request, tokens, capacity, english, if parts.len() == 1 { &said } else { "" }, cancel, progress)?; }
    if cancel.load(Ordering::Relaxed) { bail!("cancelled"); }
    let reply = ask(provider, &request, tokens, cancel)?;
    let tidied = no_empty_topics(&no_letter_owners(&name_ids(&tidy(&reply, english), boards)), &template_topics(boards), english);
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
    fn only_empty_template_placeholders_are_removed() {
        let topics = vec!["Latency".into(), "Hiring".into()];
        let text = "### Conclusions\n#### Latency\n- No alerts fired.\n#### Hiring\n- None\n\n### Decisions\n- None";
        let cleaned = no_empty_topics(text, &topics, true);
        assert!(cleaned.contains("#### Latency\n- No alerts fired."));
        assert!(!cleaned.contains("#### Hiring"));
        assert!(cleaned.contains("### Decisions\n- None"));
        assert_eq!(no_empty_topics("### Conclusions\n#### Hiring\n- None", &topics, true), "### Conclusions\n- None");
        assert_eq!(no_empty_topics(text, &[], true), text);
        let real = text.replace("#### Hiring\n- None", "#### Hiring\n- No hiring this quarter.");
        assert_eq!(no_empty_topics(&real, &topics, true), real);
    }

    #[test]
    fn empty_template_topics_are_not_revived_in_other_sections() {
        let topics = vec!["预算".into(), "要改进的".into()];
        let text = "### 结论\n- 发布没有停机。\n### 决定\n#### 预算\n无\n### 待办\n#### 要改进的\n- [ ] 小王补上告警测试。\n#### 预算\n- 无\n### 待讨论\n#### 预算\n无";
        let cleaned = no_empty_topics(text, &topics, false);
        assert_eq!(cleaned, "### 结论\n- 发布没有停机。\n### 决定\n- 无\n### 待办\n#### 要改进的\n- [ ] 小王补上告警测试。\n### 待讨论\n- 无");
    }

    #[test]
    fn repeated_facts_in_different_sections_do_not_empty_a_decision() {
        let text = "### Conclusions\n- Cache lifetime is ten minutes.\n### Decisions\n- Cache lifetime is ten minutes.\n- Cache lifetime is ten minutes.";
        assert_eq!(drop_repeated_lines(text), "### Conclusions\n- Cache lifetime is ten minutes.\n### Decisions\n- Cache lifetime is ten minutes.");
    }

    #[test]
    fn a_blank_template_is_not_evidence_of_discussion() {
        let mut graph: Graph = serde_json::from_value(json!({"nodes":[{"id":"n1","kind":"topic","label":"Budget"}],"template":["Budget"]})).unwrap();
        assert!(!board_text(&graph).contains("Budget"));
        graph.nodes[0].marks.push("confirmed".into());
        assert!(board_text(&graph).contains("Budget"));
    }

    #[test]
    fn board_material_keeps_the_tail_for_context_aware_summarization() {
        let nodes: Vec<Value> = (0..100).map(|i| json!({"id":format!("n{i}"),"kind":"item","label":format!("检查项目{i}已经完成，后续由小林核对。") })).collect();
        let board: Graph = serde_json::from_value(json!({"nodes":nodes})).unwrap();
        let text = boards_text(&[board.clone(), board], false);
        assert!(text.len() > 3000 && text.contains("第 2 张板"));
        assert_eq!(text.matches("检查项目99已经完成").count(), 2);
    }

    #[test]
    fn summary_input_omits_automatic_letters_but_keeps_names_dates_and_source_lines() {
        let source = json!([{"speaker":"A","text":"我周五前写接口文档。"},{"speaker":"小王","text":"我周四补测试。"}]);
        let lines = Line::spoken(source.as_array().unwrap());
        assert_eq!(transcript(&lines, false), "> 我周五前写接口文档。\n> 小王：我周四补测试。");
        assert_eq!(lines[0].speaker.as_deref(), Some("A"));
        assert_eq!(lines[0].text, "我周五前写接口文档。");
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
    fn recorded_meetings_keep_their_fingerprints_and_current_summary_inputs() {
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
    fn oversized_material_is_all_read_in_order_before_the_final_summary() {
        let seen = Arc::new(Mutex::new(vec![]));
        let log = seen.clone();
        let provider = scripted(move |said| {
            let source = said.strip_prefix("会议记录（片段）：\n").unwrap();
            log.lock().unwrap().push(source.to_string());
            Ok(source.lines().filter_map(|l| l.split_once(':').map(|(id, _)| id)).collect::<Vec<_>>().join("\n"))
        });
        let source = (0..40).map(|i| format!("事实{i}:{}\n", "演练结果🦀".repeat(20))).collect::<String>();
        let mut request = messages(&[], &source, false, false);
        let original = request[1]["content"].as_str().unwrap().to_string();
        let system = request[0].clone();
        let mut steps = vec![];
        compact(&provider, &mut request, 1024, 4096, false, "", &AtomicBool::new(false), &mut |Step::Writing { done, total }| steps.push((done, total))).unwrap();
        let read = seen.lock().unwrap();
        assert!(read.len() > 1);
        assert_eq!(read.join(""), original, "every source character reaches exactly one first-pass request");
        assert_eq!(request[0], system, "retain the final instructions and template rule");
        assert!(provider.prompt_tokens(&request).unwrap() + 1024 <= 4096);
        let final_text = request[1]["content"].as_str().unwrap();
        let mut last = 0;
        for i in 0..40 {
            let at = final_text.find(&format!("事实{i}\n")).or_else(|| final_text.strip_suffix(&format!("事实{i}")).map(str::len)).unwrap();
            assert!(at >= last); last = at;
        }
        assert_eq!(steps.last(), Some(&(read.len(), read.len() + 1)));
    }

    #[test]
    fn context_reduction_can_be_cancelled_and_must_actually_shorten_the_notes() {
        let cancel = Arc::new(AtomicBool::new(false));
        for stop in [true, false] {
            let cancellation = cancel.clone();
            let calls = Arc::new(Mutex::new(0));
            let count = calls.clone();
            let provider = scripted(move |said| {
                *count.lock().unwrap() += 1;
                cancellation.store(stop, Ordering::Relaxed);
                Ok(said.to_string())
            });
            cancel.store(false, Ordering::Relaxed);
            let mut request = messages(&[], &"完整材料🦀\n".repeat(1000), false, false);
            let error = compact(&provider, &mut request, 1024, 4096, false, "", &cancel, &mut |_| {}).unwrap_err();
            if stop {
                assert_eq!(*calls.lock().unwrap(), 1);
                assert_eq!(error.to_string(), "cancelled");
            } else {
                assert!(error.to_string().contains("did not shorten"));
            }
        }
    }

    #[test]
    fn a_short_original_transcript_is_not_replaced_with_board_notes() {
        let transcript = "会议记录：\n> If deployment fails, should we roll back?\n> That is still undecided.";
        let seen = Arc::new(Mutex::new(vec![]));
        let log = seen.clone();
        let provider = scripted(move |said| {
            assert!(!said.contains("should we roll back"));
            log.lock().unwrap().push(said.strip_prefix("会议记录（片段）：\n").unwrap().to_string());
            Ok("- 板书中的检查均已完成。".into())
        });
        let source = format!("白板上的内容：\n{}\n\n{transcript}", "检查已完成。\n".repeat(2000));
        let mut request = messages(&[], "", false, false);
        request[1]["content"] = json!(source);
        compact(&provider, &mut request, 1024, 4096, false, transcript, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(seen.lock().unwrap().join("") + transcript, source);
        assert!(request[1]["content"].as_str().unwrap().ends_with(transcript));
        assert!(provider.prompt_tokens(&request).unwrap() + 1024 <= 4096);
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
    fn an_empty_local_reply_retries_only_within_the_remaining_context() {
        use std::io::{BufRead, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for (path, response, maximum) in [
                ("/v1/chat/completions", json!({"choices":[{"message":{"content":""}}]}), Some(1024)),
                ("/props", json!({"default_generation_settings":{"n_ctx":4096}}), None),
                ("/apply-template", json!({"prompt":"rendered source"}), None),
                ("/tokenize", json!({"tokens":vec![1; 2400]}), None),
                ("/v1/chat/completions", json!({"choices":[{"message":{"content":"The source was retained."}}]}), Some(1696)),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
                let mut reader = std::io::BufReader::new(&mut stream);
                let mut first = String::new(); reader.read_line(&mut first).unwrap();
                assert!(first.contains(path));
                let mut size = 0;
                loop {
                    let mut line = String::new(); reader.read_line(&mut line).unwrap();
                    if line == "\r\n" { break; }
                    if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") { size = n.trim().parse::<usize>().unwrap(); }
                }
                let mut body = vec![0; size]; reader.read_exact(&mut body).unwrap();
                if let Some(maximum) = maximum {
                    let value: Value = serde_json::from_slice(&body).unwrap();
                    assert_eq!(value["max_tokens"], maximum);
                }
                let body = response.to_string();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        let request = vec![json!({"role":"user","content":"Keep the source."})];
        assert_eq!(ask(&Provider::Local { base }, &request, 1024, &AtomicBool::new(false)).unwrap(), "The source was retained.");
        server.join().unwrap();
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
