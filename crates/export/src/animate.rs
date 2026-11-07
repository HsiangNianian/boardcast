//! The animated SVG, ported from src/ink/animate.ts: the final board written stroke by stroke in the
//! order of the meeting, then left whole. CSS only, so a viewer without animation shows the whole board.

use crate::ink::Composed;
use boardcast_board::pen::{outline, Rng};
use boardcast_board::compose::PAPER;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Write;

/// The same pen speed as on the board, in pixels per millisecond, with a shortest stroke and a pause between strokes.
const SPEED: f64 = 0.9;
const MIN_STROKE: f64 = 45.0;
const GAP: f64 = 35.0;
/// A little longer between the model's turns, so the rhythm shows.
const PAUSE: f64 = 300.0;
/// However long the meeting was, the board is written in this long at most.
pub const TOTAL: f64 = 12000.0;
const PADDING: f64 = 48.0;
const MASK_WIDTH: f32 = 26.0;

const MARKS: [&str; 5] = ["划掉", "圈出", "下划线", "星标", "高亮"];

/// The node, edge (`from>to`), group or region that a piece of ink belongs to, from the start of its key.
pub fn owner_of(key: &str) -> String {
    let head = key.split('|').next().unwrap_or_default();
    let node = |s: &str| s.strip_prefix('n').is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
    if let Some(at) = head.find(['>', '~']) {
        let (from, to) = (&head[..at], &head[at + 1..]);
        let to = to.strip_suffix('l').filter(|t| node(t)).unwrap_or(to);
        if node(from) && node(to) {
            return format!("{from}>{to}");
        }
    }
    if let Some(digits) = head.strip_prefix('g').and_then(|h| h.strip_suffix('t')) {
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return head[..head.len() - 1].to_string();
        }
    }
    head.to_string()
}

/// A mark has a letter at the end of its key; on imported text, the second part is the mark itself.
fn mark_of(key: &str) -> Option<&str> {
    let parts: Vec<&str> = key.split('|').collect();
    if parts.len() < 2 {
        return None;
    }
    if let Some(mark) = MARKS.iter().find(|m| **m == parts[1]) {
        return Some(mark);
    }
    match parts[parts.len() - 1] {
        "x" => Some("划掉"),
        "u" => Some("下划线"),
        "o" => Some("圈出"),
        "s" => Some("星标"),
        "h" => Some("高亮"),
        _ => None,
    }
}

fn number_in(id: &str) -> f64 {
    let digits: String = id.chars().skip_while(|c| !c.is_ascii_digit()).take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap_or(0.0)
}

/// Which round of the meeting each piece of ink appeared in, smallest first: when its node, edge or
/// group first shows in the record, and for a mark, when it was added. Things with no record come last.
pub fn ranks(nodes: &[Value], regions: &[(String, Vec<String>)], steps: &[Value], keys: &[String]) -> Vec<f64> {
    let mut first: HashMap<String, f64> = HashMap::new();
    let mut marked: HashMap<String, f64> = HashMap::new();
    let set = |step: &Value, part: &str| step["d"][part]["set"].as_array().cloned().unwrap_or_default();
    for (i, step) in steps.iter().enumerate() {
        let i = i as f64;
        for n in set(step, "nodes") {
            let id = n["id"].as_str().unwrap_or_default();
            first.entry(id.to_string()).or_insert(i);
            for m in n["marks"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                marked.entry(format!("{id}|{m}")).or_insert(i);
            }
            for s in n["spans"].as_array().into_iter().flatten() {
                marked.entry(format!("{id}|{}", s["mark"].as_str().unwrap_or_default())).or_insert(i);
            }
        }
        for e in set(step, "edges") {
            first.entry(format!("{}>{}", e["from"].as_str().unwrap_or_default(), e["to"].as_str().unwrap_or_default())).or_insert(i);
        }
        for g in set(step, "groups") {
            first.entry(g["id"].as_str().unwrap_or_default().to_string()).or_insert(i);
        }
    }
    let late = steps.len() as f64;
    let node = |id: &str| first.get(id).copied().unwrap_or(late + number_in(id));
    let known = |id: &str| nodes.iter().any(|n| n["id"] == id);
    let rank_of = |owner: &str| -> f64 {
        if let Some(rank) = first.get(owner) {
            return *rank;
        }
        if known(owner) {
            return node(owner);
        }
        // An edge cannot come before both of its ends.
        if let Some((from, to)) = owner.split_once('>') {
            return node(from).max(node(to));
        }
        // A group follows its earliest member, and an unnamed region its earliest row.
        let earliest = |ids: Vec<f64>| ids.into_iter().reduce(f64::min);
        let members = nodes.iter().filter(|n| n["group"] == owner).map(|n| node(n["id"].as_str().unwrap_or_default())).collect();
        if let Some(rank) = earliest(members) {
            return rank;
        }
        let rows = regions.iter().find(|(id, _)| id == owner).map_or(vec![], |(_, rows)| rows.iter().map(|r| node(r)).collect());
        earliest(rows).unwrap_or(late + 1e6)
    };
    keys.iter()
        .map(|key| {
            let owner = owner_of(key);
            let base = rank_of(&owner);
            match mark_of(key).filter(|_| known(&owner)) {
                Some(mark) => base.max(marked.get(&format!("{owner}|{mark}")).copied().unwrap_or(base)),
                None => base,
            }
        })
        .collect()
}

