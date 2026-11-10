//! Reading the layout of a PDF page, ported from src/board/paper.ts: the pieces of its text layer become
//! blocks, named as a paper names them. Ordinary documents go through here too and simply never meet
//! the rules for captions and equation numbers.
//!
//! 1. Analyse by text direction, and drop margin marks and page numbers.
//! 2. Find a gutter, so two columns are read apart, and things across both are kept whole.
//! 3. Join pieces into lines, and classify each: heading, caption, equation, reference or text.
//! 4. Join lines into paragraphs, breaking at an indent, a short line or a heading.
//! 5. Grow each caption to take in the figure or table it belongs to, and name everything.
//!
//! Coordinates are board coordinates: the page's top left is the origin and y goes down.

use crate::regions::{js_round, utf16_len, Area};
use boardcast_board::spans::{rotate_point, RegionRow, Seg};
use fancy_regex::Regex as Fancy;
use regex::Regex;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct TextPiece {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Radians from the text's own direction to the page.
    #[serde(default)]
    pub angle: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Heading,
    Figure,
    Table,
    Equation,
    Reference,
    Text,
}

impl BlockKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Heading => "heading",
            Self::Figure => "figure",
            Self::Table => "table",
            Self::Equation => "equation",
            Self::Reference => "reference",
            Self::Text => "text",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaperArea {
    pub area: Area,
    pub kind: BlockKind,
}

#[derive(Debug, Default)]
pub struct PageAnalysis {
    pub areas: Vec<PaperArea>,
    /// This page's section headings in order, for the paper's outline.
    pub headings: Vec<String>,
    pub columns: u8,
}

/// What carries from page to page.
#[derive(Debug, Default)]
pub struct PaperState {
    pub in_refs: bool,
    /// Above zero, this document has more figures above their captions than below.
    figure_up: i32,
    table_up: i32,
    /// Captions seen so far: a number counts once, and later ones are text referring to it.
    captions: HashSet<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Side {
    Left,
    Right,
    Span,
}

struct Line {
    text: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    side: Side,
    kind: BlockKind,
    num: Option<String>,
    row: Option<RegionRow>,
    parts: Vec<usize>,
}

struct Block {
    text: String,
    name: Option<String>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    kind: BlockKind,
    side: Side,
    lh: f64,
    lines: usize,
    num: Option<String>,
    last_right: Option<f64>,
    rows: Vec<RegionRow>,
}

macro_rules! fancy {
    ($name:ident, $pattern:expr) => {
        static $name: LazyLock<Fancy> = LazyLock::new(|| Fancy::new($pattern).expect("valid"));
    };
}
macro_rules! plain {
    ($name:ident, $pattern:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).expect("valid"));
    };
}

// A caption: its number is followed by a stop, then a capital or a Chinese character. A figure mentioned
// in the text ("Fig. 3)." or "Table 3 shows") does not count.
fancy!(CAPTION, r"^(Figure|FIGURE|Fig\.?|图|Table|TABLE|Tab\.?|表)\s*(S?[0-9]+|[IVX]+)\s*[.:：．]\s*(?=[A-Z(（\p{Han}])");
// A numbered heading: a capitalised word or a Chinese character must follow ("3 x3 conv" in a figure is not one).
fancy!(NUMBERED, r"^((?:[0-9]+\.)*[0-9]+\.?|[IVX]+\.|[A-Z]\.|[一二三四五六七八九十]+、|第[一二三四五六七八九十0-9]+[章节])\s*(?=[A-Z][A-Za-z]|\p{Han})");
plain!(NAMED, r"(?i)^(Abstract|Introduction|Related Work|Conclusions?|Acknowledge?ments?|References|Bibliography|Appendix|Supporting Information|Supplementary (?:Information|Materials?)|摘要|引言|结论|致谢|参考文献|附录|补充材料)");
plain!(REFS, r"(?i)^(References|Bibliography|参考文献)\s*$");
plain!(AFTER_REFS, r"(?i)^(Appendix|附录|Supporting Information|Supplementary (Information|Materials?)|补充材料)");
plain!(REF_ENTRY, r"^(\[[0-9]+\]|\([0-9]+\)|[0-9]+\.\s)");
plain!(PAGE_NO, r"^\s*([0-9]{1,4}|[A-Z]-?[0-9]{1,4})\s*$");
plain!(EQNUM, r"\(([0-9]+[a-z]?)\)\s*$");
plain!(ONLY_EQNUM, r"^\([0-9]+[a-z]?\)$");
plain!(CAPTION_TABLE, r"^(Table|TABLE|Tab|表)");
plain!(CONTINUED, r"(?i)\(continued\)|（续）");
plain!(LEADING_NUMBER, r"^[0-9.]+\s*");
plain!(NUMBER_THEN_DOT, r"^[0-9]+(\.[0-9]+)*\.\s");
plain!(LEAD, r"^([0-9]+)\s");
plain!(SENTENCE, r"(?i)(?-u:\b)(is|are|was|were|we|our|this|that)(?-u:\b)");
plain!(INITIAL, r"^[A-Z]\.\s");
plain!(ARXIV_MARK, r"(?i)^arXiv\s*:");
plain!(LETTER, r"\p{L}");
plain!(HAN, r"\p{Han}");
plain!(CJK, r"[\p{Han}\p{Hiragana}\p{Katakana}]");

