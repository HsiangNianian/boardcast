//! The animated board, as the app replays it: each step of the meeting laid out as it stood then, new
//! ink written stroke by stroke, and what moved or went away redrawn where it now is. The GIF draws it
//! frame by frame; the SVG does it in CSS, so a viewer without animation shows the finished board.

use crate::ink::{Page, Picture};
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

/// A clip's picture in a replay: shown from when its card's frame is begun until the frame moves or goes.
pub struct Shown {
    pub picture: Picture,
    pub from: f64,
    pub until: Option<f64>,
}

/// A whole replay of one board: its strokes, the area it ever covers, the page under it, and how long it runs.
pub struct Replay {
    pub strokes: Vec<Played>,
    pub frame: (f64, f64, f64, f64),
    pub page: Option<Page>,
    pub pictures: Vec<Shown>,
    pub end: f64,
}

impl Replay {
    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty() && self.page.is_none()
    }
}

/// Lay the board out step by step, as the app's replay does, and time each new piece of ink.
pub fn replay(page: &replay_page::Page, glyphs: &mut Glyphs) -> Replay {
    timed(page, glyphs, None)
}

/// As `replay`, but in real time: each step's ink begins no sooner than `starts` says, in ms, one for each
/// step and one more for the board as kept, and nothing is sped up.
pub fn timed(page: &replay_page::Page, glyphs: &mut Glyphs, starts: Option<&[f64]>) -> Replay {
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
    let mut cuts = HashMap::new();
    let mut pictures: Vec<Shown> = vec![];
    // Which picture each clip frame on the board shows, by the frame's key.
    let mut framed: HashMap<String, usize> = HashMap::new();
    for (i, g) in graphs.iter().enumerate() {
        if let Some(start) = starts.and_then(|s| s.get(i)) {
            at = f64::max(at, *start);
        }
        layout.recall(&kept);
        let laid = layout.layout(g, &mut first_diagram);
        let (pieces, _) = compose(g, &laid, glyphs);
        let mut cut: HashMap<String, Picture> = crate::ink::pictures(g, &laid, &page.sources, &mut cuts).into_iter().map(|(id, p)| (format!("{id}|box"), p)).collect();
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
                if let Some(p) = framed.remove(key.as_str()) {
                    pictures[p].until = Some(at);
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
            if let (Some(picture), Some(&first)) = (cut.remove(&piece.seed), ids.first()) {
                framed.insert(piece.key.clone(), pictures.len());
                pictures.push(Shown { picture, from: strokes[first].delay, until: None });
            }
            shown.insert(piece.key.clone(), (piece.seed.clone(), style, ids));
        }
        at = if wrote { pen } else { at } + PAUSE;
    }
    // However long the meeting was, the board is written in `TOTAL` at most.
    let k = if at > TOTAL && starts.is_none() { TOTAL / at } else { 1.0 };
    for s in &mut strokes {
        s.delay = (s.delay * k).round();
        s.duration = (s.duration * k).round();
        s.until = s.until.map(|u| (u * k).round());
    }
    for p in &mut pictures {
        p.from = (p.from * k).round();
        p.until = p.until.map(|u| (u * k).round());
    }
    let a = area.unwrap_or([0.0, 0.0, 800.0, 500.0]);
    let image = page.image.clone().zip(page.graph.underlay()).map(|(image, (w, h))| Page { image, w, h });
    Replay { strokes, frame: (a[0] - PADDING, a[1] - PADDING, a[2] - a[0] + 2.0 * PADDING, a[3] - a[1] + 2.0 * PADDING), page: image, pictures, end: (at * k).round() }
}

fn path(points: &[[f32; 2]]) -> String {
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        let _ = write!(d, "{}{:.2} {:.2}", if i == 0 { 'M' } else { 'L' }, p[0], p[1]);
    }
    d
}

/// GIF and video frames a second, the longest side in pixels, and how long the finished board holds before
/// it loops or ends.
const GIF_FPS: f64 = 10.0;
const GIF_SIDE: f64 = 960.0;
const GIF_HOLD_MS: u16 = 2500;

