//! Consolidating the board: line by line the model can only add where a line seems to belong, so a free
//! discussion piles up in the order it was said, every remark one more item. Now and then the model is shown
//! the whole board and the discussion so far, and writes the board again as a clear outline: each position
//! with its reasons, the conclusion starred, what was turned down struck, what is still open as a question.
//!
//! The model writes the outline rather than operations: rewriting is easier for it than working out dozens
//! of moves. Each line it writes is matched to the node it was before, by label or alias, so the board keeps
//! its ids and who raised what, and ink that moves slides instead of being drawn again.
//!
//! Only outline boards are consolidated; a flow chart, with its plain nodes, is left as it is. A few links drawn
//! between points of an outline stay, as long as both ends are still on the board.

use crate::{Provider, Utterance};
use anyhow::{bail, Result};
use alcyo_board::graph::{Graph, Kind, Node, NodeData};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

/// How much of the discussion the model sees: the latest lines, which the board already sums up before them.
pub const HISTORY_LINES: usize = 80;

/// Tokens a consolidation may use, thinking included: a ceiling against a model running on, not a budget. A
/// tight one cut the outline off; what a consolidation costs is set by how often it runs and how hard the
/// model thinks.
const ROOM: u32 = 32768;

const MARKS: [&str; 5] = ["划掉", "圈出", "下划线", "星标", "高亮"];

/// The instructions, kept as files beside the board-layout skill so they are edited as text, not as code
/// (skills/board-consolidate): what every meeting shares, then how to organise each kind of meeting, chosen
/// by a short question first.
const BASE: [&str; 2] = [include_str!("../skills/board-consolidate/base.zh.md"), include_str!("../skills/board-consolidate/base.en.md")];
const DISCUSSION: [&str; 2] = [include_str!("../skills/board-consolidate/discussion.zh.md"), include_str!("../skills/board-consolidate/discussion.en.md")];
const ASSIGNMENT: [&str; 2] = [include_str!("../skills/board-consolidate/assignment.zh.md"), include_str!("../skills/board-consolidate/assignment.en.md")];
const GENERAL: [&str; 2] = [include_str!("../skills/board-consolidate/general.zh.md"), include_str!("../skills/board-consolidate/general.en.md")];
const CLASSIFY: [&str; 2] = [include_str!("../skills/board-consolidate/classify.zh.md"), include_str!("../skills/board-consolidate/classify.en.md")];

/// What kind of meeting the part being consolidated is, which decides how it is organised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Arguing, comparing options and deciding: positions, conclusions, what was dropped.
    Discussion,
    /// Dividing work and planning: by person or by step.
    Assignment,
    /// Anything else, or hard to tell: the board's own organisation, tidied.
    General,
}

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Self::Discussion => "discussion",
            Self::Assignment => "assignment",
            Self::General => "general",
        }
    }

    /// The instructions for this kind of meeting: what every meeting shares, then its own way.
    pub fn system(self, english: bool) -> String {
        let at = usize::from(english);
        let own = match self {
            Self::Discussion => DISCUSSION[at],
            Self::Assignment => ASSIGNMENT[at],
            Self::General => GENERAL[at],
        };
        format!("{}\n{own}", BASE[at])
    }

    /// From the classifier's answer; anything unclear is general.
    fn from_answer(answer: &str) -> Self {
        let answer = answer.to_lowercase();
        if answer.contains("assignment") {
            Self::Assignment
        } else if answer.contains("discussion") {
            Self::Discussion
        } else {
            Self::General
        }
    }
}

/// One line of the outline the model wrote.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub depth: usize,
    pub kind: Kind,
    pub label: String,
    pub marks: Vec<String>,
    pub by: Option<String>,
}

/// Whether the board is an outline to consolidate: it has a topic, holds no imported page, and is mostly
/// points rather than a flow chart. A stray flow node, as a link drawn to a new name makes one, does not stop
/// it; a flow node the outline leaves out is kept as it was.
pub fn applies_to(graph: &Graph) -> bool {
    let flow = graph.nodes.iter().filter(|n| n.kind == Kind::Node).count();
    graph.nodes.iter().any(|n| n.kind == Kind::Topic)
        && graph.nodes.iter().all(|n| matches!(n.kind, Kind::Topic | Kind::Item | Kind::Note | Kind::Question | Kind::Node))
        && flow * 5 <= graph.nodes.len().max(5)
}