/// What the timing needs to know about a piece.
pub struct Timing {
    pub under: bool,
    pub lengths: Vec<f64>,
}

/// When each stroke starts, and how long it takes, in milliseconds.
#[derive(Debug, PartialEq)]
pub struct Timed {
    pub delays: Vec<f64>,
    pub durations: Vec<f64>,
}

/// One stroke after another in rank order, sped up as a whole if it would run past `total`.
pub fn timeline(pieces: &[Timing], rank: &[f64], total: f64) -> (Vec<Timed>, f64) {
    let mut order: Vec<usize> = (0..pieces.len()).collect();
    order.sort_by(|a, b| rank[*a].total_cmp(&rank[*b]));
    let stroke_time = |length: f64| (length / SPEED).max(MIN_STROKE);
    let mut turns: Vec<f64> = rank.to_vec();
    turns.sort_by(f64::total_cmp);
    turns.dedup();
    let natural: f64 = pieces.iter().filter(|p| !p.under).flat_map(|p| &p.lengths).map(|l| stroke_time(*l) + GAP).sum::<f64>() + (turns.len() as f64 - 1.0).max(0.0) * PAUSE;
    let k = if natural > total { total / natural } else { 1.0 };
    let mut at = 0.0;
    let mut last = None;
    let mut timed: Vec<Option<Timed>> = pieces.iter().map(|_| None).collect();
    for i in order {
        let piece = &pieces[i];
        if last.is_some_and(|l| l != rank[i]) {
            at += PAUSE * k;
        }
        last = Some(rank[i]);
        // A highlight is not written; it appears with its round.
        if piece.under {
            timed[i] = Some(Timed { delays: piece.lengths.iter().map(|_| at.round()).collect(), durations: piece.lengths.iter().map(|_| 0.0).collect() });
            continue;
        }
        let (mut delays, mut durations) = (vec![], vec![]);
        for length in &piece.lengths {
            let duration = stroke_time(*length) * k;
            delays.push(at.round());
            durations.push(duration.round());
            at += duration + GAP * k;
        }
        timed[i] = Some(Timed { delays, durations });
    }
    (timed.into_iter().map(|t| t.expect("every piece is in the order")).collect(), at.round())
}

fn path(points: &[[f32; 2]]) -> String {
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        let _ = write!(d, "{}{:.2} {:.2}", if i == 0 { 'M' } else { 'L' }, p[0], p[1]);
    }
    d
}