/// The replay drawn as pixels at any moment: the paper and any page under the board, drawn once, and the
/// strokes as far as the pen has got.
struct Frames<'a> {
    replay: &'a Replay,
    base: tiny_skia::Pixmap,
    at: tiny_skia::Transform,
    width: u32,
    height: u32,
    /// Each clip's picture, read once.
    pictures: Vec<Option<tiny_skia::Pixmap>>,
    /// The page under the board, read once.
    page: Option<tiny_skia::Pixmap>,
    /// Each stroke's extent: left, top, right, bottom.
    bounds: Vec<[f64; 4]>,
    /// Each stroke's whole outline, worked out once, as most frames show most strokes finished.
    whole: Vec<Vec<boardcast_board::glyphs::Pt>>,
}

impl<'a> Frames<'a> {
    /// At most `side` pixels on the longer side; `even` rounds both sides to even numbers, as a video's
    /// halved colour planes need.
    fn new(replay: &'a Replay, side: f64, even: bool) -> anyhow::Result<Self> {
        let (_, _, w, h) = replay.frame;
        let scale = (side / w.max(h)).min(2.0);
        let round = |v: f64| if even { ((v / 2.0).round() as u32 * 2).max(2) } else { v.round().max(1.0) as u32 };
        Self::fitted(replay, round(w * scale), round(h * scale))
    }