/// The outline the model wrote, line by line. A line it could not read is left out.
pub fn parse(text: &str) -> Vec<Line> {
    let mut lines = vec![];
    for raw in text.lines() {
        // Tabs and full-width spaces indent as two spaces; a list bullet in front of the tag is not part of it.
        let raw = raw.replace(['\t', '\u{3000}'], "  ");
        let indent = raw.chars().take_while(|c| *c == ' ').count();
        let rest = raw.trim().trim_start_matches(['-', '*', '•']).trim_start();
        let Some((tag, label)) = rest.split_once([' ', '：', ':']) else { continue };
        let kind = match tag.trim() {
            "T" => Kind::Topic,
            "L" | "-" => Kind::Item,
            "K" => Kind::Note,
            "Q" => Kind::Question,
            _ => continue,
        };
        let mut label = label.trim().to_string();
        let mut marks = vec![];
        let mut by = None;
        // Marks and who raised it, from the end of the line, in any order.
        loop {
            if let Some(start) = label.rfind('[').filter(|_| label.ends_with(']')) {
                let mark = label[start + 1..label.len() - 1].to_string();
                if MARKS.contains(&mark.as_str()) {
                    if !marks.contains(&mark) {
                        marks.insert(0, mark);
                    }
                    label = label[..start].trim_end().to_string();
                    continue;
                }
            }
            // One speaker or several, as models write them: "@B", "@C@D", "@A & B", "@A、C"; an "@" inside a
            // word, as in an address, is not one.
            let speakers = |tail: &str| -> Option<Vec<String>> {
                let names: Vec<String> = tail.split(['@', '&', ',', '，', '、', '/', ' ']).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).collect();
                (!names.is_empty() && !tail.contains(['[', ']']) && names.iter().all(|n| n.chars().count() <= 16)).then_some(names)
            };
            if let Some((at, names)) = label.char_indices().filter(|&(i, c)| c == '@' && label[..i].ends_with(char::is_whitespace)).find_map(|(i, _)| speakers(&label[i..]).map(|n| (i, n))) {
                by = Some(names.join("、"));
                label = label[..at].trim_end().to_string();
                continue;
            }
            break;
        }
        if label.is_empty() {
            continue;
        }
        lines.push(Line { depth: indent / 2, kind, label, marks, by });
    }
    lines
}