fn fancy_match(re: &Fancy, s: &str) -> bool {
    re.is_match(s).unwrap_or(false)
}

fn latin(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_alphanumeric() || ",.;:)".contains(c))
}

fn join(a: &str, b: &str) -> String {
    if latin(a.chars().last()) && b.chars().next().is_some_and(|c| c.is_ascii_alphanumeric() || c == '(') {
        format!("{a} {b}")
    } else {
        format!("{a}{b}")
    }
}

/// Joining the next line: a hyphen that broke a word at the end of this one goes away.
fn join_line(a: &str, b: &str) -> String {
    let mut end = a.chars().rev();
    let broken = end.next() == Some('-') && end.next().is_some_and(|c| c.is_ascii_lowercase()) && b.chars().next().is_some_and(|c| c.is_ascii_lowercase());
    if broken {
        format!("{}{b}", &a[..a.len() - 1])
    } else {
        join(a, b)
    }
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[sorted.len() / 2]
}

/// A gutter: a band across 35-65% of the page that almost nothing crosses, with plenty of text each
/// side. A few pieces may cross it, as a paper's title and authors are centred over both columns.
fn gutter(ps: &[&TextPiece], width: f64) -> Option<f64> {
    let narrow: Vec<&&TextPiece> = ps.iter().filter(|p| p.w < width * 0.55).collect();
    if narrow.len() < 20 {
        return None;
    }
    let allowed = 3_f64.max(js_round(narrow.len() as f64 * 0.03)) as usize;
    let (from, to) = (js_round(width * 0.35) as i64, js_round(width * 0.65) as i64);
    let mut best: Option<(i64, i64)> = None;
    let mut start = -1;
    for x in from..=to {
        let at = x as f64;
        let hit = narrow.iter().filter(|p| p.x < at && p.x + p.w > at).count() > allowed;
        if !hit && start < 0 {
            start = x;
        }
        if (hit || x == to) && start >= 0 {
            if best.is_none_or(|(a, b)| x - start > b - a) {
                best = Some((start, x));
            }
            start = -1;
        }
    }
    let (a, b) = best.filter(|(a, b)| b - a >= 6)?;
    let g = (a + b) as f64 / 2.0;
    let left = narrow.iter().filter(|p| p.x + p.w <= g).count() as f64;
    let right = narrow.iter().filter(|p| p.x >= g).count() as f64;
    let enough = narrow.len() as f64 * 0.2;
    (left > enough && right > enough).then_some(g)
}

pub fn analyze_page(pieces: &[TextPiece], width: f64, height: f64, state: &mut PaperState) -> PageAnalysis {
    if !pieces.iter().any(|p| p.angle.is_some_and(|a| a != 0.0)) {
        let all: Vec<&TextPiece> = pieces.iter().collect();
        return analyze_horizontal(&all, width, height, state);
    }
    let mut result = PageAnalysis { columns: 1, ..PageAnalysis::default() };
    let mut angles: Vec<f64> = vec![];
    for p in pieces {
        let angle = p.angle.unwrap_or(0.0);
        if !angles.contains(&angle) {
            angles.push(angle);
        }
    }
    for angle in angles {
        let turned: Vec<[f64; 2]> = [[0.0, 0.0], [width, 0.0], [0.0, height], [width, height]].iter().map(|c| rotate_point(*c, -angle)).collect();
        let x0 = turned.iter().map(|c| c[0]).fold(f64::INFINITY, f64::min);
        let y0 = turned.iter().map(|c| c[1]).fold(f64::INFINITY, f64::min);
        let w = turned.iter().map(|c| c[0]).fold(f64::NEG_INFINITY, f64::max) - x0;
        let h = turned.iter().map(|c| c[1]).fold(f64::NEG_INFINITY, f64::max) - y0;
        let group: Vec<TextPiece> = pieces.iter().filter(|p| p.angle.unwrap_or(0.0) == angle).map(|p| TextPiece { x: p.x - x0, y: p.y - y0, ..p.clone() }).collect();
        let refs: Vec<&TextPiece> = group.iter().collect();
        let analyzed = analyze_horizontal(&refs, w, h, state);
        result.columns = result.columns.max(analyzed.columns);
        result.headings.extend(analyzed.headings);
        for found in analyzed.areas {
            let a = &found.area;
            let corners: Vec<[f64; 2]> = [[a.x, a.y], [a.x + a.w, a.y], [a.x, a.y + a.h], [a.x + a.w, a.y + a.h]].iter().map(|c| rotate_point([c[0] + x0, c[1] + y0], angle)).collect();
            let x = corners.iter().map(|c| c[0]).fold(f64::INFINITY, f64::min);
            let y = corners.iter().map(|c| c[1]).fold(f64::INFINITY, f64::min);
            // A margin mark may be split into pieces, so it is only recognised once its line is whole.
            if angle != 0.0 && ARXIV_MARK.is_match(a.text.trim()) && (x < width * 0.06 || x > width * 0.94) {
                continue;
            }
            let rows = a.rows.as_ref().map(|rows| {
                rows.iter()
                    .map(|r| RegionRow {
                        y: r.y + y0,
                        h: r.h,
                        segs: r.segs.iter().map(|s| Seg { t: s.t.clone(), x: s.x + x0, w: s.w }).collect(),
                        angle: if angle != 0.0 { Some(angle) } else { r.angle },
                    })
                    .collect()
            });
            result.areas.push(PaperArea {
                kind: found.kind,
                area: Area {
                    text: a.text.clone(),
                    name: a.name.clone(),
                    x,
                    y,
                    w: corners.iter().map(|c| c[0]).fold(f64::NEG_INFINITY, f64::max) - x,
                    h: corners.iter().map(|c| c[1]).fold(f64::NEG_INFINITY, f64::max) - y,
                    rows,
                },
            });
        }
    }
    result
}