    /// Exactly `width` by `height`, the board as large as fits, in the middle.
    fn fitted(replay: &'a Replay, width: u32, height: u32) -> anyhow::Result<Self> {
        use tiny_skia::{Color, FilterQuality, Pixmap, PixmapPaint, Transform};
        let (x, y, w, h) = replay.frame;
        let scale = (f64::from(width) / w).min(f64::from(height) / h).min(2.0);
        let (dx, dy) = ((f64::from(width) - w * scale) / 2.0, (f64::from(height) - h * scale) / 2.0);
        let at = Transform::from_translate(-x as f32, -y as f32).post_scale(scale as f32, scale as f32).post_translate(dx as f32, dy as f32);
        let mut base = Pixmap::new(width, height).ok_or_else(|| anyhow::anyhow!("the animation is too big to make"))?;
        let [_, r, g, bl] = PAPER.to_be_bytes();
        base.fill(Color::from_rgba8(r, g, bl, 255));
        let mut page = None;
        if let Some(under) = &replay.page {
            let picture = crate::ink::page_pixmap(&under.image)?;
            let fit = Transform::from_scale((under.w / f64::from(picture.width())) as f32, (under.h / f64::from(picture.height())) as f32).post_concat(at);
            base.draw_pixmap(0, 0, picture.as_ref(), &PixmapPaint { quality: FilterQuality::Bilinear, ..PixmapPaint::default() }, fit, None);
            page = Some(picture);
        }
        let pictures = replay.pictures.iter().map(|p| crate::ink::page_pixmap(&p.picture.image).ok()).collect();
        let bounds = replay
            .strokes
            .iter()
            .map(|s| {
                let pad = f64::from(s.inked.width);
                s.inked.center.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, p| [b[0].min(f64::from(p[0]) - pad), b[1].min(f64::from(p[1]) - pad), b[2].max(f64::from(p[0]) + pad), b[3].max(f64::from(p[1]) + pad)])
            })
            .collect();
        let whole = replay.strokes.iter().map(|s| outline(&s.inked, 1.0)).collect();
        Ok(Self { replay, base, at, width, height, pictures, page, bounds, whole })
    }

    /// The board at `t` milliseconds, as RGBA.
    fn at(&self, t: f64) -> Vec<u8> {
        let mut canvas = self.base.clone();
        self.ink(&mut canvas, t, self.at);
        canvas.take()
    }

    /// The board at `t` with `view`, an area of the board, filling the picture, as RGBA.
    fn seen(&self, t: f64, view: View) -> anyhow::Result<Vec<u8>> {
        use tiny_skia::{Color, FilterQuality, Pixmap, PixmapPaint, Transform};
        let scale = (f64::from(self.width) / view.2).min(f64::from(self.height) / view.3);
        let (dx, dy) = ((f64::from(self.width) - view.2 * scale) / 2.0, (f64::from(self.height) - view.3 * scale) / 2.0);
        let at = Transform::from_translate(-view.0 as f32, -view.1 as f32).post_scale(scale as f32, scale as f32).post_translate(dx as f32, dy as f32);
        let mut canvas = Pixmap::new(self.width, self.height).ok_or_else(|| anyhow::anyhow!("the video is too big to make"))?;
        let [_, r, g, b] = PAPER.to_be_bytes();
        canvas.fill(Color::from_rgba8(r, g, b, 255));
        if let (Some(page), Some(picture)) = (&self.replay.page, &self.page) {
            let fit = Transform::from_scale((page.w / f64::from(picture.width())) as f32, (page.h / f64::from(picture.height())) as f32).post_concat(at);
            canvas.draw_pixmap(0, 0, picture.as_ref(), &PixmapPaint { quality: FilterQuality::Bilinear, ..PixmapPaint::default() }, fit, None);
        }
        self.ink(&mut canvas, t, at);
        Ok(canvas.take())
    }

    /// The pictures and strokes shown at `t`, as far as the pen has got.
    fn ink(&self, canvas: &mut tiny_skia::Pixmap, t: f64, at: tiny_skia::Transform) {
        for (shown, pixels) in self.replay.pictures.iter().zip(&self.pictures) {
            let Some(pixels) = pixels.as_ref().filter(|_| t >= shown.from && shown.until.is_none_or(|u| t < u)) else { continue };
            let p = &shown.picture;
            let fit = tiny_skia::Transform::from_scale((p.w / f64::from(pixels.width())) as f32, (p.h / f64::from(pixels.height())) as f32).post_translate(p.x as f32, p.y as f32).post_concat(at);
            canvas.draw_pixmap(0, 0, pixels.as_ref(), &tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..tiny_skia::PixmapPaint::default() }, fit, None);
        }
        for under in [true, false] {
            for (s, whole) in self.replay.strokes.iter().zip(&self.whole).filter(|(s, _)| s.style.under == under && s.until.is_none_or(|u| t < u)) {
                let k = if s.duration <= 0.0 { if t >= s.delay { 1.0 } else { 0.0 } } else { ((t - s.delay) / s.duration).clamp(0.0, 1.0) };
                if k >= 1.0 {
                    crate::ink::fill_stroke(canvas, whole, s.style, at);
                } else if k > 0.0 {
                    crate::ink::fill_stroke(canvas, &outline(&s.inked, k as f32), s.style, at);
                }
            }
        }
    }

    /// The area of the board that has anything on it at `t`, if anything has.
    fn filled(&self, t: f64) -> Option<View> {
        let mut area: Option<[f64; 4]> = None;
        let mut add = |x0: f64, y0: f64, x1: f64, y1: f64| {
            area = Some(area.map_or([x0, y0, x1, y1], |a| [a[0].min(x0), a[1].min(y0), a[2].max(x1), a[3].max(y1)]));
        };
        if let Some(page) = &self.replay.page {
            add(0.0, 0.0, page.w, page.h);
        }
        for (s, b) in self.replay.strokes.iter().zip(&self.bounds) {
            if t >= s.delay && s.until.is_none_or(|u| t < u) {
                add(b[0], b[1], b[2], b[3]);
            }
        }
        for p in self.replay.pictures.iter().filter(|p| t >= p.from && p.until.is_none_or(|u| t < u)) {
            add(p.picture.x, p.picture.y, p.picture.x + p.picture.w, p.picture.y + p.picture.h);
        }
        area.map(|a| (a[0], a[1], a[2] - a[0], a[3] - a[1]))
    }
}

/// An area of a board: left, top, width, height.
type View = (f64, f64, f64, f64);

/// What the real-time video looks at: `filled` with a margin, never so small that the writing is huge, in
/// the picture's proportions.
fn framing(filled: View, (width, height): (u32, u32)) -> View {
    let aspect = f64::from(width) / f64::from(height);
    let (x, y, w, h) = (filled.0 - PADDING, filled.1 - PADDING, filled.2 + 2.0 * PADDING, filled.3 + 2.0 * PADDING);
    let w = w.max(h * aspect).max(LIVE_MIN_WIDTH);
    let h = w / aspect;
    let (cx, cy) = (x + (filled.2 + 2.0 * PADDING) / 2.0, y + (filled.3 + 2.0 * PADDING) / 2.0);
    (cx - w / 2.0, cy - h / 2.0, w, h)
}