fn normal(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The board the outline describes, keeping each node that was on the board before under its id, with who
/// raised it, its ink and what else it carried. A topic is always at the top; a line deeper than the one
/// before it goes under that one.
pub fn rebuild(old: &Graph, outline: &[Line]) -> Result<Graph> {
    if !outline.iter().any(|l| l.kind == Kind::Topic) {
        bail!("the outline has no topic");
    }
    // Everything else the board keeps stays as it was; only its nodes are written again.
    let mut graph = old.clone();
    graph.nodes.clear();
    graph.groups.clear();
    let mut used: HashSet<String> = HashSet::new();
    // The node at each depth so far, so a line can find its parent.
    let mut stack: Vec<String> = vec![];
    for line in outline {
        let depth = if line.kind == Kind::Topic { 0 } else { line.depth.clamp(1, stack.len().max(1)) };
        stack.truncate(depth);
        let parent = stack.last().cloned();
        if line.kind != Kind::Topic && parent.is_none() {
            continue;
        }
        let key = normal(&line.label);
        let before = old.nodes.iter().find(|n| !used.contains(&n.id) && (normal(&n.label) == key || n.aliases.iter().any(|a| normal(a) == key)));
        let node = match before {
            Some(n) => {
                let mut data: NodeData = (**n).clone();
                if data.label != line.label {
                    // The model's wording replaces the old one, which stays findable as an alias.
                    if !data.aliases.contains(&data.label) {
                        data.aliases.push(data.label.clone());
                    }
                    data.label = line.label.clone();
                }
                data.kind = line.kind;
                data.parent = parent;
                data.marks = line.marks.clone();
                data.group = None;
                data
            }
            None => {
                let id = format!("n{}", graph.next_node);
                graph.next_node += 1;
                let mut extra = Map::new();
                if let Some(by) = &line.by {
                    extra.insert("by".into(), Value::String(by.clone()));
                }
                NodeData { id, label: line.label.clone(), aliases: vec![], kind: line.kind, parent, group: None, marks: line.marks.clone(), icon: None, ink: "ink".into(), extra }
            }
        };
        used.insert(node.id.clone());
        stack.push(node.id.clone());
        graph.nodes.push(Node::new(node));
    }
    // Flow nodes the outline left out stay as they were, with their links, when they belong to a flow chart:
    // linked to another flow node. One hanging off a point, as a link to a new name makes, was folded into
    // the outline and goes. Links between points stay while both ends do.
    let is_flow = |id: &str| old.nodes.iter().any(|n| n.id == id && n.kind == Kind::Node);
    let in_flow = |id: &str| old.edges.iter().any(|e| (e.from == id && is_flow(&e.to)) || (e.to == id && is_flow(&e.from)));
    // A chain the outline wrote out in its own words, every node's name inside one of its lines ("让利 ->
    // 抢占份额 -> 提价" as "先让利抢占份额，份额稳定后提价"), was a point drawn as a flow: it goes, or the board
    // shows it twice. A chain with any node the outline does not mention is a flow chart, and stays.
    let written: Vec<String> = outline.iter().map(|l| normal(&l.label)).collect();
    let mentioned = |n: &Node| {
        let name = normal(&n.label);
        !name.is_empty() && written.iter().any(|w| w.contains(&name))
    };
    let mut folded: HashSet<String> = HashSet::new();
    for n in old.nodes.iter().filter(|n| n.kind == Kind::Node && !used.contains(&n.id)) {
        if folded.contains(&n.id) {
            continue;
        }
        // The flow nodes linked to this one, directly or through others.
        let mut chain = vec![n.id.clone()];
        let mut at = 0;
        while at < chain.len() {
            let id = chain[at].clone();
            for e in &old.edges {
                let other = if e.from == id { &e.to } else if e.to == id { &e.from } else { continue };
                if is_flow(other) && !used.contains(other) && !chain.contains(other) {
                    chain.push(other.clone());
                }
            }
            at += 1;
        }
        if chain.len() > 1 && chain.iter().all(|id| old.nodes.iter().any(|m| &m.id == id && mentioned(m))) {
            folded.extend(chain);
        }
    }
    for n in &old.nodes {
        if n.kind == Kind::Node && !used.contains(&n.id) && !folded.contains(&n.id) && in_flow(&n.id) {
            let mut kept = n.clone();
            kept.parent = kept.parent.clone().filter(|p| used.contains(p));
            used.insert(kept.id.clone());
            graph.nodes.push(kept);
        }
    }
    graph.edges.retain(|e| used.contains(&e.from) && used.contains(&e.to));
    Ok(graph)
}

/// The rewrite of `asked`, the board as it was when the model was asked, brought up to `now`: the board went
/// on changing while the model thought, often for a minute. What was added since stays, under its parent where
/// the rewrite kept it, otherwise under the nearest ancestor it kept, otherwise as it was; what was deleted
/// since goes; links added since stay while both ends do.
pub fn catch_up(asked: &Graph, now: &Graph, rewritten: &Graph) -> Graph {
    let mut graph = rewritten.clone();
    let was = |id: &str| asked.nodes.iter().any(|n| n.id == id);
    let is = |id: &str| now.nodes.iter().any(|n| n.id == id);
    // The rewrite and the lines heard meanwhile both numbered their new nodes on from the board as asked, so
    // the same id can name two things: the rewrite's new ones are numbered again, after both.
    graph.next_node = graph.next_node.max(now.next_node);
    let mut renamed: Vec<(String, String)> = vec![];
    for n in &mut graph.nodes {
        if !was(&n.id) && is(&n.id) {
            let id = format!("n{}", graph.next_node);
            graph.next_node += 1;
            renamed.push((n.id.clone(), id.clone()));
            n.id = id;
        }
    }
    let fresh = |id: &str| renamed.iter().find(|(from, _)| from == id).map(|(_, to)| to.clone());
    for n in &mut graph.nodes {
        if let Some(to) = n.parent.as_deref().and_then(fresh) {
            n.parent = Some(to);
        }
    }
    for e in &mut graph.edges {
        if let Some(to) = fresh(&e.from) {
            e.from = to;
        }
        if let Some(to) = fresh(&e.to) {
            e.to = to;
        }
    }
    graph.nodes.retain(|n| !was(&n.id) || is(&n.id));
    for n in now.nodes.iter().filter(|n| !was(&n.id)) {
        let mut added = n.clone();
        let mut at = n.parent.clone();
        while let Some(p) = &at {
            if graph.nodes.iter().any(|m| &m.id == p) {
                break;
            }
            at = now.nodes.iter().chain(asked.nodes.iter()).find(|m| &m.id == p).and_then(|m| m.parent.clone());
        }
        added.parent = at;
        graph.nodes.push(added);
    }
    let present: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut edges = graph.edges.clone();
    for e in &now.edges {
        if !asked.edges.iter().any(|old| old.from == e.from && old.to == e.to) && !edges.iter().any(|old| old.from == e.from && old.to == e.to) {
            edges.push(e.clone());
        }
    }
    edges.retain(|e| present.contains(e.from.as_str()) && present.contains(e.to.as_str()));
    graph.edges = edges;
    graph
}

/// When a consolidation is asked for, which decides whether it is worth it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// During the meeting: only once the board has piled up ([`needed`]), so it is not rewritten under
    /// people's eyes every few lines.
    Piled,
    /// When listening stops, after a meeting rather than a short exchange: whenever lines came since the last
    /// rewrite, as what was drawn after it would otherwise stay as drawn line by line. On the heard phone
    /// meeting the three passes rewritten at the end scored 0.70-0.73 item F1, the three left with a rewrite 3
    /// to 23 lines earlier 0.48-0.67. A board that piled up is rewritten whatever its length.
    Final,
}