fn analyze_horizontal(pieces: &[&TextPiece], width: f64, height: f64, state: &mut PaperState) -> PageAnalysis {
    let in_margin = |y: f64, h: f64| y > height * 0.9 || y + h < height * 0.07;
    let valid: Vec<&TextPiece> = pieces.iter().copied().filter(|p| !p.text.trim().is_empty() && p.w > 0.0 && p.h > 0.0 && !(PAGE_NO.is_match(&p.text) && in_margin(p.y, p.h))).collect();
    let (vertical, ps) = vertical_text(&valid);
    let g = gutter(&ps, width);
    let side_of = |x: f64, w: f64| match g {
        None => Side::Left,
        Some(g) if x + w <= g + 2.0 => Side::Left,
        Some(g) if x >= g - 2.0 => Side::Right,
        Some(_) => Side::Span,
    };
    let heights: Vec<f64> = ps.iter().map(|p| p.h).collect();
    let body = median(&heights);

    // Normal-sized text first, with superscripts added last: their y differs a little, and joining them
    // early would split a line. Italics can also sit a little off, so either end of a line can join.
    let small = |p: &TextPiece| p.h < body * 0.8;
    let mut order: Vec<usize> = (0..ps.len()).collect();
    order.sort_by(|a, b| {
        let (p, q) = (ps[*a], ps[*b]);
        small(p).cmp(&small(q)).then((p.y - q.y).total_cmp(&0.0)).then((p.x - q.x).total_cmp(&0.0))
    });
    let mut lines: Vec<Line> = vec![];
    for i in order {
        let p = ps[i];
        let side = side_of(p.x, p.w);
        let hit = lines.iter_mut().find(|l| {
            let near = l.h.max(p.h) * 1.5;
            let overlap = (l.y + l.h).min(p.y + p.h) - l.y.max(p.y);
            l.side == side && ((l.y + l.h / 2.0 - (p.y + p.h / 2.0)).abs() < l.h.min(p.h) * 0.5 || (small(p) && overlap >= p.h * 0.4)) && p.x + p.w > l.x - near && p.x - (l.x + l.w) < near
        });
        if let Some(hit) = hit {
            hit.parts.push(i);
            let x0 = hit.x.min(p.x);
            let x1 = (hit.x + hit.w).max(p.x + p.w);
            let y0 = hit.y.min(p.y);
            hit.h = (hit.y + hit.h).max(p.y + p.h) - y0;
            hit.y = y0;
            hit.x = x0;
            hit.w = x1 - x0;
            hit.side = side_of(hit.x, hit.w);
        } else {
            lines.push(Line { text: String::new(), x: p.x, y: p.y, w: p.w, h: p.h, side, kind: BlockKind::Text, num: None, row: None, parts: vec![i] });
        }
    }
    // Left to right; a smaller piece, such as a citation mark, sits right against the one before.
    let tenth = |v: f64| js_round(v * 10.0) / 10.0;
    for l in &mut lines {
        l.parts.sort_by(|a, b| ps[*a].x.total_cmp(&ps[*b].x));
        let big = l.parts.iter().map(|i| ps[*i].h).fold(f64::NEG_INFINITY, f64::max);
        l.row = Some(RegionRow {
            y: js_round(l.y),
            h: js_round(l.h),
            segs: l.parts.iter().map(|i| Seg { t: ps[*i].text.trim().to_string(), x: tenth(ps[*i].x), w: tenth(ps[*i].w) }).collect(),
            angle: None,
        });
        let mut text = String::new();
        for i in &l.parts {
            let piece = ps[*i].text.trim();
            text = if text.is_empty() {
                piece.to_string()
            } else if ps[*i].h < big * 0.8 {
                text + piece
            } else {
                join(&text, piece)
            };
        }
        l.text = text;
        // A page number in two pieces ("S-" and "6") only shows once joined.
        if PAGE_NO.is_match(&l.text) && in_margin(l.y, l.h) {
            l.text.clear();
        }
    }
    // A page holding only a number is a full-page figure, wherever the number sits.
    let written: Vec<usize> = (0..lines.len()).filter(|i| !lines[*i].text.is_empty()).collect();
    if written.len() == 1 && PAGE_NO.is_match(&lines[written[0]].text) {
        lines[written[0]].text.clear();
    }

    classify(&mut lines, g, width, body, state);
    merge_equation_numbers(&mut lines, g);
    let live: Vec<&Line> = lines.iter().filter(|l| !l.text.is_empty()).collect();
    let mut blocks = paragraphs(&live, g, body);
    for a in vertical {
        let rows = a.rows.clone().unwrap_or_default();
        let heights: Vec<f64> = rows.iter().map(|r| r.h).collect();
        blocks.push(Block { text: a.text, name: None, x: a.x, y: a.y, w: a.w, h: a.h, kind: BlockKind::Text, side: side_of(a.x, a.w), lh: median(&heights), lines: rows.len(), num: None, last_right: None, rows });
    }
    let gone = grow_captions(&mut blocks, g, width, height, body, state);

    let mut headings = vec![];
    for b in &mut blocks {
        if b.kind == BlockKind::Equation {
            b.name = Some(format!("公式 ({})", b.num.as_deref().unwrap_or_default()));
        }
        if b.kind == BlockKind::Heading {
            let t = b.text.split_whitespace().collect::<Vec<_>>().join(" ");
            let name = match NUMBERED.captures(&t).ok().flatten() {
                Some(m) => {
                    let whole = m.get(0).expect("the whole match");
                    format!("§{} {}", m[1].strip_suffix('.').unwrap_or(&m[1]), t[whole.end()..].trim())
                }
                None => t,
            };
            headings.push(name.clone());
            b.name = Some(name);
        }
    }
    let areas = blocks
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !gone.contains(i))
        .map(|(_, b)| PaperArea {
            kind: b.kind,
            // A figure's text is scattered, so it is marked as a whole.
            area: Area { text: b.text, name: b.name, x: b.x, y: b.y, w: b.w, h: b.h, rows: (b.kind != BlockKind::Figure && b.kind != BlockKind::Table).then_some(b.rows) },
        })
        .collect();
    PageAnalysis { areas, headings, columns: if g.is_none() { 1 } else { 2 } }
}