/// The replay as a GIF: the same strokes at the same times as the animated SVG, written frame by frame,
/// then the whole board held a moment before it starts again.
pub fn gif(replay: &Replay) -> anyhow::Result<Vec<u8>> {
    let end = replay.end;
    let frames_of = Frames::new(replay, GIF_SIDE, false)?;
    let (pw, ph) = (frames_of.width, frames_of.height);

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
            let pixels = frames_of.at(f as f64 * step);
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

/// The replay as a WebM video (AV1): the same frames as the GIF, ten a second, then the finished board held
/// as long. A board with no motion between frames costs almost nothing, so the hold is plain repeated frames.
pub fn webm(replay: &Replay) -> anyhow::Result<Vec<u8>> {
    use rav1e::prelude::*;
    let frames_of = Frames::new(replay, GIF_SIDE, true)?;
    let (w, h) = (frames_of.width as usize, frames_of.height as usize);
    let config = EncoderConfig {
        width: w,
        height: h,
        time_base: Rational::new(1, GIF_FPS as u64),
        bit_depth: 8,
        chroma_sampling: ChromaSampling::Cs420,
        pixel_range: PixelRange::Limited,
        color_description: Some(ColorDescription { color_primaries: ColorPrimaries::BT709, transfer_characteristics: TransferCharacteristics::BT709, matrix_coefficients: MatrixCoefficients::BT709 }),
        // Handwriting is thin lines on flat paper: a lower quantizer than the default keeps its edges.
        quantizer: 70,
        speed_settings: SpeedSettings::from_preset(10),
        max_key_frame_interval: 600,
        low_latency: true,
        ..Default::default()
    };
    let mut encoder: Context<u8> = Config::new().with_encoder_config(config).with_threads(0).new_context().map_err(|e| anyhow::anyhow!("the video encoder would not start: {e:?}"))?;
    let step = 1000.0 / GIF_FPS;
    let count = (replay.end / step).ceil() as usize + 1 + (f64::from(GIF_HOLD_MS) / step).round() as usize;
    let mut out: Vec<crate::webm::Frame> = vec![];
    let take = |encoder: &mut Context<u8>, out: &mut Vec<crate::webm::Frame>| -> anyhow::Result<bool> {
        loop {
            match encoder.receive_packet() {
                Ok(p) => out.push(crate::webm::Frame { ms: (p.input_frameno as f64 * step) as u64, key: p.frame_type == FrameType::KEY, data: p.data }),
                Err(EncoderStatus::Encoded) => {}
                Err(EncoderStatus::NeedMoreData) => return Ok(false),
                Err(EncoderStatus::LimitReached) => return Ok(true),
                Err(e) => anyhow::bail!("the video encoder failed: {e:?}"),
            }
        }
    };
    // The finished board, drawn once when the replay reaches its end and shown for every frame after.
    let mut finished: Option<Vec<u8>> = None;
    for f in 0..count {
        let t = f as f64 * step;
        let rgba = if t >= replay.end {
            finished.get_or_insert_with(|| frames_of.at(replay.end)).clone()
        } else {
            frames_of.at(t)
        };
        let mut frame = encoder.new_frame();
        let (y, u, v) = yuv420(&rgba, w, h);
        frame.planes[0].copy_from_raw_u8(&y, w, 1);
        frame.planes[1].copy_from_raw_u8(&u, w / 2, 1);
        frame.planes[2].copy_from_raw_u8(&v, w / 2, 1);
        encoder.send_frame(frame).map_err(|e| anyhow::anyhow!("the video encoder failed: {e:?}"))?;
        take(&mut encoder, &mut out)?;
    }
    encoder.flush();
    while !take(&mut encoder, &mut out)? {}
    let duration = count as f64 * step;
    let video = crate::webm::Video { width: w as u32, height: h as u32, fps: Some(GIF_FPS as u32), av1c: &encoder.container_sequence_header(), frames: &out };
    Ok(crate::webm::write(Some(video), None, duration))
}

/// The real-time video's size: 720p, which keeps the writing legible on a phone and a projector.
const LIVE_SIZE: (u32, u32) = (1280, 720);
/// However still the board is, a frame at least this often, so players show the time going by.
const LIVE_EVERY_MS: f64 = 2000.0;
/// The real-time video never looks at less of the board than this wide, so a first word is not huge.
const LIVE_MIN_WIDTH: f64 = 900.0;
/// How far the view moves towards what it should show in each tenth of a second.
const LIVE_EASE: f64 = 0.15;

/// The times, in ms, at which a replay's picture changes, as merged spans from first to last.
fn busy(replay: &Replay) -> Vec<(f64, f64)> {
    let mut spans: Vec<(f64, f64)> = vec![];
    for s in &replay.strokes {
        spans.push((s.delay, s.delay + s.duration));
        if let Some(u) = s.until {
            spans.push((u, u));
        }
    }
    for p in &replay.pictures {
        spans.push((p.from, p.from));
        if let Some(u) = p.until {
            spans.push((u, u));
        }
    }
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = vec![];
    for (a, b) in spans {
        match merged.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => merged.push((a, b)),
        }
    }
    merged
}