/// A board smaller than this is not rewritten at the end: there is nothing to organise.
const FINAL_LEAST: usize = 6;
/// Fewer lines than this is an exchange, not a meeting, and its board is left as drawn at the end: rewriting
/// five lines of a retrospective folded its three sections into one topic, and an options board's pros and
/// cons into notes (dev set item F1 0.84 to 0.70).
const FINAL_LINES: usize = 16;

/// More points straight under one topic than this, and the board is piling up remarks: the layout skill's own
/// limit (skills/board-layout/RULES.md, rule C).
const CROWDED: usize = 7;
/// Points nested this deep are remarks hung one under the other rather than organised.
const TOO_DEEP: usize = 4;

/// Whether the board has piled up enough to need consolidating: a topic with more than `CROWDED` points
/// straight under it, or points nested `TOO_DEEP` levels down. A short board, or one already in order such as
/// a division of work by person, is left as the line-by-line model drew it.
pub fn needed(graph: &Graph) -> bool {
    let children = |id: &str| graph.nodes.iter().filter(|n| n.parent.as_deref() == Some(id)).count();
    let depth = |n: &NodeData| {
        let (mut d, mut at) = (1, n.parent.clone());
        while let Some(p) = at {
            d += 1;
            at = graph.nodes.iter().find(|m| m.id == p).and_then(|m| m.parent.clone());
            if d > TOO_DEEP {
                break;
            }
        }
        d
    };
    graph.nodes.iter().any(|n| (n.kind == Kind::Topic && children(&n.id) > CROWDED) || depth(n) >= TOO_DEEP)
}

/// Whether a rewrite kept enough of the board to be trusted: merging remarks shrinks a board, but losing two
/// thirds of it is a reply gone wrong, such as one cut short.
pub fn kept_enough(old: &Graph, new: &Graph) -> bool {
    new.nodes.len() * 3 >= old.nodes.len()
}

/// The messages that ask for the outline, under `system`: the board as it stands, and the discussion so far.
pub fn messages(graph: &Graph, history: &[Utterance], english: bool, system: &str) -> Vec<Value> {
    let said: Vec<String> = history[history.len().saturating_sub(HISTORY_LINES)..]
        .iter()
        .map(|u| match &u.speaker {
            Some(who) => format!("{who}：{}", u.text),
            None => u.text.clone(),
        })
        .collect();
    let (board, discussion) = if english { ("Board now:", "The discussion so far:") } else { ("现在的白板：", "到现在为止的讨论：") };
    let user = format!("{board}\n{}\n\n{discussion}\n{}", graph.describe(), said.join("\n"));
    vec![json!({ "role": "system", "content": system }), json!({ "role": "user", "content": user })]
}

/// What kind of meeting this part is, asked in a short question of its own. A failed or unclear answer is
/// [`Style::General`], which forces no template on the board.
pub fn classify(provider: &Provider, graph: &Graph, history: &[Utterance], english: bool) -> Style {
    let asked = messages(graph, history, english, CLASSIFY[usize::from(english)]);
    provider
        .chat_reply_with(&asked, 2048, "low")
        .ok()
        .and_then(|reply| crate::message(&reply).ok().map(|m| crate::strip_thinking(m).to_string()))
        .map_or(Style::General, |answer| Style::from_answer(&answer))
}