fn classify(lines: &mut [Line], g: Option<f64>, width: f64, body: f64, state: &mut PaperState) {
    let col_width = |s: Side| if g.is_none() || s == Side::Span { width * 0.8 } else { width * 0.4 };
    for i in 0..lines.len() {
        if lines[i].text.is_empty() {
            continue;
        }
        let text = lines[i].text.clone();
        if let Some(cap) = CAPTION.captures(&text).ok().flatten() {
            let kind = if CAPTION_TABLE.is_match(&cap[1]) { BlockKind::Table } else { BlockKind::Figure };
            let key = format!("{} {}", kind.name(), &cap[2]);
            // A table across pages repeats its caption with "(continued)".
            if !state.captions.contains(&key) || CONTINUED.is_match(&text) {
                state.captions.insert(key);
                lines[i].kind = kind;
                lines[i].num = Some(cap[2].to_string());
                continue;
            }
        }
        if REFS.is_match(&text) {
            lines[i].kind = BlockKind::Heading;
            state.in_refs = true;
            continue;
        }
        let l = &lines[i];
        // A heading is not long and has no final stop, though a numbered one may fill a column.
        let wide_ok = l.w < col_width(l.side) * 0.9 || l.h >= body * 1.1 || (NUMBER_THEN_DOT.is_match(&text) && l.w <= col_width(l.side) * 1.02);
        let without_number = LEADING_NUMBER.replace(&text, "");
        let short = utf16_len(&text) <= 80 && wide_ok && !(without_number.ends_with('.') || without_number.ends_with('。')) && !text.contains("://");
        // A numbered heading starts at the column's left edge; one in mid-line is a footnote mark.
        let left_edge = lines.iter().filter(|o| o.side == l.side).map(|o| o.x).fold(f64::INFINITY, f64::min);
        let at_left = l.x - left_edge < body * 2.0;
        // A large number, or words like "is" and "we", make it a sentence.
        let high_number = LEAD.captures(&text).is_some_and(|c| c[1].parse::<f64>().unwrap_or(0.0) > 20.0);
        let without_heading_number = NUMBERED.replace(&text, "");
        let sentence = high_number || SENTENCE.is_match(&without_heading_number);
        let bigger = l.h >= body * 1.15;
        // A heading has its line to itself; anything beside it makes this a table row.
        let in_row = lines.iter().enumerate().any(|(j, o)| j != i && !o.text.is_empty() && o.side == l.side && (o.y + o.h).min(l.y + l.h) - o.y.max(l.y) > o.h.min(l.h) * 0.5);
        // "J. Chem." and "K. Anderson" are abbreviations, so a lettered heading must be larger than the text.
        let initial = INITIAL.is_match(&text) && !bigger;
        let numbered = fancy_match(&NUMBERED, &text);
        // Among references, only an appendix or a clearly larger numbered heading starts something new.
        let in_list = state.in_refs && !AFTER_REFS.is_match(&text) && !(numbered && bigger);
        if short && l.h >= body * 0.9 && ((numbered && at_left && !sentence && !initial && !in_row) || NAMED.is_match(&text)) && !in_list {
            state.in_refs = false;
            lines[i].kind = BlockKind::Heading;
            continue;
        }
        if state.in_refs {
            lines[i].kind = BlockKind::Reference;
            continue;
        }
        if let Some(eq) = EQNUM.captures(&text) {
            let symbols = text.chars().any(|c| "=≤≥<>∑∫".contains(c)) && utf16_len(&text) < 120;
            if ONLY_EQNUM.is_match(text.trim()) || symbols {
                lines[i].kind = BlockKind::Equation;
                lines[i].num = Some(eq[1].to_string());
            }
        }
    }
}