/// Whether anything changes after `from` up to and including `to`.
fn changes(busy: &[(f64, f64)], from: f64, to: f64) -> bool {
    // The first span that has not ended by `from`.
    let i = busy.partition_point(|(_, end)| *end <= from);
    busy.get(i).is_some_and(|(start, _)| *start <= to)
}

/// The whole meeting as a video with its sound, in real time: `boards` replayed on the sound's clock, the
/// board in view at each moment from `shown` (when the meeting turned to each board, in ms, in order), and
/// the sound beside them. A frame is only stored when the picture changes, or every two seconds.
pub fn live_webm(boards: &[Replay], shown: &[(f64, usize)], sound: &crate::Sound) -> anyhow::Result<Vec<u8>> {
    let drawn: Vec<Frames> = boards.iter().map(|r| Frames::fitted(r, LIVE_SIZE.0, LIVE_SIZE.1)).collect::<anyhow::Result<_>>()?;
    let spans: Vec<Vec<(f64, f64)>> = boards.iter().map(busy).collect();
    let written = boards.iter().flat_map(|r| r.strokes.iter().map(|s| s.delay + s.duration)).fold(0.0, f64::max);
    let end = (sound.packets.len() as f64 * sound.packet_ms as f64).max(written + PAUSE);
    // First which frames to store and what each looks at, which is quick; then the drawing and encoding.
    let step = 1000.0 / GIF_FPS;
    let board_at = |t: f64| shown.iter().take_while(|(at, _)| *at <= t).last().or(shown.first()).map_or(0, |(_, b)| *b).min(boards.len().saturating_sub(1));
    let mut plan: Vec<(f64, usize, View)> = vec![];
    let mut view: Option<View> = None;
    let mut f = 0;
    loop {
        let t = f as f64 * step;
        if t > end {
            break;
        }
        f += 1;
        let board = board_at(t);
        let turned = plan.last().is_none_or(|(_, b, _)| *b != board);
        let want = framing(drawn[board].filled(t).unwrap_or(boards[board].frame), LIVE_SIZE);
        let moved = match view {
            // A new board is looked at straight away; on the same one the view glides.
            Some(v) if !turned => {
                let next = (v.0 + (want.0 - v.0) * LIVE_EASE, v.1 + (want.1 - v.1) * LIVE_EASE, v.2 + (want.2 - v.2) * LIVE_EASE, v.3 + (want.3 - v.3) * LIVE_EASE);
                // Less than half a pixel's difference is no move.
                let pixel = v.2 / f64::from(LIVE_SIZE.0);
                let far = [next.0 - v.0, next.1 - v.1, next.2 - v.2, next.3 - v.3].iter().any(|d| d.abs() > pixel * 0.5);
                if far {
                    view = Some(next);
                }
                far
            }
            _ => {
                view = Some(want);
                true
            }
        };
        let changed = moved || changes(&spans[board], t - step, t);
        if turned || changed || plan.last().is_some_and(|(at, _, _)| t - at >= LIVE_EVERY_MS) {
            plan.push((t, board, view.expect("set above")));
        }
    }
    // Pieces encoded side by side, each from a key frame, then joined: one encoder keeps only a few cores busy.
    let pieces = std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(1, 12);
    let size = plan.len().div_ceil(pieces).max(LIVE_PIECE_MIN);
    let encoded: Vec<anyhow::Result<(Vec<crate::webm::Frame>, Vec<u8>)>> = std::thread::scope(|scope| {
        let drawn = &drawn;
        let jobs: Vec<_> = plan.chunks(size).map(|part| scope.spawn(move || encode_live(drawn, part))).collect();
        jobs.into_iter().map(|j| j.join().unwrap_or_else(|_| Err(anyhow::anyhow!("the video encoder stopped")))).collect()
    });
    let mut frames = vec![];
    let mut header = vec![];
    for piece in encoded {
        let (piece, h) = piece?;
        if header.is_empty() {
            header = h;
        }
        frames.extend(piece);
    }
    let (w, h) = LIVE_SIZE;
    let video = crate::webm::Video { width: w, height: h, fps: None, av1c: &header, frames: &frames };
    let audio = crate::webm::Audio { head: &sound.head, pre_skip_ns: sound.pre_skip_ns(), packet_ms: sound.packet_ms, packets: &sound.packets };
    Ok(crate::webm::write(Some(video), Some(audio), end))
}

