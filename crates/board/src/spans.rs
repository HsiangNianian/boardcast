//! Finding a word or sentence in a block of imported text, ported from src/engine/spans.ts. The text
//! layer records each line's pieces and where they are; a character's own position is estimated from
//! its width, which is close enough to circle a word. Case, spacing and a hyphen breaking a word across
//! lines are ignored when comparing, so "automa-" and "tion" match "automation".

use icu_segmenter::options::WordBreakInvariantOptions;
use icu_segmenter::WordSegmenter;
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use unicode_script::{Script, UnicodeScript};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Seg {
    pub t: String,
    pub x: f64,
    pub w: f64,
}

/// One line of imported text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegionRow {
    pub y: f64,
    pub h: f64,
    pub segs: Vec<Seg>,
    /// Radians about the page origin, for text that is not horizontal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpanBox {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
}

/// A mark on a word or sentence, with one box per line it covers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub mark: String,
    pub text: String,
    pub boxes: Vec<SpanBox>,
}

impl Span {
    /// The same mark on the same place; the words may have been read differently.
    pub fn same(&self, other: &Self) -> bool {
        self.mark == other.mark && self.boxes == other.boxes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Word,
    Sentence,
}

pub fn rotate_point(p: [f64; 2], angle: f64) -> [f64; 2] {
    if angle == 0.0 || !angle.is_finite() {
        return p;
    }
    let (sin, cos) = angle.sin_cos();
    [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos]
}

struct Ch {
    c: char,
    row: usize,
    x0: f64,
    x1: f64,
    /// Left out when comparing: whitespace, and a hyphen breaking a word at a line's end.
    skip: bool,
}

fn weight(c: char) -> f64 {
    if "iljtfI.,;:'!|()[]".contains(c) {
        0.6
    } else if "mwMW".contains(c) {
        1.4
    } else if !c.is_ascii() {
        1.9
    } else {
        1.0
    }
}

fn is_cjk(c: char) -> bool {
    matches!(
        c.script(),
        Script::Han | Script::Hiragana | Script::Katakana
    )
}

fn nfkc(s: &str) -> String {
    s.nfkc().collect()
}

fn is_closing(s: &str) -> bool {
    s.chars().any(|c| ")]}'\"”’」』》〉】〕）»".contains(c))
}

fn chars(rows: &[RegionRow]) -> Vec<Ch> {
    let mut out: Vec<Ch> = vec![];
    for (ri, row) in rows.iter().enumerate() {
        let mut last_end: Option<f64> = None;
        for seg in &row.segs {
            // A gap between pieces is a word boundary.
            if let Some(end) = last_end.filter(|end| seg.x - end > row.h * 0.12) {
                out.push(Ch {
                    c: ' ',
                    row: ri,
                    x0: end,
                    x1: seg.x,
                    skip: true,
                });
            }
            let total: f64 = seg.t.chars().map(weight).sum();
            let total = if total == 0.0 { 1.0 } else { total };
            let mut x = seg.x;
            for c in seg.t.chars() {
                let w = weight(c) / total * seg.w;
                out.push(Ch {
                    c,
                    row: ri,
                    x0: x,
                    x1: x + w,
                    skip: c.is_whitespace(),
                });
                x += w;
            }
            last_end = Some(seg.x + seg.w);
        }
        // Chinese lines join with no space. A hyphen at a line's end joins the next line, and other
        // line ends keep the space between words.
        let next = rows
            .get(ri + 1)
            .and_then(|r| r.segs.first())
            .and_then(|s| s.t.chars().next());
        let Some(next) = next else { continue };
        let last = out.last().map(|c| c.c);
        if last == Some('-') && next.is_ascii_lowercase() {
            out.last_mut().expect("just read").skip = true;
        } else {
            let joined = last.is_some_and(|l| {
                is_cjk(l)
                    || is_cjk(next)
                    || nfkc(&format!("{l}{next}"))
                        .chars()
                        .any(|c| "。！？「」『』".contains(c))
            });
            if !joined {
                let end = last_end.unwrap_or(0.0);
                out.push(Ch {
                    c: ' ',
                    row: ri,
                    x0: end,
                    x1: end,
                    skip: true,
                });
            }
        }
    }
    out
}

fn is_letter(c: char) -> bool {
    c.is_alphabetic()
}

/// Whether a sentence ends here. Abbreviations such as "et al." and "Fig." do not end one.
fn ends_sentence(cs: &[Ch], i: usize) -> bool {
    let mut punctuation = i;
    while punctuation > 0
        && (cs[punctuation].skip || is_closing(&nfkc(&cs[punctuation].c.to_string())))
    {
        punctuation -= 1;
    }
    // A closing quote or bracket after the stop belongs to the sentence it closes.
    let mut after = i + 1;
    while cs.get(after).is_some_and(|c| c.skip) {
        after += 1;
    }
    let next = cs
        .get(after)
        .map_or(String::new(), |c| nfkc(&c.c.to_string()));
    if is_closing(&next) || next.chars().any(|c| "。！？?!".contains(c)) {
        return false;
    }
    let stop = nfkc(&cs[punctuation].c.to_string());
    if stop.chars().any(|c| "。！？".contains(c)) {
        return true;
    }
    if !stop.chars().any(|c| ".?!".contains(c)) {
        return false;
    }
    // A citation or a closing bracket may come before the next sentence starts.
    let mut j = i + 1;
    while j < cs.len() && (cs[j].skip || ")\"”’]0123456789,".contains(cs[j].c)) {
        j += 1;
    }
    if j >= cs.len() {
        return true;
    }
    if !(cs[j].c.is_ascii_uppercase() || cs[j].c.script() == Script::Han) {
        return false;
    }
    if !stop.contains('.') {
        return true;
    }
    let mut word = String::new();
    let mut k = punctuation;
    while k > 0 {
        k -= 1;
        if cs[k].skip || !(is_letter(cs[k].c) || cs[k].c == '.') {
            break;
        }
        word.insert(0, cs[k].c);
    }
    let single_capital = word.len() == 1 && word.chars().all(|c| c.is_ascii_uppercase());
    !(single_capital
        || [
            "et", "al", "e.g", "i.e", "cf", "vs", "etc", "Fig", "Figs", "Eq", "Eqs", "Ref", "Refs",
            "Dr", "No",
        ]
        .contains(&word.as_str()))
}

fn sentence_around(cs: &[Ch], a: usize, b: usize) -> (usize, usize) {
    let mut start = a;
    while start > 0 && !ends_sentence(cs, start - 1) {
        start -= 1;
    }
    while start < cs.len() && cs[start].skip {
        start += 1;
    }
    let mut end = b;
    while end < cs.len() && !ends_sentence(cs, end - 1) {
        end += 1;
    }
    (start, end)
}

fn word_around(cs: &[Ch], i: usize) -> (usize, usize) {
    let on_cjk = is_cjk(cs[i].c);
    let word = |c: &Ch| {
        !c.c.is_whitespace()
            && (c.c.is_alphanumeric() || c.c == '_' || c.c == '-')
            && (on_cjk || !is_cjk(c.c))
    };
    // A hyphen breaking a word across lines stays part of the word.
    let inside = |c: &Ch| word(c) || (c.c == '-' && c.skip);
    if !inside(&cs[i]) {
        return (i, i + 1);
    }
    let (mut a, mut b) = (i, i + 1);
    while a > 0 && inside(&cs[a - 1]) {
        a -= 1;
    }
    while b < cs.len() && inside(&cs[b]) {
        b += 1;
    }
    let text: String = cs[a..b].iter().map(|c| c.c).collect();
    if text.chars().any(is_cjk) {
        // Chinese has no spaces, so a dictionary finds the word around this character.
        let offset: usize = cs[a..i].iter().map(|c| c.c.len_utf8()).sum();
        let segmenter = WordSegmenter::new_auto(WordBreakInvariantOptions::default());
        let breaks: Vec<usize> = segmenter.segment_str(&text).collect();
        if let Some(pair) = breaks.windows(2).find(|w| w[0] <= offset && offset < w[1]) {
            let from = text[..pair[0]].chars().count();
            let length = text[pair[0]..pair[1]].chars().count();
            let start = a + from;
            return (start, start + length);
        }
    }
    (a, b)
}

/// JavaScript's `Math.round`, so a stored box matches the older app's exactly.
fn round(v: f64) -> i64 {
    (v + 0.5).floor() as i64
}

fn to_span(cs: &[Ch], a: usize, b: usize, mark: &str, rows: &[RegionRow]) -> Span {
    let mut boxes: Vec<(usize, f64, f64, f64, f64, Option<f64>)> = vec![];
    for c in &cs[a..b] {
        if c.c.is_whitespace() {
            continue;
        }
        let row = &rows[c.row];
        match boxes.iter_mut().find(|bx| bx.0 == c.row) {
            Some(bx) => bx.3 = (bx.1 + bx.3).max(c.x1) - bx.1,
            None => boxes.push((
                c.row,
                c.x0,
                row.y,
                c.x1 - c.x0,
                row.h,
                row.angle.filter(|v| *v != 0.0),
            )),
        }
    }
    let text: String = cs[a..b]
        .iter()
        .filter(|c| !(c.c == '-' && c.skip))
        .map(|c| c.c)
        .collect();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let boxes = boxes
        .into_iter()
        .map(|(_, x, y, w, h, angle)| SpanBox {
            x: round(x),
            y: round(y),
            w: round(w),
            h: round(h),
            angle,
        })
        .collect();
    Span {
        mark: mark.to_string(),
        text,
        boxes,
    }
}

fn folded(s: &str) -> String {
    s.nfkd().collect::<String>().to_lowercase()
}

/// Find a quote in the text; for a sentence, widen to the whole sentence around it.
pub fn locate(rows: &[RegionRow], quote: &str, mark: &str, unit: Unit) -> Option<Span> {
    let cs = chars(rows);
    let mut hay: Vec<char> = vec![];
    let mut index: Vec<usize> = vec![];
    for (i, c) in cs.iter().enumerate() {
        if c.skip {
            continue;
        }
        for lower in folded(&c.c.to_string()).chars() {
            hay.push(lower);
            index.push(i);
        }
    }
    let quote: Vec<char> = folded(quote)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if quote.is_empty() {
        return None;
    }
    let at = hay.windows(quote.len()).position(|w| w == quote)?;
    let (mut a, mut b) = (index[at], index[at + quote.len() - 1] + 1);
    if unit == Unit::Sentence {
        (a, b) = sentence_around(&cs, a, b);
    }
    Some(to_span(&cs, a, b, mark, rows))
}

/// Find what a point on the page is touching, for marking by hand.
pub fn span_at(rows: &[RegionRow], x: f64, y: f64, mark: &str, unit: Unit) -> Option<Span> {
    let cs = chars(rows);
    let points: Vec<[f64; 2]> = rows
        .iter()
        .map(|r| rotate_point([x, y], -r.angle.unwrap_or(0.0)))
        .collect();
    let distance = |i: usize| {
        let (r, p) = (&rows[i], points[i]);
        let left = r.segs.iter().map(|s| s.x).fold(f64::INFINITY, f64::min);
        let right = r
            .segs
            .iter()
            .map(|s| s.x + s.w)
            .fold(f64::NEG_INFINITY, f64::max);
        let dx = (left - p[0]).max(p[0] - right).max(0.0);
        dx * dx + (r.y + r.h / 2.0 - p[1]).powi(2)
    };
    let mut row = 0;
    for i in 1..rows.len() {
        if distance(i) < distance(row) {
            row = i;
        }
    }
    let local_x = points.get(row).map_or(x, |p| p[0]);
    let mut hit = None;
    let mut best = f64::INFINITY;
    for (i, c) in cs.iter().enumerate() {
        if c.row != row || c.c.is_whitespace() {
            continue;
        }
        let d = if local_x < c.x0 {
            c.x0 - local_x
        } else if local_x > c.x1 {
            local_x - c.x1
        } else {
            0.0
        };
        if d < best {
            best = d;
            hit = Some(i);
        }
    }
    let hit = hit?;
    let (a, b) = if unit == Unit::Word {
        word_around(&cs, hit)
    } else {
        sentence_around(&cs, hit, hit + 1)
    };
    Some(to_span(&cs, a, b, mark, rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn locating_matches_the_typescript_engine() {
        let goldens: Value =
            serde_json::from_str(include_str!("../fixtures/goldens.json")).unwrap();
        let cases = goldens["spans"].as_array().unwrap();
        assert!(cases.len() >= 30);
        for case in cases {
            let rows: Vec<RegionRow> = serde_json::from_value(case["rows"].clone()).unwrap();
            let unit = if case["unit"] == "sentence" {
                Unit::Sentence
            } else {
                Unit::Word
            };
            let mark = case["mark"].as_str().unwrap();
            let got = match case["quote"].as_str() {
                Some(quote) => locate(&rows, quote, mark, unit),
                None => span_at(
                    &rows,
                    case["x"].as_f64().unwrap(),
                    case["y"].as_f64().unwrap(),
                    mark,
                    unit,
                ),
            };
            assert_eq!(
                serde_json::to_value(&got).unwrap(),
                case["span"],
                "{}",
                case["name"]
            );
        }
    }
}
