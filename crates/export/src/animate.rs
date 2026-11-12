//! The animated board, as the app replays it: each step of the meeting laid out as it stood then, new
//! ink written stroke by stroke, and what moved or went away redrawn where it now is. The GIF draws it
//! frame by frame; the SVG does it in CSS, so a viewer without animation shows the finished board.

use crate::ink::Page;
use crate::replay_page;
use boardcast_board::board_layout::StableLayout;
use boardcast_board::compose::{compose, Style, PAPER};
use boardcast_board::glyphs::Glyphs;
use boardcast_board::graph::Graph;
use boardcast_board::pen::{outline, Inked, Rng};
use boardcast_board::steps::{apply_step, ink_only};
use boardcast_board::sugiyama::first_diagram;
use std::collections::{HashMap, HashSet};
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

/// A stroke of the replay: its ink, when the pen starts on it and how long it takes, and when it goes
/// away again because its piece moved or was taken off.
pub struct Played {
    pub inked: Inked,
    pub style: Style,
    pub delay: f64,
    pub duration: f64,
    pub until: Option<f64>,
    /// The same stroke written earlier, and how far this one is moved from it, when it only moved.
    pub copy_of: Option<(usize, f32, f32)>,
}

/// A whole replay of one board: its strokes, the area it ever covers, the page under it, and how long it runs.
pub struct Replay {
    pub strokes: Vec<Played>,
    pub frame: (f64, f64, f64, f64),
    pub page: Option<Page>,
    pub end: f64,
}

impl Replay {
    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty() && self.page.is_none()
    }
}

/// Lay the board out step by step, as the app's replay does, and time each new piece of ink.
pub fn replay(page: &replay_page::Page, glyphs: &mut Glyphs) -> Replay {
    let mut graphs = vec![];
    let mut g = Graph::default();
    for step in &page.steps {
        g = apply_step(&g, step);
        graphs.push(ink_only(&g));
    }
    // The record can end short of the board as kept, which is what the replay finishes on.
    graphs.push(ink_only(&page.graph));
    let stroke_time = |length: f64| (length / SPEED).max(MIN_STROKE);
    // Each part starts where it finally ended up, so the board ends as it is kept and its parts do not
    // wander about; rows still go in where they were said, pushing what is under them down.
    let mut kept = StableLayout::default();
    kept.restore(&page.memory);
    let mut layout = StableLayout::default();
    let mut strokes: Vec<Played> = vec![];
    // What is on the board now: each piece by its key, with its seed, its look and its strokes in `strokes`.
    let mut shown: HashMap<String, (String, Style, Vec<usize>)> = HashMap::new();
    let mut area: Option<[f64; 4]> = None;
    let mut at = 0.0;
    for g in &graphs {
        layout.recall(&kept);
        let laid = layout.layout(g, &mut first_diagram);
        let (pieces, _) = compose(g, &laid, glyphs);
        if !g.nodes.is_empty() || g.underlay().is_some() {
            let b = &laid.bounds;
            area = Some(area.map_or([b.x0, b.y0, b.x1, b.y1], |a| [a[0].min(b.x0), a[1].min(b.y0), a[2].max(b.x1), a[3].max(b.y1)]));
        }
        let wanted: HashMap<&str, Style> = pieces.iter().filter_map(|p| p.lines.first().map(|l| (p.key.as_str(), l.style))).collect();
        // What was already written, by its seed, which stays the same when a piece only moves.
        let written: HashMap<String, Vec<usize>> = shown.values().map(|(seed, _, ids)| (seed.clone(), ids.clone())).collect();
        // Whatever moved, changed its look or went away leaves the board as this step begins.
        shown.retain(|key, (_, style, ids)| {
            let keep = wanted.get(key.as_str()) == Some(style);
            if !keep {
                for &i in ids.iter() {
                    strokes[i].until = Some(at);
                }
            }
            keep
        });
        let mut pen = at;
        let mut wrote = false;
        for piece in &pieces {
            let Some(style) = piece.lines.first().map(|l| l.style) else { continue };
            if shown.contains_key(&piece.key) {
                continue;
            }
            // Ink already written that only moved or took a new look (crossed-out words fading) shows in its
            // new place at once, as a highlight does; only what is new is written out.
            let before = written.get(&piece.seed);
            let at_once = style.under || before.is_some();
            let mut r = Rng::new(&piece.seed);
            let mut ids = vec![];
            for (n, line) in piece.lines.iter().enumerate() {
                let inked = line.inked(&mut r);
                // A stroke that kept its shape and only shifted is the earlier one, moved.
                let copy_of = before.and_then(|b| b.get(n)).and_then(|&i| {
                    let original = strokes[i].copy_of.map_or(i, |(o, _, _)| o);
                    let (a, b) = (&strokes[original].inked.center, &inked.center);
                    let (dx, dy) = (b.first()?[0] - a.first()?[0], b.first()?[1] - a.first()?[1]);
                    let same = a.len() == b.len() && a.iter().zip(b.iter()).all(|(p, q)| (q[0] - p[0] - dx).abs() < 0.01 && (q[1] - p[1] - dy).abs() < 0.01);
                    same.then_some((original, dx, dy))
                });
                let (delay, duration) = if at_once {
                    (at, 0.0)
                } else {
                    let d = stroke_time(f64::from(inked.length));
                    let start = pen;
                    pen += d + GAP;
                    wrote = true;
                    (start, d)
                };
                ids.push(strokes.len());
                strokes.push(Played { inked, style: line.style, delay, duration, until: None, copy_of });
            }
            shown.insert(piece.key.clone(), (piece.seed.clone(), style, ids));
        }
        at = if wrote { pen } else { at } + PAUSE;
    }
    // However long the meeting was, the board is written in `TOTAL` at most.
    let k = if at > TOTAL { TOTAL / at } else { 1.0 };
    for s in &mut strokes {
        s.delay = (s.delay * k).round();
        s.duration = (s.duration * k).round();
        s.until = s.until.map(|u| (u * k).round());
    }
    let a = area.unwrap_or([0.0, 0.0, 800.0, 500.0]);
    let image = page.image.clone().zip(page.graph.underlay()).map(|(image, (w, h))| Page { image, w, h });
    Replay { strokes, frame: (a[0] - PADDING, a[1] - PADDING, a[2] - a[0] + 2.0 * PADDING, a[3] - a[1] + 2.0 * PADDING), page: image, end: (at * k).round() }
}