/// Encoding a piece costs a key frame, so a piece is never shorter than this many frames.
const LIVE_PIECE_MIN: usize = 120;

/// Draw and encode some of the real-time video's frames, each `(when, which board, what it looks at)`;
/// the first is a key frame, so pieces join. Gives the frames and the encoder's codec record.
fn encode_live(drawn: &[Frames], part: &[(f64, usize, View)]) -> anyhow::Result<(Vec<crate::webm::Frame>, Vec<u8>)> {
    use rav1e::prelude::*;
    let (w, h) = (LIVE_SIZE.0 as usize, LIVE_SIZE.1 as usize);
    let config = EncoderConfig {
        width: w,
        height: h,
        time_base: Rational::new(1, GIF_FPS as u64),
        bit_depth: 8,
        chroma_sampling: ChromaSampling::Cs420,
        pixel_range: PixelRange::Limited,
        color_description: Some(ColorDescription { color_primaries: ColorPrimaries::BT709, transfer_characteristics: TransferCharacteristics::BT709, matrix_coefficients: MatrixCoefficients::BT709 }),
        quantizer: 70,
        speed_settings: SpeedSettings::from_preset(10),
        // In stored frames: a place to start from at least every minute.
        max_key_frame_interval: 30,
        low_latency: true,
        tiles: 4,
        ..Default::default()
    };
    let mut encoder: Context<u8> = Config::new().with_encoder_config(config).with_threads(2).new_context().map_err(|e| anyhow::anyhow!("the video encoder would not start: {e:?}"))?;
    let mut out: Vec<crate::webm::Frame> = vec![];
    let take = |encoder: &mut Context<u8>, out: &mut Vec<crate::webm::Frame>| -> anyhow::Result<bool> {
        loop {
            match encoder.receive_packet() {
                Ok(p) => out.push(crate::webm::Frame { ms: part[p.input_frameno as usize].0 as u64, key: p.frame_type == FrameType::KEY, data: p.data }),
                Err(EncoderStatus::Encoded) => {}
                Err(EncoderStatus::NeedMoreData) => return Ok(false),
                Err(EncoderStatus::LimitReached) => return Ok(true),
                Err(e) => anyhow::bail!("the video encoder failed: {e:?}"),
            }
        }
    };
    for &(t, board, view) in part {
        let rgba = drawn[board].seen(t, view)?;
        let mut frame = encoder.new_frame();
        let (y, u, v) = yuv420(&rgba, w, h);
        frame.planes[0].copy_from_raw_u8(&y, w, 1);
        frame.planes[1].copy_from_raw_u8(&u, w / 2, 1);
        frame.planes[2].copy_from_raw_u8(&v, w / 2, 1);
        encoder.send_frame(frame).map_err(|e| anyhow::anyhow!("the video encoder failed: {e:?}"))?;
        take(&mut encoder, &mut out)?;
    }
    encoder.flush();
    while !take(&mut encoder, &mut out)? {}
    Ok((out, encoder.container_sequence_header()))
}