/// A "(2)" alone on a line joins the equation at its height.
fn merge_equation_numbers(lines: &mut [Line], g: Option<f64>) {
    for i in 0..lines.len() {
        if lines[i].kind != BlockKind::Equation || !ONLY_EQNUM.is_match(lines[i].text.trim()) {
            continue;
        }
        let l = &lines[i];
        let host = lines.iter().enumerate().find(|(j, o)| {
            *j != i && o.kind != BlockKind::Heading && (o.y + o.h / 2.0 - (l.y + l.h / 2.0)).abs() < o.h.max(l.h) * 0.8 && o.x + o.w <= l.x + 2.0 && (g.is_none() || o.side == l.side || o.side == Side::Span)
        });
        let Some((host, _)) = host else { continue };
        let (text, x, y, w, h, num, row) = (l.text.trim().to_string(), l.x, l.y, l.w, l.h, l.num.clone(), l.row.clone());
        let target = &mut lines[host];
        target.text = format!("{} {text}", target.text);
        target.w = x + w - target.x;
        let top = target.y.min(y);
        target.h = (target.y + target.h).max(y + h) - top;
        target.y = top;
        if let (Some(hosted), Some(row)) = (&target.row, row) {
            let mut segs = hosted.segs.clone();
            segs.extend(row.segs);
            segs.sort_by(|a, b| a.x.total_cmp(&b.x));
            target.row = Some(RegionRow { y: js_round(top), h: js_round(target.h), segs, angle: None });
        }
        target.kind = BlockKind::Equation;
        target.num = num;
        lines[i].text.clear();
    }
}

fn paragraphs(live: &[&Line], g: Option<f64>, body: f64) -> Vec<Block> {
    // This page's line spacing: double-spaced drafts leave nearly a line between lines, which must not
    // read as a paragraph break.
    let mut pitches: HashMap<Side, Vec<f64>> = HashMap::new();
    let mut by_y: Vec<&&Line> = live.iter().collect();
    by_y.sort_by(|a, b| a.y.total_cmp(&b.y));
    let mut last_of: HashMap<Side, f64> = HashMap::new();
    for l in by_y {
        if let Some(before) = last_of.get(&l.side).filter(|y| l.y > **y) {
            pitches.entry(l.side).or_default().push(l.y - before);
        }
        last_of.insert(l.side, l.y);
    }
    let leading = |s: Side, lh: f64| (lh * 0.9).max((lh * 1.5).min((median(pitches.get(&s).map_or(&[], Vec::as_slice)) - lh) * 1.3));
    let right = |s: Side| live.iter().filter(|l| l.side == s).map(|l| l.x + l.w).fold(f64::NEG_INFINITY, f64::max);
    let rank = |s: Side| match s {
        Side::Span => 0,
        Side::Left => 1,
        Side::Right => 2,
    };
    let mut sorted: Vec<&&Line> = live.iter().collect();
    sorted.sort_by(|a, b| {
        let first = if g.is_none() { a.y.total_cmp(&b.y) } else { rank(a.side).cmp(&rank(b.side)).then(a.y.total_cmp(&b.y)) };
        first.then(a.x.total_cmp(&b.x))
    });
    let mut blocks: Vec<Block> = vec![];
    for l in sorted {
        let row = l.row.clone();
        if let Some(prev) = blocks.iter_mut().rev().find(|b| b.side == l.side) {
            let bottom = prev.y + prev.h;
            // A heading too long for a line: the same size, aligned, directly below.
            if prev.kind == BlockKind::Heading && l.kind == BlockKind::Text && l.h >= body * 1.1 && (l.h - prev.lh).abs() < prev.lh * 0.15 && l.x > prev.x - l.h && l.x < prev.x + l.h * 2.0 && l.y - bottom < l.h * 1.5 {
                prev.text = join_line(&prev.text, &l.text);
                prev.w = (prev.x + prev.w).max(l.x + l.w) - prev.x;
                prev.h = l.y + l.h - prev.y;
                prev.rows.extend(row);
                continue;
            }
            let continues = l.kind == BlockKind::Text || (l.kind == BlockKind::Reference && !REF_ENTRY.is_match(&l.text));
            let accepts = prev.kind == BlockKind::Text || prev.kind == BlockKind::Reference || ((prev.kind == BlockKind::Figure || prev.kind == BlockKind::Table) && prev.lines < 6);
            let close = l.y - bottom < (prev.lh.max(l.h) * 0.9).max(leading(l.side, l.h)) && l.y >= bottom - prev.lh * 0.6;
            // A similar size; a line with a superscript is a little taller and still counts.
            let same_size = prev.lh.max(l.h) / prev.lh.min(l.h) < 1.3 || ((prev.lh - body).abs() < body * 0.25 && l.h < body * 1.7 && l.h > body * 0.8);
            let indented = l.kind == BlockKind::Text && l.x > prev.x + l.h * 0.8;
            // A last line much shorter than this one ended the paragraph. Only much shorter, since an
            // abstract indented on both sides never reaches the page's edge.
            let ended = prev.kind == BlockKind::Text && prev.last_right.is_some_and(|r| r < right(l.side).min(l.x + l.w) - l.h * 2.5);
            if continues && accepts && close && same_size && !indented && !ended {
                prev.text = join_line(&prev.text, &l.text);
                let x0 = prev.x.min(l.x);
                prev.w = (prev.x + prev.w).max(l.x + l.w) - x0;
                prev.x = x0;
                prev.h = l.y + l.h - prev.y;
                prev.lines += 1;
                prev.rows.extend(row);
                prev.last_right = Some(l.x + l.w);
                continue;
            }
        }
        blocks.push(Block { text: l.text.clone(), name: None, x: l.x, y: l.y, w: l.w, h: l.h, kind: l.kind, side: l.side, lh: l.h, lines: 1, num: l.num.clone(), last_right: Some(l.x + l.w), rows: row.into_iter().collect() });
    }
    blocks
}