/// Written from the final board, which is laid out as the file will show it.
pub fn animated_svg(board: &Composed, nodes: &[Value], steps: &[Value], title: &str) -> String {
    struct Written {
        d: String,
        center: String,
        length: f32,
        width: f32,
    }
    let mut written: Vec<Vec<Written>> = vec![];
    for piece in &board.pieces {
        let mut r = Rng::new(&piece.seed);
        written.push(
            piece
                .lines
                .iter()
                .map(|line| {
                    let inked = line.inked(&mut r);
                    Written { d: format!("{}Z", path(&outline(&inked, 1.0))), center: path(&inked.center), length: inked.length, width: inked.width }
                })
                .collect(),
        );
    }
    let timing: Vec<Timing> = board
        .pieces
        .iter()
        .zip(&written)
        .map(|(p, w)| Timing { under: p.lines.first().is_some_and(|l| l.style.under), lengths: w.iter().map(|s| f64::from(s.length)).collect() })
        .collect();
    let keys: Vec<String> = board.pieces.iter().map(|p| p.seed.clone()).collect();
    let (timed, _) = timeline(&timing, &ranks(nodes, &board.regions, steps, &keys), TOTAL);
    let b = &board.bounds;
    let (x, y, w, h) = (b.x0 - PADDING, b.y0 - PADDING, b.x1 - b.x0 + 2.0 * PADDING, b.y1 - b.y0 + 2.0 * PADDING);
    let mut out = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{x} {y} {w} {h}\" width=\"{}\" height=\"{}\">\n<title>{}</title>\n", w.round(), h.round(), crate::ink::xml_text(title));
    out.push_str(concat!(
        "<style>\n",
        "@keyframes w{from{stroke-dashoffset:var(--length)}to{stroke-dashoffset:0}}\n",
        "@keyframes pop{from{opacity:0}to{opacity:1}}\n",
        ".w{fill:none;stroke:#fff;stroke-linecap:round;stroke-linejoin:round;animation-name:w;animation-timing-function:linear;animation-fill-mode:both}\n",
        ".pop{animation:pop 1ms both}\n",
        "@media (prefers-reduced-motion:reduce){.w{animation:none;stroke-dashoffset:0}.pop{animation:none}}\n",
        "</style>\n",
    ));
    let _ = writeln!(out, "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"#{PAPER:06x}\"/>");
    if let Some(page) = &board.page {
        let _ = writeln!(out, "<image href=\"{}\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\"/>", page.image.data_url(), page.w, page.h);
    }
    let mut mask = 0;
    for under in [true, false] {
        for (i, piece) in board.pieces.iter().enumerate() {
            let Some(style) = piece.lines.first().map(|l| l.style).filter(|s| s.under == under) else { continue };
            let blend = if under { " style=\"mix-blend-mode:multiply\"" } else { "" };
            let _ = write!(out, "<g fill=\"#{:06x}\" opacity=\"{}\"{blend}>", style.rgb, style.alpha);
            for (j, stroke) in written[i].iter().enumerate() {
                let (delay, duration) = (timed[i].delays[j], timed[i].durations[j]);
                if duration == 0.0 {
                    if delay > 0.0 {
                        let _ = write!(out, "<path d=\"{}\" class=\"pop\" style=\"animation-delay:{delay}ms\"/>", stroke.d);
                    } else {
                        let _ = write!(out, "<path d=\"{}\"/>", stroke.d);
                    }
                    continue;
                }
                let length = stroke.length + 1.0;
                // Wide enough to uncover the whole stroke, or a wide one would stay clipped for good.
                let reveal = MASK_WIDTH.max(stroke.width * 3.0);
                let _ = write!(
                    out,
                    "<mask id=\"m{mask}\" maskUnits=\"userSpaceOnUse\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"><path class=\"w\" d=\"{}\" stroke-width=\"{reveal}\" stroke-dasharray=\"{length:.1}\" stroke-dashoffset=\"0\" style=\"--length:{length:.1};animation-duration:{duration}ms;animation-delay:{delay}ms\"/></mask><path d=\"{}\" mask=\"url(#m{mask})\"/>",
                    stroke.center, stroke.d,
                );
                mask += 1;
            }
            out.push_str("</g>\n");
        }
    }
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goldens() -> Value {
        serde_json::from_str(include_str!("../../board/fixtures/goldens.json")).unwrap()
    }

    #[test]
    fn ranks_match_the_typescript_engine() {
        let g = goldens();
        let cases = g["ranks"].as_array().unwrap();
        assert!(cases.len() >= 8);
        for case in cases {
            let regions: Vec<(String, Vec<String>)> = serde_json::from_value(case["regions"].clone()).unwrap();
            let keys: Vec<String> = serde_json::from_value(case["keys"].clone()).unwrap();
            let got = ranks(case["nodes"].as_array().unwrap(), &regions, case["steps"].as_array().unwrap(), &keys);
            let expected: Vec<f64> = serde_json::from_value(case["ranks"].clone()).unwrap();
            for (i, (a, b)) in got.iter().zip(&expected).enumerate() {
                assert_eq!(a, b, "{} key {}", case["file"], keys[i]);
            }
        }
    }

    #[test]
    fn timing_matches_the_typescript_engine() {
        let g = goldens();
        for case in g["timelines"].as_array().unwrap() {
            let pieces: Vec<Timing> = case["pieces"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| Timing { under: p["under"].as_bool().unwrap_or(false), lengths: serde_json::from_value(p["lengths"].clone()).unwrap() })
                .collect();
            let rank: Vec<f64> = serde_json::from_value(case["rank"].clone()).unwrap();
            let (timed, end) = timeline(&pieces, &rank, case["total"].as_f64().unwrap());
            assert_eq!(end, case["end"].as_f64().unwrap());
            for (t, e) in timed.iter().zip(case["timed"].as_array().unwrap()) {
                let delays: Vec<f64> = serde_json::from_value(e["delays"].clone()).unwrap();
                let durations: Vec<f64> = serde_json::from_value(e["durs"].clone()).unwrap();
                assert_eq!(t.delays, delays);
                assert_eq!(t.durations, durations);
            }
        }
    }

    #[test]
    fn owners_and_marks_are_read_from_keys() {
        assert_eq!(owner_of("n1>n2l|bg"), "n1>n2");
        assert_eq!(owner_of("n1~n2"), "n1>n2");
        assert_eq!(owner_of("g3t|名字"), "g3");
        assert_eq!(owner_of("n4|box"), "n4");
        assert_eq!(mark_of("n4|文字|h"), Some("高亮"));
        assert_eq!(mark_of("n4|圈出"), Some("圈出"));
        assert_eq!(mark_of("n4"), None);
    }
}
