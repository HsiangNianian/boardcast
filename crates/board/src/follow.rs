//! Following prepared material: which board a line is about, so the meeting can turn to it. A line that
//! asks for a page ("go to page 5", "翻到第5页", "5ページ") names one outright; otherwise the line is
//! matched against the words on each board, and a board that matches clearly better than the one in view
//! is suggested. Only a suggestion: turning away from a speaker by mistake is worse than not turning.

use crate::graph::{Graph, Kind};
use std::collections::{HashMap, HashSet};

/// What a board says, for matching lines against it.
pub struct Page {
    /// Its place among the meeting's boards.
    pub board: usize,
    /// The number it had in the document, or its place counting from one.
    pub number: u32,
    /// Its first block of text, or its first label: usually the heading.
    pub title: String,
    terms: HashSet<String>,
}

/// A match must reach this, summed over the rarer words it shares, before a board is suggested.
const MIN_SCORE: f32 = 2.2;
/// And beat the board in view by this much, so the meeting stays where it is when that fits too.
const LEAD: f32 = 1.5;

const STOP: &[&str] = &[
    "the", "and", "for", "that", "this", "with", "are", "was", "you", "your", "have", "has", "not",
    "but", "from", "they", "what", "which", "will", "can", "about", "into", "then", "than",
    "there", "here", "our", "its", "it's", "let's", "also", "just", "some", "page", "slide",
    "next", "previous",
];