/// A reasoning model can spend its room on thinking about the whole meeting and leave the answer empty or cut
/// off. An outline cut off is never used, as it would wipe the rest of the board.
fn cut_off(reply: &Value) -> bool {
    reply["choices"][0]["finish_reason"].as_str() == Some("length") || crate::message(reply).map(|m| crate::strip_thinking(m).trim().is_empty()).unwrap_or(true)
}

/// A remote model: asked once more with twice the room when its answer was cut off.
fn ask_remote(provider: &Provider, asked: &[Value], effort: &str) -> Result<Value> {
    let mut reply = provider.chat_reply_with(asked, ROOM, effort)?;
    if cut_off(&reply) {
        reply = provider.chat_reply_with(asked, ROOM * 2, effort)?;
    }
    if cut_off(&reply) {
        bail!("the outline was cut off, so the board is left as it was");
    }
    Ok(reply)
}

/// The most a local model may write, thinking included: at 30 tokens a second on a laptop, under five minutes.
const LOCAL_ROOM: u64 = 8192;
/// Less room than this left in the local context for the answer, and the discussion is shortened first.
const LOCAL_LEAST: u64 = 2048;
/// The discussion is never shortened below this many lines: with less the rewrite would only guess.
const LOCAL_FEWEST_LINES: usize = 8;

/// A local model, within its context window, which is far smaller than a service's: the discussion is
/// shortened, earliest lines first, until the board, the instructions and the room to answer all fit. It
/// thinks first when `think`, and if its thinking runs out of room or comes to nothing, it is asked again
/// without, as it is when thinking fails or runs past its time. A request the server still turns away for its
/// length is tried once more with half the discussion.
fn ask_local(provider: &Provider, graph: &Graph, history: &[Utterance], english: bool, instructions: &str, think: bool) -> Result<Value> {
    let mut heard = &history[history.len().saturating_sub(HISTORY_LINES)..];
    let mut room = LOCAL_ROOM;
    loop {
        let asked = messages(graph, heard, english, instructions);
        match provider.room_for(&asked) {
            Some((window, used)) => {
                let free = window.saturating_sub(used + 64);
                if free >= LOCAL_LEAST || heard.len() <= LOCAL_FEWEST_LINES {
                    if free < LOCAL_LEAST / 2 {
                        bail!("the board and the discussion do not fit the local model's context ({used} of {window} tokens), so the board is left as it was");
                    }
                    room = room.min(free);
                    break;
                }
                heard = &heard[heard.len() / 2..];
            }
            // The server would not say: try as it is, and let a refusal shorten it below.
            None => break,
        }
    }
    let ask = |heard: &[Utterance], think: bool| provider.chat_reply_thinking(&messages(graph, heard, english, instructions), room as u32, "high", think);
    let mut reply = match ask(heard, think) {
        Ok(reply) => reply,
        Err(e) if heard.len() > LOCAL_FEWEST_LINES && format!("{e:#}").contains("context") => {
            heard = &heard[heard.len() / 2..];
            ask(heard, false)?
        }
        // Thinking ran out of time, or failed some other way: once more, without it.
        Err(_) if think => ask(heard, false)?,
        Err(e) => return Err(e),
    };
    if think && cut_off(&reply) {
        reply = ask(heard, false)?;
    }
    if cut_off(&reply) {
        bail!("the outline was cut off, so the board is left as it was");
    }
    Ok(reply)
}

/// What a consolidation came to: the board rewritten, and what the model thought.
pub struct Consolidated {
    pub graph: Graph,
    pub outline: String,
    pub thinking: String,
    pub ms: u128,
    pub tokens_in: u64,
    pub tokens_out: u64,
    /// How many of `tokens_out` went to thinking.
    pub reasoning_tokens: u64,
    /// What kind of meeting it was taken for; none when the instructions were given.
    pub style: Option<Style>,
}