struct Zone {
    top: f64,
    bottom: f64,
    inside: Vec<usize>,
    x0: f64,
    x1: f64,
}

/// Grow a caption over the figure or table it names, taking in the scattered text there.
fn grow_captions(blocks: &mut [Block], g: Option<f64>, width: f64, height: f64, body: f64, state: &mut PaperState) -> HashSet<usize> {
    let col_width = |s: Side| if g.is_none() || s == Side::Span { width * 0.8 } else { width * 0.4 };
    let per_line = |b: &Block| (utf16_len(&b.text) + HAN.find_iter(&b.text).count()) as f64 / b.lines as f64;
    // Mostly letters, as rows of numbers in a table are not prose.
    let prose = |b: &Block| {
        let chars: String = b.text.chars().filter(|c| !c.is_whitespace()).collect();
        !chars.is_empty() && LETTER.find_iter(&chars).count() as f64 / utf16_len(&chars) as f64 >= 0.6
    };
    let solid = |b: &Block| {
        (b.kind == BlockKind::Text && utf16_len(&b.text) >= 60 && b.w > col_width(b.side) * 0.5 && b.lh >= body * 0.85 && per_line(b) >= 35.0 && prose(b)) || b.kind == BlockKind::Equation || (b.kind == BlockKind::Heading && b.lh >= body * 0.95)
    };
    let is_cap = |b: &Block| b.kind == BlockKind::Figure || b.kind == BlockKind::Table;
    let mut gone: HashSet<usize> = HashSet::new();
    let zone = |blocks: &[Block], gone: &HashSet<usize>, cap: usize, up: bool| -> Zone {
        let c = &blocks[cap];
        let x0 = if c.side == Side::Right && g.is_some() { g.expect("checked") } else { 0.0 };
        let x1 = if c.side == Side::Left && g.is_some() { g.expect("checked") } else { width };
        let overlaps = |i: usize| i != cap && !gone.contains(&i) && blocks[i].x < x1 && blocks[i].x + blocks[i].w > x0;
        // Going this way, a paragraph or another caption is as far as it goes.
        let walls: Vec<&Block> = (0..blocks.len()).filter(|i| overlaps(*i)).map(|i| &blocks[i]).filter(|b| (solid(b) || is_cap(b)) && if up { b.y + b.h <= c.y } else { b.y >= c.y + c.h }).collect();
        let top = if up { walls.iter().map(|b| b.y + b.h).fold(0.0, f64::max) } else { c.y + c.h };
        let bottom = if up { c.y } else { walls.iter().map(|b| b.y).fold(height, f64::min) };
        let inside = (0..blocks.len()).filter(|i| overlaps(*i)).filter(|i| !solid(&blocks[*i]) && !is_cap(&blocks[*i]) && blocks[*i].y >= top - 2.0 && blocks[*i].y + blocks[*i].h <= bottom + 2.0).collect();
        Zone { top, bottom, inside, x0, x1 }
    };
    let caps: Vec<usize> = (0..blocks.len()).filter(|i| is_cap(&blocks[*i])).collect();
    // First a vote: scattered text on one side only settles which way this document goes.
    for cap in &caps {
        let a = zone(blocks, &gone, *cap, true).inside.len();
        let b = zone(blocks, &gone, *cap, false).inside.len();
        let vote = if blocks[*cap].kind == BlockKind::Figure { &mut state.figure_up } else { &mut state.table_up };
        if a > 0 && b == 0 {
            *vote += 1;
        }
        if b > 0 && a == 0 {
            *vote -= 1;
        }
    }
    for cap in caps {
        let above = zone(blocks, &gone, cap, true);
        let below = zone(blocks, &gone, cap, false);
        // Its own side if clear, then this document's habit, then the usual: figure above, table below.
        let figure = blocks[cap].kind == BlockKind::Figure;
        let vote = if figure { state.figure_up } else { state.table_up };
        let up = if !above.inside.is_empty() && below.inside.is_empty() {
            true
        } else if !below.inside.is_empty() && above.inside.is_empty() {
            false
        } else if vote != 0 {
            vote > 0
        } else {
            figure
        };
        let z = if up { above } else { below };
        gone.extend(z.inside.iter().copied());
        if z.bottom - z.top > body * 2.0 {
            let c = &blocks[cap];
            // With no text layer, the figure takes the column's width.
            let (left, right) = if z.inside.is_empty() {
                (c.x.min(z.x0 + 8.0), (c.x + c.w).max(z.x1 - 8.0))
            } else {
                (z.inside.iter().map(|i| blocks[*i].x).fold(c.x, f64::min), z.inside.iter().map(|i| blocks[*i].x + blocks[*i].w).fold(c.x + c.w, f64::max))
            };
            let y0 = if up { z.top + body * 0.5 } else { c.y };
            let y1 = if up { c.y + c.h } else { z.bottom - body * 0.5 };
            let c = &mut blocks[cap];
            c.x = left;
            c.w = right - left;
            c.y = y0;
            c.h = y1 - y0;
        }
        // Lines just after a caption that were split from it, such as a bold "Left:".
        for i in 0..blocks.len() {
            let (cx, cy, cw, ch) = (blocks[cap].x, blocks[cap].y, blocks[cap].w, blocks[cap].h);
            let b = &blocks[i];
            if i != cap && !gone.contains(&i) && !is_cap(b) && b.kind != BlockKind::Heading && b.x >= cx - 4.0 && b.x + b.w <= cx + cw + 4.0 && b.y >= cy - 4.0 && b.y + b.h <= cy + ch + body * 3.0 && b.y < cy + ch && b.lh < body * 1.2 {
                let bottom = b.y + b.h;
                gone.insert(i);
                blocks[cap].h = ch.max(bottom - cy);
            }
        }
        let c = &mut blocks[cap];
        let rest = CAPTION.replace(&c.text, "").trim().to_string();
        let number = c.num.as_deref().unwrap_or_default();
        c.name = Some(format!("{} {number}{}", if figure { "图" } else { "表" }, if rest.is_empty() { String::new() } else { format!("：{rest}") }));
    }
    gone
}

