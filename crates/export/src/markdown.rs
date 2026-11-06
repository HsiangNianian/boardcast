//! A meeting as Markdown, ported from src/board/markdown.ts: topics and items become headings and
//! numbered lists, the flow chart becomes Mermaid, and the transcript follows. Draft ink is left out.

use crate::Doc;
use boardcast_board::board_layout::{is_diagram, is_head};
use boardcast_board::graph::{Graph, Kind, Node};
use serde_json::Value;
use std::collections::HashSet;

struct Words {
    summary: &'static str,
    transcript: &'static str,
    board: &'static str,
    flow: &'static str,
    annotations: &'static str,
    open: &'static str,
    links: &'static str,
    note: &'static str,
    question: &'static str,
    colon: &'static str,
    omitted: &'static str,
}

const ZH: Words = Words {
    summary: "摘要",
    transcript: "会议记录",
    board: "第 {} 张板",
    flow: "流程",
    annotations: "原文标注",
    open: "待定",
    links: "关联",
    note: "备注：",
    question: "？",
    colon: "：",
    omitted: "\n\n> 此 Markdown 不包含自由图形、笔迹、图片和自定义视觉样式；请同时导出 SVG、PDF 或 Excalidraw 保留完整视觉内容。\n",
};

const EN: Words = Words {
    summary: "Summary",
    transcript: "Transcript",
    board: "Board {}",
    flow: "Flow",
    annotations: "Annotated source",
    open: "Open items",
    links: "Links",
    note: "Note: ",
    question: "? ",
    colon: ": ",
    omitted: "\n\n> This Markdown omits free drawings, strokes, images and custom visual styling. Export SVG, PDF or Excalidraw alongside it to preserve the complete visual board.\n",
};

fn mark_name(mark: &str, english: bool) -> &str {
    if !english {
        return mark;
    }
    match mark {
        "划掉" => "Strike",
        "圈出" => "Circle",
        "下划线" => "Underline",
        "星标" => "Star",
        "高亮" => "Highlight",
        other => other,
    }
}

fn one_line_breaks(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// Speech and labels are plain text: nothing in them may become Markdown, and a line break must not
/// escape the heading or list item it is in.
fn text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if "\\`*_{}[]()#!<>|~&:@".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    let lead = out.len() - out.trim_start_matches([' ', '\t']).len();
    if out[lead..].starts_with(['-', '+']) {
        out.insert(lead, '\\');
    }
    let digits = out[lead..].len() - out[lead..].trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits > 0 {
        let dot = lead + digits;
        let after = out[dot..].strip_prefix('.');
        if after.is_some_and(|a| a.is_empty() || a.starts_with([' ', '\t'])) {
            out.insert(dot, '\\');
        }
    }
    one_line_breaks(&out).replace('\n', "<br />")
}

/// A formula, line by line in code spans long enough not to be closed by its own backticks.
fn code(s: &str) -> String {
    let lines = one_line_breaks(s);
    let rows: Vec<String> = lines
        .split('\n')
        .map(|line| {
            if line.is_empty() {
                return String::new();
            }
            let longest = line.split(|c| c != '`').map(str::len).max().unwrap_or(0);
            let fence = "`".repeat(longest + 1);
            let edge = |c: Option<char>| matches!(c, Some('`' | ' '));
            let pad = if (edge(line.chars().next()) || edge(line.chars().last())) && !line.chars().all(|c| c == ' ') { " " } else { "" };
            format!("{fence}{pad}{line}{pad}{fence}")
        })
        .collect();
    rows.join("<br />")
}

fn extra_text(n: &Node) -> Option<&str> {
    n.extra.get("text").and_then(Value::as_str)
}

fn marked(n: &Node) -> String {
    let mut t = if n.kind == Kind::Formula {
        code(&n.label)
    } else if n.kind == Kind::Region {
        text(extra_text(n).unwrap_or(&n.label))
    } else {
        text(&n.label)
    };
    let has = |mark: &str| n.marks.iter().any(|m| m == mark);
    if has("划掉") {
        t = format!("~~{t}~~");
    }
    if has("圈出") {
        t = format!("**{t}**");
    }
    if has("下划线") {
        t = format!("<u>{t}</u>");
    }
    if has("星标") {
        t = format!("{t} ★");
    }
    if has("高亮") {
        t = format!("<mark>{t}</mark>");
    }
    t
}

fn is_attach(n: &Node) -> bool {
    matches!(n.kind, Kind::Question | Kind::Formula | Kind::Note)
}