/// Ask the model to rewrite the board, after asking what kind of meeting it is. `None` when the board is not
/// one to consolidate. `system` replaces the instructions, and the question, to try other wording without
/// building again.
pub fn consolidate(provider: &Provider, graph: &Graph, history: &[Utterance], english: bool, system: Option<&str>, effort: &str, when: When) -> Result<Option<Consolidated>> {
    let wanted = match when {
        When::Piled => needed(graph),
        When::Final => needed(graph) || (history.len() >= FINAL_LINES && graph.nodes.len() >= FINAL_LEAST),
    };
    if !applies_to(graph) || !wanted {
        return Ok(None);
    }
    let started = std::time::Instant::now();
    let style = system.is_none().then(|| classify(provider, graph, history, english));
    let instructions = system.map_or_else(|| style.unwrap_or(Style::General).system(english), str::to_string);
    let reply = if matches!(provider, Provider::Local { .. }) {
        ask_local(provider, graph, history, english, &instructions, effort != "none")?
    } else {
        ask_remote(provider, &messages(graph, history, english, &instructions), effort)?
    };
    let outline = crate::strip_thinking(crate::message(&reply)?).to_string();
    let lines = parse(&outline);
    let rebuilt = rebuild(graph, &lines).map_err(|e| {
        let head: String = outline.lines().take(6).collect::<Vec<_>>().join(" | ");
        anyhow::anyhow!("{e}; the reply began: {}", head.chars().take(300).collect::<String>())
    })?;
    if !kept_enough(graph, &rebuilt) {
        bail!("the outline kept {} of the board's {} nodes, too few to trust, so the board is left as it was", rebuilt.nodes.len(), graph.nodes.len());
    }
    Ok(Some(Consolidated {
        graph: rebuilt,
        outline,
        thinking: crate::reasoning(&reply).unwrap_or_default().to_string(),
        ms: started.elapsed().as_millis(),
        tokens_in: reply["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        tokens_out: reply["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        reasoning_tokens: reply["usage"]["completion_tokens_details"]["reasoning_tokens"].as_u64().unwrap_or(0),
        style,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alcyo_board::graph::parse_ops;

    #[test]
    fn an_outline_is_read_with_marks_and_speakers() {
        let lines = parse("```\nT 目标人群\n  L 面要广 @B\n    K 老年人也时尚\n\t- L 结论：主打年轻化 [星标]\n  L 地推 [划掉] @A\n  Q：年龄上限？\n这是一句解释\n```");
        assert_eq!(lines.len(), 6);
        assert_eq!((lines[1].depth, lines[1].kind, lines[1].by.as_deref()), (1, Kind::Item, Some("B")));
        assert_eq!(lines[3].marks, ["星标"]);
        assert_eq!((lines[4].label.as_str(), lines[4].marks.clone(), lines[4].by.as_deref()), ("地推", vec!["划掉".to_string()], Some("A")));
        assert_eq!(lines[5].kind, Kind::Question);
    }

    #[test]
    fn several_speakers_are_read_however_they_are_written() {
        let lines = parse("T 目标客户群定位 [星标] @A & B\n  L 特定年龄段方案 @C@D\n  L 地推 [划掉] @A、C\n  K 发邮件给 a@b.com 确认");
        assert_eq!((lines[0].label.as_str(), lines[0].by.as_deref()), ("目标客户群定位", Some("A、B")));
        assert_eq!(lines[0].marks, ["星标"]);
        assert_eq!((lines[1].label.as_str(), lines[1].by.as_deref()), ("特定年龄段方案", Some("C、D")));
        assert_eq!((lines[2].label.as_str(), lines[2].by.as_deref(), lines[2].marks.clone()), ("地推", Some("A、C"), vec!["划掉".to_string()]));
        assert_eq!(lines[3].label, "发邮件给 a@b.com 确认");
    }

    #[test]
    fn rebuilding_keeps_old_nodes_and_moves_them() {
        let mut old = Graph::default();
        old.apply(&parse_ops("T 客户群\nL 客户群 人群面要广\nL 人群面要广 老年人也时尚\nL 客户群 操作不了"));
        let topic_id = old.nodes[0].id.clone();
        let fashion_id = old.nodes.iter().find(|n| n.label == "老年人也时尚").unwrap().id.clone();
        let outline = parse("T 客户群\n  L 面要广，不必界定太死\n    K 老年人也时尚\n  Q 年龄上限？");
        let new = rebuild(&old, &outline).unwrap();
        assert_eq!(new.nodes.len(), 4);
        assert_eq!(new.nodes[0].id, topic_id);
        let fashion = new.nodes.iter().find(|n| n.id == fashion_id).unwrap();
        assert_eq!(fashion.kind, Kind::Note);
        assert_eq!(fashion.parent.as_deref(), Some(new.nodes[1].id.as_str()));
        // A remark the outline dropped is gone; new lines get new ids.
        assert!(!new.nodes.iter().any(|n| n.label == "操作不了"));
        assert!(new.nodes.iter().all(|n| n.id.starts_with('n')));
    }

    #[test]
    fn only_a_board_that_piled_up_is_consolidated() {
        let mut short = Graph::default();
        short.apply(&parse_ops("T 小王\nL 小王 前端\nL 小王 登录页\nT 老李\nL 老李 后端"));
        assert!(!needed(&short));
        let mut flat = Graph::default();
        flat.apply(&parse_ops("T 议题"));
        for i in 0..8 {
            flat.apply(&parse_ops(&format!("L 议题 第{i}点")));
        }
        assert!(needed(&flat));
        let mut deep = Graph::default();
        deep.apply(&parse_ops("T 议题\nL 议题 一\nL 一 二\nL 二 三"));
        assert!(needed(&deep));
    }

    #[test]
    fn a_rewrite_that_lost_most_of_the_board_is_not_trusted() {
        let mut old = Graph::default();
        old.apply(&parse_ops("T 议题"));
        for i in 0..11 {
            old.apply(&parse_ops(&format!("L 议题 第{i}点")));
        }
        let cut = rebuild(&old, &parse("T 议题\n  L 第0点\n  L 第1点")).unwrap();
        assert!(!kept_enough(&old, &cut));
        let merged = rebuild(&old, &parse("T 议题\n  L 第0点\n    K 第1点\n  L 第2点\n  L 结论 [星标]")).unwrap();
        assert!(kept_enough(&old, &merged));
    }

    #[test]
    fn the_kind_of_meeting_picks_the_instructions() {
        assert_eq!(Style::from_answer("Discussion"), Style::Discussion);
        assert_eq!(Style::from_answer("assignment\n"), Style::Assignment);
        assert_eq!(Style::from_answer("我觉得是汇报"), Style::General);
        let zh = Style::Assignment.system(false);
        assert!(zh.starts_with("你在整理会议白板") && zh.contains("按人或者按步骤"));
        assert!(Style::Discussion.system(true).contains("Decided:"));
    }

    #[test]
    fn what_was_said_while_the_model_thought_is_kept() {
        let mut asked = Graph::default();
        asked.apply(&parse_ops("T 「目标人群」\nL 「目标人群」 「面要广」\nL 「面要广」 「老人也时尚」\nL 「目标人群」 「旧的一条」"));
        let mut now = asked.clone();
        // While the model thought: a remark under a point it moves, a new topic, and a point deleted.
        let wide = now.nodes.iter().find(|n| n.label == "面要广").unwrap().id.clone();
        let old = now.nodes.iter().find(|n| n.label == "旧的一条").unwrap().id.clone();
        now.apply(&parse_ops(&format!("L {wide} 「外观时尚」\nT 「促销方式」\nL 「促销方式」 「直播带货」\nD {old}")));
        let outline = parse("T 目标人群\n  L 面要广 [圈出]\n    - 老人也时尚\n  L 旧的一条");
        let rewritten = rebuild(&asked, &outline).unwrap();
        let caught = catch_up(&asked, &now, &rewritten);
        let find = |label: &str| caught.nodes.iter().find(|n| n.label == label).unwrap_or_else(|| panic!("{label} is gone"));
        assert_eq!(find("外观时尚").parent.as_deref(), Some(wide.as_str()));
        assert_eq!(find("直播带货").parent.as_deref(), Some(find("促销方式").id.as_str()));
        assert!(find("面要广").marks.iter().any(|m| m == "圈出"), "the rewrite's marks stay");
        assert!(!caught.nodes.iter().any(|n| n.label == "旧的一条"), "deleted since, so gone");
        let ids: HashSet<&str> = caught.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), caught.nodes.len(), "no id twice");
    }

    #[test]
    fn new_nodes_from_both_sides_never_share_an_id() {
        let mut asked = Graph::default();
        asked.apply(&parse_ops("T 「议题」\nL 「议题」 「一」\nL 「一」 「一的理由」"));
        let mut now = asked.clone();
        // A line heard meanwhile takes the next id, which the rewrite also gives its new point.
        now.apply(&parse_ops("L 「议题」 「后来说的」"));
        let rewritten = rebuild(&asked, &parse("T 议题\n  L 新归纳的一条\n    K 一的理由\n  L 一")).unwrap();
        let taken = now.nodes.iter().find(|n| n.label == "后来说的").unwrap().id.clone();
        assert!(rewritten.nodes.iter().any(|n| n.id == taken && n.label == "新归纳的一条"), "the case this guards against");
        let caught = catch_up(&asked, &now, &rewritten);
        let ids: HashSet<&str> = caught.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids.len(), caught.nodes.len(), "no id twice: {:?}", caught.nodes.iter().map(|n| (&n.id, &n.label)).collect::<Vec<_>>());
        let new_point = caught.nodes.iter().find(|n| n.label == "新归纳的一条").unwrap();
        let reason = caught.nodes.iter().find(|n| n.label == "一的理由").unwrap();
        assert_eq!(reason.parent.as_deref(), Some(new_point.id.as_str()), "its children follow it to the new id");
        assert_eq!(caught.nodes.iter().find(|n| n.label == "后来说的").unwrap().id, taken);
    }

    #[test]
    fn a_chain_the_outline_wrote_out_is_not_shown_twice() {
        let mut old = Graph::default();
        old.apply(&parse_ops("T 促销方式\nL 促销方式 地推\nE 让利 -> 抢占份额\nE 抢占份额 -> 提价\nE 网关 -> 鉴权服务"));
        let rebuilt = rebuild(&old, &parse("T 促销方式\n  L 地推 [划掉]\n  L 先让利抢占份额，份额稳定后提价")).unwrap();
        let labels: Vec<&str> = rebuilt.nodes.iter().map(|n| n.label.as_str()).collect();
        assert!(!labels.contains(&"抢占份额") && !labels.contains(&"提价"), "{labels:?}");
        assert!(labels.contains(&"网关") && labels.contains(&"鉴权服务"), "a flow the outline never mentions stays: {labels:?}");
        assert_eq!(rebuilt.edges.len(), 1);
    }

    #[test]
    fn a_tidy_board_is_rewritten_only_at_the_end() {
        let mut tidy = Graph::default();
        tidy.apply(&parse_ops("T 议题\nL 议题 一\nL 议题 二\nT 另一题\nL 另一题 三\nL 另一题 四"));
        assert!(!needed(&tidy));
        let provider = Provider::Scripted(std::sync::Arc::new(|_: &str| Ok("T 议题\n  L 一\n  L 二\nT 另一题\n  L 三\n  L 四 [星标]".to_string())));
        let heard: Vec<Utterance> = (0..FINAL_LINES).map(|i| Utterance { speaker: None, text: format!("第{i}句，四最重要"), pointing: vec![] }).collect();
        assert!(consolidate(&provider, &tidy, &heard, false, None, "high", When::Piled).unwrap().is_none(), "left alone during the meeting");
        let done = consolidate(&provider, &tidy, &heard, false, None, "high", When::Final).unwrap().expect("rewritten at the end");
        assert!(done.graph.nodes.iter().any(|n| n.label == "四" && n.marks == ["星标"]));
        let mut small = Graph::default();
        small.apply(&parse_ops("T 议题\nL 议题 一"));
        assert!(consolidate(&provider, &small, &heard, false, None, "high", When::Final).unwrap().is_none(), "too small to organise");
        assert!(consolidate(&provider, &tidy, &heard[..5], false, None, "high", When::Final).unwrap().is_none(), "a short exchange is left as drawn");
    }

    #[test]
    fn a_flow_chart_is_left_alone() {
        let mut flow = Graph::default();
        flow.apply(&parse_ops("E 用户 小程序"));
        assert!(!applies_to(&flow));
        let mut outline = Graph::default();
        outline.apply(&parse_ops("T 议题\nL 议题 一点\nL 议题 另一点"));
        assert!(applies_to(&outline));
        // A link between two points of an outline does not make it a flow chart.
        outline.apply(&parse_ops("E 一点 另一点"));
        assert!(applies_to(&outline));
        // Nor does a link to a new name, which the board makes a flow node. Hanging off a point, it was folded
        // into the outline and goes; part of a flow chart, linked to another flow node, it stays.
        outline.apply(&parse_ops("E 一点 新名字"));
        assert!(applies_to(&outline));
        let folded = rebuild(&outline, &parse("T 议题\n  L 一点")).unwrap();
        assert!(!folded.nodes.iter().any(|n| n.label == "新名字"));
        outline.apply(&parse_ops("E 新名字 下一步"));
        let kept = rebuild(&outline, &parse("T 议题\n  L 一点")).unwrap();
        assert!(kept.nodes.iter().any(|n| n.label == "新名字") && kept.nodes.iter().any(|n| n.label == "下一步"));
    }
}