/// Text written one character at a time down a column is put back in columns first, and pieces that
/// run across are left for ordinary line joining.
fn vertical_text<'a>(pieces: &[&'a TextPiece]) -> (Vec<Area>, Vec<&'a TextPiece>) {
    let single = |p: &TextPiece| p.text.trim().chars().count() == 1;
    let mut candidates: Vec<usize> = (0..pieces.len())
        .filter(|i| {
            let p = pieces[*i];
            single(p)
                && p.w >= p.h * 0.3
                && p.w <= p.h * 1.5
                && !pieces.iter().enumerate().any(|(j, q)| j != *i && (q.y + q.h / 2.0 - p.y - p.h / 2.0).abs() < q.h.min(p.h) * 0.25 && (q.x - p.x - p.w).abs().min((p.x - q.x - q.w).abs()) < p.h * 0.35)
        })
        .collect();
    candidates.sort_by(|a, b| pieces[*a].y.total_cmp(&pieces[*b].y).then(pieces[*b].x.total_cmp(&pieces[*a].x)));
    let mut columns: Vec<Vec<usize>> = vec![];
    for i in candidates {
        let p = pieces[i];
        let found = columns.iter_mut().find(|c| {
            let last = pieces[*c.last().expect("a column has a piece")];
            (last.x + last.w / 2.0 - p.x - p.w / 2.0).abs() < last.w.min(p.w) * 0.25 && p.y - last.y >= last.h.min(p.h) * 0.6 && p.y - last.y <= last.h.max(p.h) * 1.35
        });
        match found {
            Some(column) => column.push(i),
            None => columns.push(vec![i]),
        }
    }
    let mut used: HashSet<usize> = HashSet::new();
    let mut areas = vec![];
    let tenth = |v: f64| js_round(v * 10.0) / 10.0;
    for c in 0..columns.len() {
        let first = columns[c][0];
        let cjk = columns[c].iter().filter(|i| CJK.is_match(pieces[**i].text.as_str())).count();
        if used.contains(&first) || columns[c].len() < 3 || cjk < 2 {
            continue;
        }
        let mut group = vec![c];
        used.insert(first);
        let mut at = 0;
        while at < group.len() {
            for other in 0..columns.len() {
                let (a, b) = (pieces[columns[group[at]][0]], pieces[columns[other][0]]);
                let size = a.h.max(b.h);
                if !used.contains(&columns[other][0]) && (a.x - b.x).abs() > size * 0.5 && (a.x - b.x).abs() <= size * 2.5 && (a.y - b.y).abs() <= size * 1.5 {
                    group.push(other);
                    used.insert(columns[other][0]);
                }
            }
            at += 1;
        }
        // Columns read in the order the file stores them.
        group.sort_by_key(|g| columns[*g].iter().copied().min().unwrap_or(usize::MAX));
        let glyphs: Vec<usize> = group.iter().flat_map(|g| columns[*g].iter().copied()).collect();
        used.extend(glyphs.iter().copied());
        let at = |i: &usize| pieces[*i];
        let x = glyphs.iter().map(|i| at(i).x).fold(f64::INFINITY, f64::min);
        let y = glyphs.iter().map(|i| at(i).y).fold(f64::INFINITY, f64::min);
        areas.push(Area {
            text: glyphs.iter().map(|i| at(i).text.trim()).collect(),
            name: None,
            x,
            y,
            w: glyphs.iter().map(|i| at(i).x + at(i).w).fold(f64::NEG_INFINITY, f64::max) - x,
            h: glyphs.iter().map(|i| at(i).y + at(i).h).fold(f64::NEG_INFINITY, f64::max) - y,
            rows: Some(glyphs.iter().map(|i| RegionRow { y: js_round(at(i).y), h: js_round(at(i).h), segs: vec![Seg { t: at(i).text.trim().to_string(), x: tenth(at(i).x), w: tenth(at(i).w) }], angle: None }).collect()),
        });
    }
    let rest = pieces.iter().enumerate().filter(|(i, _)| !used.contains(i)).map(|(_, p)| *p).collect();
    (areas, rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::regions::page_graph;
    use serde_json::{json, Value};

    fn near(a: f64, b: &Value, what: &str) {
        let b = b.as_f64().unwrap_or_else(|| panic!("{what} is not a number: {b}"));
        assert!((a - b).abs() < 1e-6, "{what}: {a} vs {b}");
    }

    #[test]
    fn pages_read_as_the_typescript_engine_reads_them() {
        let goldens: Value = serde_json::from_str(include_str!("../../board/fixtures/goldens.json")).unwrap();
        let documents = goldens["paper"].as_array().unwrap();
        assert!(documents.len() >= 10);
        for document in documents {
            let name = document["name"].as_str().unwrap();
            let mut state = PaperState::default();
            for (p, page) in document["pages"].as_array().unwrap().iter().enumerate() {
                let at = format!("{name} page {p}");
                let pieces: Vec<TextPiece> = serde_json::from_value(page["pieces"].clone()).unwrap();
                let got = analyze_page(&pieces, page["w"].as_f64().unwrap(), page["h"].as_f64().unwrap(), &mut state);
                assert_eq!(json!(got.headings), page["headings"], "headings, {at}");
                assert_eq!(json!(got.columns), page["columns"], "columns, {at}");
                assert_eq!(state.in_refs, page["inRefs"].as_bool().unwrap(), "references, {at}");
                let areas = page["areas"].as_array().unwrap();
                let kinds: Vec<&str> = got.areas.iter().map(|a| a.kind.name()).collect();
                let wanted: Vec<&str> = areas.iter().map(|a| a["kind"].as_str().unwrap()).collect();
                assert_eq!(kinds, wanted, "kinds, {at}");
                for (a, e) in got.areas.iter().zip(areas) {
                    assert_eq!(a.area.text, e["text"].as_str().unwrap(), "text, {at}");
                    assert_eq!(a.area.name.as_deref(), e["name"].as_str(), "name, {at}");
                    near(a.area.x, &e["x"], &format!("x, {at}"));
                    near(a.area.y, &e["y"], &format!("y, {at}"));
                    near(a.area.w, &e["w"], &format!("w, {at}"));
                    near(a.area.h, &e["h"], &format!("h, {at}"));
                    let rows: Option<Vec<RegionRow>> = e.get("rows").map(|r| serde_json::from_value(r.clone()).unwrap());
                    match (&a.area.rows, rows) {
                        (Some(got), Some(rows)) => {
                            assert_eq!(got.len(), rows.len(), "rows, {at}");
                            for (x, y) in got.iter().zip(&rows) {
                                assert_eq!(x.segs.len(), y.segs.len(), "pieces, {at}");
                                near(x.y, &json!(y.y), &format!("row y, {at}"));
                                near(x.h, &json!(y.h), &format!("row h, {at}"));
                                assert_eq!(x.angle, y.angle, "row angle, {at}");
                                for (s, t) in x.segs.iter().zip(&y.segs) {
                                    assert_eq!(s.t, t.t, "{at}");
                                    near(s.x, &json!(t.x), &format!("seg x, {at}"));
                                }
                            }
                        }
                        (None, None) => {}
                        (a, b) => panic!("rows differ, {at}: {a:?} vs {b:?}"),
                    }
                }
                let plain: Vec<Area> = got.areas.iter().map(|a| a.area.clone()).collect();
                let g = page_graph(page["w"].as_f64().unwrap(), page["h"].as_f64().unwrap(), Some(p as u32 + 1), &plain);
                let mut g = serde_json::to_value(&g).unwrap();
                let mut wanted = page["graph"].clone();
                // Rows hold whole numbers as decimals here, which reads the same.
                strip_numbers(&mut g);
                strip_numbers(&mut wanted);
                assert_eq!(g, wanted, "board, {at}");
            }
        }
    }

    fn strip_numbers(v: &mut Value) {
        match v {
            Value::Number(n) => *v = json!(n.as_f64().map(|x| (x * 1e6).round() / 1e6)),
            Value::Array(items) => items.iter_mut().for_each(strip_numbers),
            Value::Object(fields) => fields.values_mut().for_each(strip_numbers),
            _ => {}
        }
    }
}