fn path(points: &[[f32; 2]]) -> String {
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        let _ = write!(d, "{}{:.2} {:.2}", if i == 0 { 'M' } else { 'L' }, p[0], p[1]);
    }
    d
}

/// GIF frames a second, the longest side in pixels, and how long the finished board holds before it loops.
const GIF_FPS: f64 = 10.0;
const GIF_SIDE: f64 = 960.0;
const GIF_HOLD_MS: u16 = 2500;

/// The replay as a GIF: the same strokes at the same times as the animated SVG, written frame by frame,
/// then the whole board held a moment before it starts again.
pub fn gif(replay: &Replay) -> anyhow::Result<Vec<u8>> {
    use tiny_skia::{Color, FilterQuality, Pixmap, PixmapPaint, Transform};

    let (x, y, w, h) = replay.frame;
    let end = replay.end;
    let scale = (GIF_SIDE / w.max(h)).min(2.0);
    let (pw, ph) = ((w * scale).round().max(1.0) as u32, (h * scale).round().max(1.0) as u32);
    let at = Transform::from_translate(-x as f32, -y as f32).post_scale(scale as f32, scale as f32);
    // The paper and any page under the board, drawn once.
    let mut base = Pixmap::new(pw, ph).ok_or_else(|| anyhow::anyhow!("the GIF is too big to make"))?;
    let [_, r, g, bl] = PAPER.to_be_bytes();
    base.fill(Color::from_rgba8(r, g, bl, 255));
    if let Some(page) = &replay.page {
        let picture = crate::ink::page_pixmap(&page.image)?;
        let fit = Transform::from_scale((page.w / f64::from(picture.width())) as f32, (page.h / f64::from(picture.height())) as f32).post_concat(at);
        base.draw_pixmap(0, 0, picture.as_ref(), &PixmapPaint { quality: FilterQuality::Bilinear, ..PixmapPaint::default() }, fit, None);
    }

    let mut bytes = vec![];
    {
        let mut encoder = gif::Encoder::new(&mut bytes, pw as u16, ph as u16, &[])?;
        encoder.set_repeat(gif::Repeat::Infinite)?;
        // Each frame is only what changed, laid over the frames before.
        let write = |encoder: &mut gif::Encoder<&mut Vec<u8>>, mut rgba: Vec<u8>, (left, top, w, h): (u32, u32, u32, u32), ms: u32| -> anyhow::Result<()> {
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
                for s in replay.strokes.iter().filter(|s| s.style.under == under && s.until.is_none_or(|u| t < u)) {
                    let k = if s.duration <= 0.0 { if t >= s.delay { 1.0 } else { 0.0 } } else { ((t - s.delay) / s.duration).clamp(0.0, 1.0) };
                    if k > 0.0 {
                        crate::ink::fill_stroke(&mut canvas, &outline(&s.inked, k as f32), s.style, at);
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

/// The replay in CSS: each stroke uncovered along its centre line in its time, and hidden again when its
/// piece moves or goes away.
pub fn animated_svg(replay: &Replay, title: &str) -> String {
    let (x, y, w, h) = replay.frame;
    let mut out = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{x} {y} {w} {h}\" width=\"{}\" height=\"{}\">\n<title>{}</title>\n", w.round(), h.round(), crate::ink::xml_text(title));
    out.push_str(concat!(
        "<style>\n",
        "@keyframes w{from{stroke-dashoffset:var(--length)}to{stroke-dashoffset:0}}\n",
        "@keyframes pop{from{opacity:0}to{opacity:1}}\n",
        "@keyframes here{from,to{visibility:visible}}\n",
        ".w{fill:none;stroke:#fff;stroke-linecap:round;stroke-linejoin:round;animation-name:w;animation-timing-function:linear;animation-fill-mode:both}\n",
        ".pop{animation:pop 1ms both}\n",
        // Ink that later moves or goes away is hidden unless its time is playing, so a still picture is the finished board.
        ".gone{visibility:hidden;animation-name:here;animation-timing-function:step-end}\n",
        "@media (prefers-reduced-motion:reduce){.w{animation:none;stroke-dashoffset:0}.pop,.gone{animation:none}}\n",
        "</style>\n",
    ));
    let _ = writeln!(out, "<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"#{PAPER:06x}\"/>");
    if let Some(page) = &replay.page {
        let _ = writeln!(out, "<image href=\"{}\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\"/>", page.image.data_url(), page.w, page.h);
    }
    // Strokes that are later moved are kept once and shown again where they moved to.
    let copied: HashSet<usize> = replay.strokes.iter().filter_map(|s| s.copy_of.map(|(o, _, _)| o)).collect();
    // Plain shapes, apart from the written ones with their masks and times, so a copy shows whatever the original does.
    let mut kept: Vec<usize> = copied.iter().copied().collect();
    kept.sort_unstable();
    if !kept.is_empty() {
        out.push_str("<defs>");
        for i in kept {
            let _ = write!(out, "<path id=\"s{i}\" d=\"{}Z\"/>", path(&outline(&replay.strokes[i].inked, 1.0)));
        }
        out.push_str("</defs>\n");
    }
    let mut mask = 0;
    for under in [true, false] {
        for s in replay.strokes.iter().filter(|s| s.style.under == under) {
            let d = format!("{}Z", path(&outline(&s.inked, 1.0)));
            let blend = if under { ";mix-blend-mode:multiply" } else { "" };
            // A stroke that goes away again is hidden from then on; one that stays is left alone.
            let gone = s.until.map_or(String::new(), |u| format!(" class=\"gone\" style=\"animation-duration:{u}ms{blend}\""));
            let style = if gone.is_empty() && under { " style=\"mix-blend-mode:multiply\"".to_string() } else { gone };
            let _ = write!(out, "<g fill=\"#{:06x}\" opacity=\"{}\"{style}>", s.style.rgb, s.style.alpha);
            if let Some((original, dx, dy)) = s.copy_of {
                let pop = if s.delay > 0.0 { format!(" class=\"pop\" style=\"animation-delay:{}ms\"", s.delay) } else { String::new() };
                let _ = write!(out, "<use href=\"#s{original}\" transform=\"translate({dx:.2} {dy:.2})\"{pop}/>");
            } else if s.duration == 0.0 {
                if s.delay > 0.0 {
                    let _ = write!(out, "<path d=\"{d}\" class=\"pop\" style=\"animation-delay:{}ms\"/>", s.delay);
                } else {
                    let _ = write!(out, "<path d=\"{d}\"/>");
                }
            } else {
                let length = s.inked.length + 1.0;
                // Wide enough to uncover the whole stroke, or a wide one would stay clipped for good.
                let reveal = MASK_WIDTH.max(s.inked.width * 3.0);
                let _ = write!(
                    out,
                    "<mask id=\"m{mask}\" maskUnits=\"userSpaceOnUse\" x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\"><path class=\"w\" d=\"{}\" stroke-width=\"{reveal}\" stroke-dasharray=\"{length:.1}\" stroke-dashoffset=\"0\" style=\"--length:{length:.1};animation-duration:{}ms;animation-delay:{}ms\"/></mask><path d=\"{d}\" mask=\"url(#m{mask})\"/>",
                    path(&s.inked.center),
                    s.duration,
                    s.delay,
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
    use serde_json::Value;

    /// A board built over four steps, the last putting a row under the first item, above the second.
    fn page_with_an_insert() -> replay_page::Page {
        let mut g = Graph::default();
        let mut steps = vec![];
        for (i, ops) in ["T 「Release checklist」", "L n1 「Load testing」", "L n1 「Release notes」", "L n2 「Before launch」\nM n2 星标"].iter().enumerate() {
            let before = g.clone();
            g.apply(&boardcast_board::graph::parse_ops(ops));
            let d = boardcast_board::steps::diff(&before, &g).unwrap();
            steps.push(serde_json::json!({ "d": d, "t": i as f64 * 1000.0 }));
        }
        replay_page::Page { graph: g, memory: Value::Null, steps, image: None }
    }

    #[test]
    fn the_replay_lays_out_each_step_and_moves_what_an_insert_pushes_down() {
        let mut glyphs = Glyphs::from_checkout(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))).unwrap();
        let page = page_with_an_insert();
        let replay = replay(&page, &mut glyphs);
        // "Release notes" sat right under "Load testing" until a row went in between: it moves down at once,
        // without being written again.
        assert!(replay.strokes.iter().any(|s| s.until.is_some()), "nothing moved");
        let moved: Vec<&Played> = replay.strokes.iter().filter(|s| s.until.is_none() && s.duration == 0.0 && !s.style.under).collect();
        assert!(!moved.is_empty(), "what moved was written again");
        // Everything still on the board at the end is written by then.
        assert!(replay.strokes.iter().filter(|s| s.until.is_none()).all(|s| s.delay + s.duration <= replay.end));
        assert!(replay.end <= TOTAL);
        assert!(animated_svg(&replay, "t").contains("class=\"gone\""));
    }

    #[test]
    fn the_gif_writes_the_board_out_and_holds_it() {
        use image::AnimationDecoder;
        let mut glyphs = Glyphs::from_checkout(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))).unwrap();
        let page = page_with_an_insert();
        let bytes = gif(&replay(&page, &mut glyphs)).unwrap();
        let frames: Vec<image::Frame> = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes)).unwrap().into_frames().collect_frames().unwrap();
        assert!(frames.len() > 5, "{} frames", frames.len());
        let ink = |f: &image::Frame| f.buffer().pixels().filter(|p| p.0[0] < 128).count();
        assert!(ink(&frames[0]) < ink(frames.last().unwrap()), "the first frame has less ink than the last");
        let (hold, _) = frames.last().unwrap().delay().numer_denom_ms();
        assert_eq!(hold, u32::from(GIF_HOLD_MS));
    }
}
