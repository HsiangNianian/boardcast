//! The animated SVG, ported from src/ink/animate.ts: the final board written stroke by stroke in the
//! order of the meeting, then left whole. CSS only, so a viewer without animation shows the whole board.

use crate::ink::Composed;
use boardcast_board::compose::PAPER;
use boardcast_board::pen::{outline, Rng};
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
    let node = |s: &str| {
        s.strip_prefix('n')
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
    };
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
    let digits: String = id
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().unwrap_or(0.0)
}

/// Which round of the meeting each piece of ink appeared in, smallest first: when its node, edge or
/// group first shows in the record, and for a mark, when it was added. Things with no record come last.
pub fn ranks(
    nodes: &[Value],
    regions: &[(String, Vec<String>)],
    steps: &[Value],
    keys: &[String],
) -> Vec<f64> {
    let mut first: HashMap<String, f64> = HashMap::new();
    let mut marked: HashMap<String, f64> = HashMap::new();
    let set = |step: &Value, part: &str| {
        step["d"][part]["set"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    };
    for (i, step) in steps.iter().enumerate() {
        let i = i as f64;
        for n in set(step, "nodes") {
            let id = n["id"].as_str().unwrap_or_default();
            first.entry(id.to_string()).or_insert(i);
            for m in n["marks"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                marked.entry(format!("{id}|{m}")).or_insert(i);
            }
            for s in n["spans"].as_array().into_iter().flatten() {
                marked
                    .entry(format!("{id}|{}", s["mark"].as_str().unwrap_or_default()))
                    .or_insert(i);
            }
        }
        for e in set(step, "edges") {
            first
                .entry(format!(
                    "{}>{}",
                    e["from"].as_str().unwrap_or_default(),
                    e["to"].as_str().unwrap_or_default()
                ))
                .or_insert(i);
        }
        for g in set(step, "groups") {
            first
                .entry(g["id"].as_str().unwrap_or_default().to_string())
                .or_insert(i);
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
        let members = nodes
            .iter()
            .filter(|n| n["group"] == owner)
            .map(|n| node(n["id"].as_str().unwrap_or_default()))
            .collect();
        if let Some(rank) = earliest(members) {
            return rank;
        }
        let rows = regions
            .iter()
            .find(|(id, _)| id == owner)
            .map_or(vec![], |(_, rows)| rows.iter().map(|r| node(r)).collect());
        earliest(rows).unwrap_or(late + 1e6)
    };
    keys.iter()
        .map(|key| {
            let owner = owner_of(key);
            let base = rank_of(&owner);
            match mark_of(key).filter(|_| known(&owner)) {
                Some(mark) => base.max(
                    marked
                        .get(&format!("{owner}|{mark}"))
                        .copied()
                        .unwrap_or(base),
                ),
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
    let natural: f64 = pieces
        .iter()
        .filter(|p| !p.under)
        .flat_map(|p| &p.lengths)
        .map(|l| stroke_time(*l) + GAP)
        .sum::<f64>()
        + (turns.len() as f64 - 1.0).max(0.0) * PAUSE;
    let k = if natural > total {
        total / natural
    } else {
        1.0
    };
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
            timed[i] = Some(Timed {
                delays: piece.lengths.iter().map(|_| at.round()).collect(),
                durations: piece.lengths.iter().map(|_| 0.0).collect(),
            });
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
    (
        timed
            .into_iter()
            .map(|t| t.expect("every piece is in the order"))
            .collect(),
        at.round(),
    )
}

fn path(points: &[[f32; 2]]) -> String {
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        let _ = write!(
            d,
            "{}{:.2} {:.2}",
            if i == 0 { 'M' } else { 'L' },
            p[0],
            p[1]
        );
    }
    d
}

/// GIF frames a second, the longest side in pixels, and how long the finished board holds before it loops.
const GIF_FPS: f64 = 10.0;
const GIF_SIDE: f64 = 960.0;
const GIF_HOLD_MS: u16 = 2500;

/// The animated board as a GIF: the same strokes in the same order and time as the animated SVG, written
/// frame by frame, then the whole board held a moment before it starts again.
pub fn gif(board: &Composed, nodes: &[Value], steps: &[Value]) -> anyhow::Result<Vec<u8>> {
    use tiny_skia::{Color, FilterQuality, Pixmap, PixmapPaint, Transform};

    let mut inked = vec![];
    for piece in &board.pieces {
        let mut r = Rng::new(&piece.seed);
        inked.push(
            piece
                .lines
                .iter()
                .map(|line| (line.inked(&mut r), line.style))
                .collect::<Vec<_>>(),
        );
    }
    let timing: Vec<Timing> = board
        .pieces
        .iter()
        .zip(&inked)
        .map(|(p, lines)| Timing {
            under: p.lines.first().is_some_and(|l| l.style.under),
            lengths: lines.iter().map(|(i, _)| f64::from(i.length)).collect(),
        })
        .collect();
    let keys: Vec<String> = board.pieces.iter().map(|p| p.seed.clone()).collect();
    let (timed, end) = timeline(&timing, &ranks(nodes, &board.regions, steps, &keys), TOTAL);

    let b = &board.bounds;
    let (x, y, w, h) = (
        b.x0 - PADDING,
        b.y0 - PADDING,
        b.x1 - b.x0 + 2.0 * PADDING,
        b.y1 - b.y0 + 2.0 * PADDING,
    );
    let scale = (GIF_SIDE / w.max(h)).min(2.0);
    let (pw, ph) = (
        (w * scale).round().max(1.0) as u32,
        (h * scale).round().max(1.0) as u32,
    );
    let at = Transform::from_translate(-x as f32, -y as f32).post_scale(scale as f32, scale as f32);
    // The paper and any page under the board, drawn once.
    let mut base =
        Pixmap::new(pw, ph).ok_or_else(|| anyhow::anyhow!("the GIF is too big to make"))?;
    let [_, r, g, bl] = PAPER.to_be_bytes();
    base.fill(Color::from_rgba8(r, g, bl, 255));
    if let Some(page) = &board.page {
        let picture = crate::ink::page_pixmap(&page.image)?;
        let fit = Transform::from_scale(
            (page.w / f64::from(picture.width())) as f32,
            (page.h / f64::from(picture.height())) as f32,
        )
        .post_concat(at);
        base.draw_pixmap(
            0,
            0,
            picture.as_ref(),
            &PixmapPaint {
                quality: FilterQuality::Bilinear,
                ..PixmapPaint::default()
            },
            fit,
            None,
        );
    }

    let mut bytes = vec![];
    {
        let mut encoder = gif::Encoder::new(&mut bytes, pw as u16, ph as u16, &[])?;
        encoder.set_repeat(gif::Repeat::Infinite)?;
        // Each frame is only what changed, laid over the frames before.
        let write = |encoder: &mut gif::Encoder<&mut Vec<u8>>,
                     mut rgba: Vec<u8>,
                     (left, top, w, h): (u32, u32, u32, u32),
                     ms: u32|
         -> anyhow::Result<()> {
            let mut frame = gif::Frame::from_rgba_speed(w as u16, h as u16, &mut rgba, 10);
            (frame.left, frame.top) = (left as u16, top as u16);
            frame.delay = (ms / 10).min(u32::from(u16::MAX)) as u16;
            frame.dispose = gif::DisposalMethod::Keep;
            encoder.write_frame(&frame)?;
            Ok(())
        };
        let step = 1000.0 / GIF_FPS;
        let frames = (end / step).ceil() as usize + 1;
        let mut previous: Option<Vec<u8>> = None;
        let mut pending: Option<(Vec<u8>, (u32, u32, u32, u32), u32)> = None;
        for f in 0..frames {
            let t = f as f64 * step;
            let mut canvas = base.clone();
            for under in [true, false] {
                for (i, lines) in inked.iter().enumerate() {
                    for (j, (ink, style)) in lines
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, s))| s.under == under)
                    {
                        let (delay, duration) = (timed[i].delays[j], timed[i].durations[j]);
                        let k = if duration <= 0.0 {
                            if t >= delay {
                                1.0
                            } else {
                                0.0
                            }
                        } else {
                            ((t - delay) / duration).clamp(0.0, 1.0)
                        };
                        if k > 0.0 {
                            crate::ink::fill_stroke(
                                &mut canvas,
                                &outline(ink, k as f32),
                                *style,
                                at,
                            );
                        }
                    }
                }
            }
            let pixels = canvas.take();
            // Only what changed since the frame before is stored, over it; a frame with no change only
            // lengthens the one before.
            let changed = match &previous {
                None => Some((0, 0, pw, ph)),
                Some(before) => changed_area(before, &pixels, pw, ph),
            };
            match changed {
                None => pending.as_mut().expect("a first frame is always stored").2 += step as u32,
                Some(area) => {
                    if let Some((rgba, at, ms)) = pending.take() {
                        write(&mut encoder, rgba, at, ms)?;
                    }
                    let (left, top, cw, ch) = area;
                    let mut part = Vec::with_capacity((cw * ch * 4) as usize);
                    for y in top..top + ch {
                        let row = ((y * pw + left) * 4) as usize;
                        part.extend_from_slice(&pixels[row..row + (cw * 4) as usize]);
                    }
                    pending = Some((part, area, step as u32));
                }
            }
            previous = Some(pixels);
        }
        if let Some((rgba, at, ms)) = pending.take() {
            write(&mut encoder, rgba, at, ms.max(u32::from(GIF_HOLD_MS)))?;
        }
    }
    Ok(bytes)
}

/// The smallest rectangle holding every pixel that differs between two frames, if any does.
fn changed_area(a: &[u8], b: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..h {
        let row = (y * w * 4) as usize..((y + 1) * w * 4) as usize;
        if a[row.clone()] == b[row.clone()] {
            continue;
        }
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if a[i..i + 4] != b[i..i + 4] {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    (x0 <= x1).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
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
                    Written {
                        d: format!("{}Z", path(&outline(&inked, 1.0))),
                        center: path(&inked.center),
                        length: inked.length,
                        width: inked.width,
                    }
                })
                .collect(),
        );
    }
    let timing: Vec<Timing> = board
        .pieces
        .iter()
        .zip(&written)
        .map(|(p, w)| Timing {
            under: p.lines.first().is_some_and(|l| l.style.under),
            lengths: w.iter().map(|s| f64::from(s.length)).collect(),
        })
        .collect();
    let keys: Vec<String> = board.pieces.iter().map(|p| p.seed.clone()).collect();
    let (timed, _) = timeline(&timing, &ranks(nodes, &board.regions, steps, &keys), TOTAL);
    let b = &board.bounds;
    let (x, y, w, h) = (
        b.x0 - PADDING,
        b.y0 - PADDING,
        b.x1 - b.x0 + 2.0 * PADDING,
        b.y1 - b.y0 + 2.0 * PADDING,
    );
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
    let _ = writeln!(
        out,
        "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"#{PAPER:06x}\"/>"
    );
    if let Some(page) = &board.page {
        let _ = writeln!(out, "<image href=\"{}\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\"/>", page.image.data_url(), page.w, page.h);
    }
    let mut mask = 0;
    for under in [true, false] {
        for (i, piece) in board.pieces.iter().enumerate() {
            let Some(style) = piece
                .lines
                .first()
                .map(|l| l.style)
                .filter(|s| s.under == under)
            else {
                continue;
            };
            let blend = if under {
                " style=\"mix-blend-mode:multiply\""
            } else {
                ""
            };
            let _ = write!(
                out,
                "<g fill=\"#{:06x}\" opacity=\"{}\"{blend}>",
                style.rgb, style.alpha
            );
            for (j, stroke) in written[i].iter().enumerate() {
                let (delay, duration) = (timed[i].delays[j], timed[i].durations[j]);
                if duration == 0.0 {
                    if delay > 0.0 {
                        let _ = write!(
                            out,
                            "<path d=\"{}\" class=\"pop\" style=\"animation-delay:{delay}ms\"/>",
                            stroke.d
                        );
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

    #[test]
    fn the_gif_writes_the_board_out_and_holds_it() {
        use image::AnimationDecoder;
        let mut g = boardcast_board::graph::Graph::default();
        g.apply(&boardcast_board::graph::parse_ops(
            "T 「Release checklist」\nL 「Release checklist」 「Load testing」\nM n2 星标",
        ));
        let mut glyphs = boardcast_board::glyphs::Glyphs::from_checkout(std::path::Path::new(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../.."),
        ))
        .unwrap();
        let board = crate::ink::compose_board(&g, &Value::Null, None, &mut glyphs);
        let nodes: Vec<Value> = g
            .nodes
            .iter()
            .map(|n| serde_json::to_value(n).unwrap())
            .collect();
        let bytes = gif(&board, &nodes, &[]).unwrap();
        let frames: Vec<image::Frame> =
            image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes))
                .unwrap()
                .into_frames()
                .collect_frames()
                .unwrap();
        assert!(frames.len() > 5, "{} frames", frames.len());
        let ink = |f: &image::Frame| f.buffer().pixels().filter(|p| p.0[0] < 128).count();
        assert!(
            ink(&frames[0]) < ink(frames.last().unwrap()),
            "the first frame has less ink than the last"
        );
        let (hold, _) = frames.last().unwrap().delay().numer_denom_ms();
        assert_eq!(hold, u32::from(GIF_HOLD_MS));
    }

    fn goldens() -> Value {
        serde_json::from_str(include_str!("../../board/fixtures/goldens.json")).unwrap()
    }

    #[test]
    fn ranks_match_the_typescript_engine() {
        let g = goldens();
        let cases = g["ranks"].as_array().unwrap();
        assert!(cases.len() >= 8);
        for case in cases {
            let regions: Vec<(String, Vec<String>)> =
                serde_json::from_value(case["regions"].clone()).unwrap();
            let keys: Vec<String> = serde_json::from_value(case["keys"].clone()).unwrap();
            let got = ranks(
                case["nodes"].as_array().unwrap(),
                &regions,
                case["steps"].as_array().unwrap(),
                &keys,
            );
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
                .map(|p| Timing {
                    under: p["under"].as_bool().unwrap_or(false),
                    lengths: serde_json::from_value(p["lengths"].clone()).unwrap(),
                })
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