fn attach_line(n: &Node, w: &Words) -> String {
    match n.kind {
        Kind::Question => format!("{}{}", w.question, marked(n)),
        Kind::Formula => marked(n),
        _ => format!("{}{}", w.note, marked(n)),
    }
}

/// Mermaid takes these as entities, and a line break must not close the code block around it.
fn mermaid(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let lines = one_line_breaks(s);
    for c in lines.chars() {
        match c {
            '\n' => out.push_str("<br />"),
            '&' | '#' | '"' | '`' | '<' | '>' => out.push_str(&format!("#{};", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn live(n: &Node) -> bool {
    n.ink != "draft"
}

fn walk(g: &Graph, parent: &Node, depth: usize, w: &Words, out: &mut Vec<String>) {
    let mut kids: Vec<&Node> = g.children(&parent.id).filter(|n| live(n)).collect();
    kids.sort_by_key(|n| is_attach(n));
    let mut index = 0;
    for c in kids {
        let pad = "   ".repeat(depth.saturating_sub(1));
        if is_attach(c) {
            out.push(format!("{pad}- {}", attach_line(c, w)));
        } else {
            let prefix = match depth {
                1 => format!("{}.", index + 1),
                2 => format!("- {}.", char::from(b'a' + (index % 26) as u8)),
                _ => "-".to_string(),
            };
            out.push(format!("{pad}{prefix} {}", marked(c)));
            index += 1;
            walk(g, c, depth + 1, w, out);
        }
    }
}

fn board(g: &Graph, h: &str, w: &Words, english: bool, out: &mut Vec<String>) {
    for n in g.nodes.iter().filter(|n| live(n) && is_head(g, n)) {
        out.push(format!("{h} {}", marked(n)));
        out.push(String::new());
        let before = out.len();
        walk(g, n, 1, w, out);
        if out.len() > before {
            out.push(String::new());
        }
    }

    let label_of = |id: &str| g.nodes.iter().find(|n| n.id == id).map(|n| n.label.as_str());
    let dia: Vec<&Node> = g.nodes.iter().filter(|n| live(n) && is_diagram(g, n)).collect();
    let ids: HashSet<&str> = dia.iter().map(|n| n.id.as_str()).collect();
    if !dia.is_empty() {
        out.extend([format!("{h} {}", w.flow), String::new(), "```mermaid".into(), "flowchart LR".into()]);
        let mut grouped = HashSet::new();
        for group in &g.groups {
            let members: Vec<&&Node> = dia.iter().filter(|n| n.group.as_deref() == Some(&group.id)).collect();
            if members.is_empty() {
                continue;
            }
            out.push(format!("  subgraph {}[\"{}\"]", group.id, mermaid(&group.label)));
            for n in members {
                out.push(format!("    {}[\"{}\"]", n.id, mermaid(&n.label)));
                grouped.insert(n.id.as_str());
            }
            out.push("  end".into());
        }
        for n in dia.iter().filter(|n| !grouped.contains(n.id.as_str())) {
            out.push(format!("  {}[\"{}\"]", n.id, mermaid(&n.label)));
        }
        for e in g.edges.iter().filter(|e| e.ink != "draft" && ids.contains(e.from.as_str()) && ids.contains(e.to.as_str())) {
            out.push(match e.label.as_deref().filter(|l| !l.is_empty()) {
                Some(label) => format!("  {} -->|\"{}\"| {}", e.from, mermaid(label), e.to),
                None => format!("  {} --> {}", e.from, e.to),
            });
        }
        out.extend(["```".into(), String::new()]);
        for n in dia.iter().filter(|n| !n.marks.is_empty()) {
            let marks: Vec<&str> = n.marks.iter().map(|m| mark_name(m, english)).collect();
            out.push(format!("- {}{}{}", text(&n.label), w.colon, marks.join(" · ")));
        }
        let notes: Vec<&Node> = g.nodes.iter().filter(|n| live(n) && is_attach(n) && n.parent.as_deref().is_some_and(|p| !p.is_empty() && ids.contains(p))).collect();
        for n in &notes {
            let parent = n.parent.as_deref().and_then(label_of).unwrap_or_default();
            out.push(format!("- {}{}{}", text(parent), w.colon, attach_line(n, w)));
        }
        if !notes.is_empty() {
            out.push(String::new());
        }
    }

    let spans = |n: &Node| n.extra.get("spans").and_then(Value::as_array).cloned().unwrap_or_default();
    let pins: Vec<&Node> = g.nodes.iter().filter(|n| live(n) && n.kind == Kind::Region && (!n.marks.is_empty() || !spans(n).is_empty() || g.children(&n.id).any(live))).collect();
    if !pins.is_empty() {
        out.extend([format!("{h} {}", w.annotations), String::new()]);
        for n in pins {
            out.push(format!("- {}", marked(n)));
            for s in spans(n) {
                out.push(format!("   - {}{}{}", mark_name(s["mark"].as_str().unwrap_or_default(), english), w.colon, text(s["text"].as_str().unwrap_or_default())));
            }
            for c in g.children(&n.id).filter(|n| live(n)) {
                out.push(format!("   - {}", attach_line(c, w)));
            }
        }
        out.push(String::new());
    }

    let loose: Vec<&Node> = g.nodes.iter().filter(|n| live(n) && is_attach(n) && n.parent.as_deref().filter(|p| !p.is_empty()).is_none_or(|p| label_of(p).is_none())).collect();
    if !loose.is_empty() {
        out.extend([format!("{h} {}", w.open), String::new()]);
        out.extend(loose.iter().map(|n| format!("- {}", attach_line(n, w))));
        out.push(String::new());
    }

    let is_dia = |id: &str| dia.iter().any(|n| n.id == id);
    let cross: Vec<_> = g.edges.iter().filter(|e| e.ink != "draft" && !(is_dia(&e.from) && is_dia(&e.to))).collect();
    if !cross.is_empty() {
        out.extend([format!("{h} {}", w.links), String::new()]);
        for e in cross {
            let name = |id: &str| text(label_of(id).unwrap_or(id));
            let label = e.label.as_deref().filter(|l| !l.is_empty()).map_or(String::new(), |l| format!("{}{}", w.colon, text(l)));
            out.push(format!("- {} → {}{label}", name(&e.from), name(&e.to)));
        }
        out.push(String::new());
    }
}

pub fn markdown(doc: &Doc, current: usize) -> String {
    let w = if doc.english { &EN } else { &ZH };
    let board_name = |n: u64| w.board.replace("{}", &n.to_string());
    let mut out = vec![format!("# {}", text(&doc.title)), String::new(), format!("> {} · boardcast", doc.exported), String::new()];
    if !doc.summary.trim().is_empty() {
        out.extend([format!("## {}", w.summary), String::new(), doc.summary.trim().to_string(), String::new()]);
    }
    let many = doc.boards.len() > 1;
    if many {
        for (i, g) in doc.boards.iter().enumerate() {
            out.extend([format!("## {}", board_name(i as u64 + 1)), String::new()]);
            board(g, "###", w, doc.english, &mut out);
        }
    } else if let Some(g) = doc.boards.get(current).or(doc.boards.first()) {
        board(g, "##", w, doc.english, &mut out);
    }
    if !doc.lines.is_empty() {
        out.extend([format!("## {}", w.transcript), String::new()]);
        let mut page = None;
        for l in &doc.lines {
            if many && page != Some(l.page) {
                if page.is_some() {
                    out.push(String::new());
                }
                page = Some(l.page);
                out.extend([format!("### {}", board_name(l.page + 1)), String::new()]);
            }
            out.push(match &l.speaker {
                Some(speaker) => format!("- **{}**{}{}", text(speaker), w.colon, text(&l.text)),
                None => format!("- {}", text(&l.text)),
            });
        }
        out.push(String::new());
    }
    let mut result = out.join("\n");
    if doc.omits_visuals {
        result.push_str(w.omitted);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Said;

    #[test]
    fn every_meeting_matches_the_typescript_engine() {
        let goldens: Value = serde_json::from_str(include_str!("../../board/fixtures/goldens.json")).unwrap();
        let cases = goldens["markdown"].as_array().unwrap();
        assert!(cases.len() >= 30);
        for case in cases {
            let doc = Doc {
                title: case["title"].as_str().unwrap().into(),
                exported: "2026-01-02 03:04".into(),
                began: String::new(),
                summary: case["summary"].as_str().unwrap_or_default().into(),
                boards: serde_json::from_value(case["boards"].clone()).unwrap(),
                lines: Said::from_stored(case["lines"].as_array().unwrap()),
                english: case["lang"] == "en",
                omits_visuals: false,
            };
            assert_eq!(markdown(&doc, 0), case["output"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn a_meeting_with_imported_pages_says_what_is_missing() {
        let doc = Doc { title: "t".into(), exported: String::new(), began: String::new(), summary: String::new(), boards: vec![Graph::default()], lines: vec![], english: true, omits_visuals: true };
        assert!(markdown(&doc, 0).contains("This Markdown omits"));
    }
}