fn wide(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

/// The words a match counts: Latin words of three letters or more, and every pair of neighbouring
/// Chinese or Japanese characters, as those scripts do not mark where words end.
fn terms(text: &str) -> HashSet<String> {
    let lower = text.to_lowercase();
    let mut out = HashSet::new();
    let mut word = String::new();
    let mut run: Vec<char> = vec![];
    let flush_run = |run: &mut Vec<char>, out: &mut HashSet<String>| {
        for pair in run.windows(2) {
            out.insert(pair.iter().collect());
        }
        run.clear();
    };
    for c in lower.chars().chain([' ']) {
        if c.is_ascii_alphanumeric() || (c == '\'' && !word.is_empty()) {
            word.push(c);
        } else {
            if word.chars().count() >= 3
                && !STOP.contains(&word.as_str())
                && !word.chars().all(|c| c.is_ascii_digit())
            {
                out.insert(std::mem::take(&mut word));
            }
            word.clear();
        }
        if wide(c) {
            run.push(c);
        } else {
            flush_run(&mut run, &mut out);
        }
    }
    out
}

/// Every board of a meeting, as it can be matched.
pub fn pages(boards: &[Graph]) -> Vec<Page> {
    boards
        .iter()
        .enumerate()
        .map(|(i, g)| {
            let text = |n: &crate::graph::Node| {
                n.extra
                    .get("text")
                    .and_then(|t| t.as_str())
                    .map_or_else(|| n.label.clone(), str::to_string)
            };
            let words: Vec<String> = g.nodes.iter().map(text).collect();
            let number = g
                .extra
                .get("underlay")
                .and_then(|u| u.get("no"))
                .and_then(serde_json::Value::as_u64)
                .map_or(i as u32 + 1, |n| n as u32);
            let title = g
                .nodes
                .iter()
                .find(|n| n.kind == Kind::Region || n.kind == Kind::Topic)
                .or(g.nodes.first())
                .map(|n| n.label.clone())
                .unwrap_or_default();
            Page {
                board: i,
                number,
                title,
                terms: terms(&words.join("\n")),
            }
        })
        .collect()
}

/// The board a line asks to turn to, if it asks: by number, or the next or previous one. Numbers are
/// the document's, as the speaker sees them on the page.
pub fn asked(text: &str, pages: &[Page], current: usize) -> Option<usize> {
    let t = text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if has(&[
        "下一页",
        "下页",
        "翻页",
        "次のページ",
        "next page",
        "next slide",
    ]) {
        return (current + 1 < pages.len()).then_some(current + 1);
    }
    if has(&[
        "上一页",
        "前一页",
        "回到上一页",
        "前のページ",
        "previous page",
        "last page",
        "previous slide",
        "go back a page",
    ]) {
        return current.checked_sub(1);
    }
    let number = page_number(&t)?;
    pages
        .iter()
        .find(|p| p.number == number)
        .map(|p| p.board)
        .filter(|&b| b != current)
}

/// "第5页", "第五页", "5ページ", "page 5", "p. 5", "slide 5".
fn page_number(t: &str) -> Option<u32> {
    let digits = |s: &str| {
        s.chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse::<u32>()
            .ok()
    };
    let before = |s: &str| {
        let head: String = s
            .chars()
            .rev()
            .skip_while(|c| c.is_whitespace())
            .take_while(char::is_ascii_digit)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        head.parse::<u32>().ok()
    };
    for key in ["第", "page ", "slide ", "p. "] {
        if let Some(at) = t.find(key) {
            let rest = &t[at + key.len()..];
            if let Some(n) = digits(rest.trim_start()).or_else(|| chinese_number(rest)) {
                let after: String = rest
                    .chars()
                    .skip_while(|c| c.is_ascii_digit() || "一二三四五六七八九十两".contains(*c))
                    .take(1)
                    .collect();
                if key != "第" || matches!(after.as_str(), "页" | "頁" | "张" | "ペ") {
                    return Some(n);
                }
            }
        }
    }
    for key in ["ページ", "页", "頁"] {
        if let Some(at) = t.find(key) {
            if let Some(n) = before(&t[..at]) {
                return Some(n);
            }
        }
    }
    None
}

/// 一 to 九十九, as a page is said aloud in Chinese.
fn chinese_number(s: &str) -> Option<u32> {
    let value = |c: char| {
        "零一二三四五六七八九"
            .chars()
            .position(|d| d == c)
            .map(|v| v as u32)
            .or((c == '两').then_some(2))
    };
    let chars: Vec<char> = s
        .chars()
        .take_while(|c| value(*c).is_some() || *c == '十')
        .collect();
    match chars.as_slice() {
        [] => None,
        ['十'] => Some(10),
        ['十', d] => Some(10 + value(*d)?),
        [t, '十'] => Some(value(*t)? * 10),
        [t, '十', d] => Some(value(*t)? * 10 + value(*d)?),
        [d] => value(*d),
        _ => None,
    }
}

/// The board a line is about, when that is clearly another one than the board in view. Rarer words count
/// for more, as a word on every page says nothing about which page is meant.
pub fn suggested(text: &str, pages: &[Page], current: usize) -> Option<usize> {
    if pages.len() < 2 {
        return None;
    }
    let said = terms(text);
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for p in pages {
        for t in &p.terms {
            *seen.entry(t.as_str()).or_insert(0) += 1;
        }
    }
    let n = pages.len() as f32;
    let score = |p: &Page| -> f32 {
        said.iter()
            .filter(|t| p.terms.contains(*t))
            .map(|t| (1.0 + n / seen[t.as_str()] as f32).ln())
            .sum()
    };
    let here = pages.get(current).map_or(0.0, score);
    let (best, top) = pages
        .iter()
        .filter(|p| p.board != current)
        .map(|p| (p.board, score(p)))
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    (top >= MIN_SCORE && top >= here * LEAD + 0.5).then_some(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{NodeData, Ordered};

    fn page(no: u32, blocks: &[&str]) -> Graph {
        let mut g = Graph::default();
        g.extra.insert(
            "underlay".into(),
            serde_json::json!({ "w": 800, "h": 1000, "no": no }),
        );
        for (i, b) in blocks.iter().enumerate() {
            g.nodes.push(Ordered::new(NodeData {
                id: format!("n{}", i + 1),
                label: (*b).into(),
                aliases: vec![],
                kind: Kind::Region,
                parent: None,
                group: None,
                marks: vec![],
                icon: None,
                ink: "ink".into(),
                extra: Default::default(),
            }));
        }
        g
    }

    fn deck() -> Vec<Page> {
        pages(&[
            page(
                1,
                &[
                    "Introduction",
                    "Meetings lose what was said.",
                    "We draw it as people talk.",
                ],
            ),
            page(
                2,
                &[
                    "Related work",
                    "Speech recognition and diagram generation have been studied apart.",
                ],
            ),
            page(
                3,
                &[
                    "Methods",
                    "A local model turns each sentence into board operations.",
                    "Layout keeps every region in place.",
                ],
            ),
            page(4, &["评测结果", "在二十八场会议上测试了板书质量和延迟。"]),
            page(5, &["結論", "会議の板書は自動で書ける。"]),
        ])
    }

    #[test]
    fn asking_for_a_page_names_it() {
        let d = deck();
        assert_eq!(asked("Let's go to page 3", &d, 0), Some(2));
        assert_eq!(asked("翻到第4页", &d, 0), Some(3));
        assert_eq!(asked("看一下第四页", &d, 0), Some(3));
        assert_eq!(asked("5ページを見てください", &d, 0), Some(4));
        assert_eq!(asked("下一页", &d, 1), Some(2));
        assert_eq!(
            asked("next slide please", &d, 4),
            None,
            "there is no next one"
        );
        assert_eq!(asked("回到上一页", &d, 3), Some(2));
        assert_eq!(
            asked("第3个方案更好", &d, 0),
            None,
            "a third option is not a page"
        );
        assert_eq!(asked("page 9", &d, 0), None, "no such page");
        assert_eq!(asked("we cut latency by 40 percent", &d, 0), None);
    }

    #[test]
    fn a_line_about_another_page_suggests_it_and_one_about_this_page_does_not() {
        let d = deck();
        assert_eq!(
            suggested(
                "So the local model turns each sentence into board operations",
                &d,
                0
            ),
            Some(2)
        );
        assert_eq!(suggested("板书质量和延迟的评测结果怎么样", &d, 0), Some(3));
        assert_eq!(suggested("会議の板書を自動で書く結論です", &d, 0), Some(4));
        // Already on the page it is about.
        assert_eq!(
            suggested(
                "So the local model turns each sentence into board operations",
                &d,
                2
            ),
            None
        );
        // Too little in common to turn away.
        assert_eq!(suggested("I think that model is fine", &d, 0), None);
        assert_eq!(suggested("好的我们继续", &d, 0), None);
    }

    /// The deck and lines in eval/follow-cases.json, said with its first page in view. A wrong turn
    /// interrupts a speaker, so none is allowed; a missed one leaves the board where it was, so a few are.
    #[test]
    fn follow_cases() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("../../../eval/follow-cases.json")).unwrap();
        let boards: Vec<Graph> = cases["pages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let blocks: Vec<&str> = p["blocks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| b.as_str().unwrap())
                    .collect();
                page(p["no"].as_u64().unwrap() as u32, &blocks)
            })
            .collect();
        let deck = pages(&boards);
        let (mut wrong, mut missed, mut total) = (vec![], vec![], 0);
        for line in cases["lines"].as_array().unwrap() {
            let text = line["text"].as_str().unwrap();
            total += 1;
            let asked_now = asked(text, &deck, 0);
            if let Some(want) = line.get("asked") {
                if asked_now != want.as_u64().map(|b| b as usize) {
                    wrong.push(format!("asked {text}: {asked_now:?}, want {want}"));
                }
                if want.is_u64() {
                    continue;
                }
            }
            let got = suggested(text, &deck, 0);
            match (got, line["suggested"].as_u64().map(|b| b as usize)) {
                (Some(g), Some(w)) if g == w => {}
                (None, Some(w)) => missed.push(format!("{text}: none, want {w}")),
                (Some(g), want) => wrong.push(format!("{text}: {g}, want {want:?}")),
                (None, None) => {}
            }
        }
        println!(
            "{total} lines: {} wrong, {} missed {missed:?}",
            wrong.len(),
            missed.len()
        );
        assert!(wrong.is_empty(), "turned the wrong way: {wrong:#?}");
        assert!(missed.len() <= 2, "missed too many: {missed:#?}");
    }

    #[test]
    fn a_board_without_a_document_still_has_a_title_and_number() {
        let mut g = Graph::default();
        g.apply(&crate::graph::parse_ops(
            "T 「Release checklist」\nL 「Release checklist」 「Load testing」",
        ));
        let p = pages(&[g.clone(), g]);
        assert_eq!((p[1].number, p[1].title.as_str()), (2, "Release checklist"));
    }
}