/// RGBA as BT.709 limited-range Y, Cb and Cr, the colour planes halved both ways (each from four pixels).
fn yuv420(rgba: &[u8], w: usize, h: usize) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let px = |x: usize, y: usize| {
        let i = (y * w + x) * 4;
        (f64::from(rgba[i]), f64::from(rgba[i + 1]), f64::from(rgba[i + 2]))
    };
    let mut luma = vec![0_u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let (r, g, b) = px(x, y);
            luma[y * w + x] = (16.0 + 219.0 * (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0).round() as u8;
        }
    }
    let (cw, ch) = (w / 2, h / 2);
    let (mut cb, mut cr) = (vec![0_u8; cw * ch], vec![0_u8; cw * ch]);
    for y in 0..ch {
        for x in 0..cw {
            let mut sum = (0.0, 0.0, 0.0);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (r, g, b) = px(2 * x + dx, 2 * y + dy);
                sum = (sum.0 + r / 4.0, sum.1 + g / 4.0, sum.2 + b / 4.0);
            }
            let (r, g, b) = sum;
            cb[y * cw + x] = (128.0 + 224.0 * (-0.1146 * r - 0.3854 * g + 0.5 * b) / 255.0).round().clamp(16.0, 240.0) as u8;
            cr[y * cw + x] = (128.0 + 224.0 * (0.5 * r - 0.4542 * g - 0.0458 * b) / 255.0).round().clamp(16.0, 240.0) as u8;
        }
    }
    (luma, cb, cr)
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
    // Clip pictures appear with their frames and go when they do, as strokes do.
    for shown in &replay.pictures {
        let p = &shown.picture;
        let gone = shown.until.map_or(String::new(), |u| format!(" class=\"gone\" style=\"animation-duration:{u}ms\""));
        let pop = if shown.from > 0.0 { format!(" class=\"pop\" style=\"animation-delay:{}ms\"", shown.from) } else { String::new() };
        let _ = writeln!(out, "<g{gone}><image href=\"{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\"{pop}/></g>", p.image.data_url(), p.x, p.y, p.w, p.h);
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
        replay_page::Page { graph: g, memory: Value::Null, steps, image: None, sources: Default::default() }
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

    #[test]
    fn the_video_has_a_frame_for_every_tenth_of_a_second_and_the_hold() {
        let mut glyphs = Glyphs::from_checkout(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))).unwrap();
        let replay = replay(&page_with_an_insert(), &mut glyphs);
        let bytes = webm(&replay).unwrap();
        let find = |needle: &[u8]| bytes.windows(needle.len()).filter(|w| *w == needle).count();
        assert!(bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) && find(b"webm") == 1 && find(b"V_AV1") == 1);
        // Each frame is a SimpleBlock of track 1; the first starts the video and is a key frame.
        let step = 1000.0 / GIF_FPS;
        let expected = (replay.end / step).ceil() as usize + 1 + (f64::from(GIF_HOLD_MS) / step).round() as usize;
        let blocks = bytes.windows(2).enumerate().filter(|(_, w)| w[0] == 0xA3).filter(|(i, _)| {
            // An element ID, its size, then track 1 (0x81): a block, not a byte that happens to be 0xA3.
            let len_bytes = bytes[i + 1].leading_zeros() as usize + 1;
            bytes.get(i + 1 + len_bytes) == Some(&0x81)
        });
        assert!(blocks.count() >= expected, "a block for each of the {expected} frames");
        // The codec's private data (ID 63 A2, then a one-byte size): the av1C record, marker and version 1, then
        // the sequence header OBU (type 1, so a header byte of 0x0A) that the mapping asks for.
        let at = bytes.windows(2).position(|w| w == [0x63, 0xA2]).expect("CodecPrivate");
        let len = usize::from(bytes[at + 2] & 0x7f);
        let private = &bytes[at + 3..at + 3 + len];
        assert_eq!(private[0], 0x81, "av1C starts with its marker and version");
        assert_eq!(private[4], 0x0A, "the sequence header OBU follows the four-byte record");
    }
}
