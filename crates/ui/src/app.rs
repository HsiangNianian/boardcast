//! The window: the board, the transcript, and an input that accepts Chinese through the IME.

use boardcast_board::glyphs::{Glyphs, Pt};
use boardcast_board::board_layout::{target_at, Bounds as BoardBounds, Rect, StableLayout};
use boardcast_board::check::check;
use boardcast_board::compose::{compose, Style, PAPER};
use boardcast_board::graph::{parse_ops, Graph, Utterance};
use boardcast_board::sugiyama::first_diagram;
use boardcast_agent::{Backend, Guidance, LocalServer, Provider, Said, Update};
use boardcast_board::pen::{ink, outline, Inked, Rng};
use crate::pipeline::{Event, Pipeline, Script};
use crate::raster::Layer;
use anyhow::{Context as _, Result};
use gpui_kit::base::input::{Input, InputEvent, InputState};
use gpui_kit::{
    canvas, div, point, prelude::*, px, rgb, size, App, Background, Bounds, Context, Entity, FillOptions,
    Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, PathStyle, Pixels, Point, Render, RenderImage,
    ScrollEvent, Window, WindowBounds, WindowOptions,
};
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

const INK_RGB: u32 = 0x1e_1e_1e;
const MARKS: [&str; 5] = ["划掉", "圈出", "下划线", "星标", "高亮"];
const TOOLBAR_W: f32 = 290.0;
const TOOLBAR_H: f32 = 36.0;
/// A press that moves less than this is a click, not a pan.
const CLICK_SLOP: f32 = 4.0;
const MIN_ZOOM: f32 = 0.2;
const MAX_ZOOM: f32 = 4.0;
/// A new sharpness level is rendered once zooming has paused this long.
const RERASTER_AFTER: Duration = Duration::from_millis(120);
/// Writing speed. A backlog speeds it up so the board never falls far behind the talk.
const PEN_PX_PER_SECOND: f32 = 1500.0;

struct Queued {
    inked: Inked,
    style: Style,
    key: String,
    /// The batch this stroke belongs to, for end-to-end timing.
    batch: u64,
}

struct Stroke {
    key: String,
    shape: Vec<Pt>,
    style: Style,
    area: Area,
}

/// A rectangle on the board: left, top, right, bottom.
type Area = [f32; 4];

fn area_of(shape: &[Pt]) -> Area {
    shape.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |a, p| [a[0].min(p[0]), a[1].min(p[1]), a[2].max(p[0]), a[3].max(p[1])])
}

fn meets(a: Area, b: Area) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

fn inside(a: Area, b: Area) -> bool {
    a[0] >= b[0] && a[1] >= b[1] && a[2] <= b[2] && a[3] <= b[3]
}

/// Highlights under the ink, and the ink, at one sharpness.
struct Tier {
    under: Layer,
    layer: Layer,
    level: f32,
}

impl Tier {
    fn new(level: f32) -> Self {
        Self { under: Layer::with_scale(level), layer: Layer::with_scale(level), level }
    }

    fn fill(&mut self, stroke: &Stroke) {
        let [_, r, g, b] = stroke.style.rgb.to_be_bytes();
        let target = if stroke.style.under { &mut self.under } else { &mut self.layer };
        target.fill(&stroke.shape, [r, g, b], (stroke.style.alpha * 255.0) as u8);
    }

    fn release(self) -> Vec<Arc<RenderImage>> {
        let mut images = self.under.release();
        images.extend(self.layer.release());
        images
    }

    fn uploads(&self) -> usize {
        self.under.uploads + self.layer.uploads
    }
}

/// Board coordinates at the canvas origin, and screen pixels per board unit.
#[derive(Clone, Copy, PartialEq, Debug)]
struct View {
    x: f32,
    y: f32,
    k: f32,
}

struct Rebuilt {
    tier: Tier,
    /// The part of the board a detail tier covers; None for the whole-board base.
    cover: Option<Area>,
    /// How many finished strokes this rendering covers; later ones are added on arrival.
    covers: usize,
    generation: u64,
    ms: u128,
}

/// Everything the canvas needs while painting. Finished strokes live in textures.
struct Ink {
    glyphs: Glyphs,
    board: StableLayout,
    /// The whole board at the sharpness of the fit view. Cheap, and always ready when panning.
    base: Tier,
    /// Just around the viewport at the current zoom. Memory follows the window, not the board.
    detail: Option<(Tier, Area)>,
    retired: Vec<Arc<RenderImage>>,
    queue: VecDeque<Queued>,
    live: f32,
    finished: Vec<Arc<Stroke>>,
    drawn: HashSet<String>,
    last: Option<Instant>,
    timings: Vec<Timing>,
    graph: Option<Graph>,
    next_tidy_batch: u64,
    /// What a click can land on, in board coordinates.
    targets: Vec<Rect>,
    /// The thing whose mark toolbar is open.
    selected: Option<String>,
    /// What the layout check wants the model to hear next time.
    hints: Option<String>,
    /// The content's extent, which the automatic view fits.
    extent: BoardBounds,
    /// None follows the whole board. Some keeps a view the content still fits in, or one the user chose.
    view: Option<View>,
    /// The user zoomed or panned: new content must not move the view until they return to latest.
    reading: bool,
    canvas: Bounds<Pixels>,
    dpr: f32,
    zoomed_at: Instant,
    /// Bumped when finished strokes are removed, which invalidates a rendering in progress.
    generation: u64,
    rebuilding: Option<Receiver<Rebuilt>>,
    drag: Option<(Point<Pixels>, View)>,
    zoom_test: Option<ZoomTest>,
    instant: bool,
}

/// A scripted zoom sweep that reports what zooming costs.
struct ZoomTest {
    started: Option<Instant>,
    frames: Vec<f32>,
    last: Option<Instant>,
    base: f32,
    rebuilds: Vec<u128>,
    peak_rss: u64,
}

/// Quantized so a smooth zoom re-renders rarely: half-octave steps, rounded up to stay sharp.
fn raster_level(k: f32, dpr: f32) -> f32 {
    let steps = ((k * dpr).max(0.25).log2() * 2.0).ceil();
    2.0_f32.powf(steps / 2.0).clamp(0.25, 8.0)
}

struct Timing {
    id: u64,
    text: String,
    asr_ms: Option<u128>,
    llm_ms: u128,
    batch: usize,
    ended: Instant,
    arrived: Instant,
    first_ink: Option<Instant>,
    strokes: usize,
}

fn fill_builder() -> PathBuilder {
    PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::non_zero()))
}

fn color(style: Style) -> Hsla {
    let mut c: Hsla = rgb(style.rgb).into();
    c.a = style.alpha;
    c
}

impl Ink {
    fn retire_detail(&mut self) {
        if let Some((tier, _)) = self.detail.take() {
            self.retired.extend(tier.release());
        }
    }

    /// Take a new board: keep ink whose piece survives, redraw the rest stroke by stroke.
    fn update(&mut self, graph: &Graph, batch: u64) -> usize {
        self.graph = Some(graph.clone());
        let mut layout = self.board.layout(graph, &mut first_diagram);
        let (mut pieces, mut unclear) = compose(graph, &layout, &mut self.glyphs);
        let mut report = check(graph, &layout, &unclear);
        // A real defect outweighs keeping things in place, unless the user is reading somewhere.
        if !report.hard.is_empty() && !self.reading {
            println!("{{\"auto_tidy\":{}}}", report.to_json());
            self.board = StableLayout::default();
            layout = self.board.layout(graph, &mut first_diagram);
            (pieces, unclear) = compose(graph, &layout, &mut self.glyphs);
            report = check(graph, &layout, &unclear);
        }
        self.hints = report.hint();
        self.targets = layout.targets();
        if let Some(id) = &self.selected {
            if !graph.nodes.iter().any(|n| &n.id == id) {
                self.selected = None;
            }
        }
        self.follow(&layout.bounds);
        self.extent = layout.bounds;
        let wanted: HashSet<&str> = pieces.iter().map(|p| p.key.as_str()).collect();
        // Ink cannot be partly erased from a texture, so a removal or a move repaints what remains.
        if self.drawn.iter().any(|k| !wanted.contains(k.as_str())) {
            self.finished.retain(|s| wanted.contains(s.key.as_str()));
            self.base.under.clear();
            self.base.layer.clear();
            for stroke in &self.finished {
                self.base.fill(stroke);
            }
            self.retire_detail();
            self.drawn.retain(|k| wanted.contains(k.as_str()));
            self.generation += 1;
        }
        let in_progress = self.queue.front().map(|q| q.key.clone());
        self.queue.retain(|q| wanted.contains(q.key.as_str()));
        if self.queue.front().map(|q| &q.key) != in_progress.as_ref() {
            self.live = 0.0;
        }
        let queued: HashSet<String> = self.queue.iter().map(|q| q.key.clone()).collect();
        let mut added = 0;
        for piece in pieces {
            if self.drawn.contains(&piece.key) || queued.contains(&piece.key) {
                continue;
            }
            let mut r = Rng::new(&piece.seed);
            for line in &piece.lines {
                self.queue.push_back(Queued {
                    inked: ink(&line.points, line.width, line.wobble, &mut r),
                    style: line.style,
                    key: piece.key.clone(),
                    batch,
                });
                added += 1;
            }
            self.drawn.insert(piece.key);
        }
        added
    }

    /// Forget every position and lay the board out again in one go.
    fn tidy(&mut self) {
        let Some(graph) = self.graph.clone() else { return };
        self.board = StableLayout::default();
        self.next_tidy_batch += 1;
        let batch = u64::MAX - self.next_tidy_batch;
        self.show_latest();
        self.update(&graph, batch);
    }

    /// A click rather than a drag: select what is under the pointer, or clear the selection.
    fn click(&mut self, at: Point<Pixels>) {
        let v = self.shown();
        let x = v.x + (f32::from(at.x) - f32::from(self.canvas.origin.x)) / v.k;
        let y = v.y + (f32::from(at.y) - f32::from(self.canvas.origin.y)) / v.k;
        self.selected = target_at(&self.targets, f64::from(x), f64::from(y)).map(|t| t.id.clone());
    }

    /// Where the toolbar goes, in canvas pixels: under the selection, kept inside the canvas.
    fn toolbar_at(&self) -> Option<(f32, f32)> {
        let id = self.selected.as_ref()?;
        let t = self.targets.iter().find(|t| &t.id == id)?;
        let v = self.shown();
        let (w, h) = (f32::from(self.canvas.size.width), f32::from(self.canvas.size.height));
        let x = (t.x as f32 - v.x) * v.k;
        let below = (t.y as f32 + t.h as f32 - v.y) * v.k + 8.0;
        let above = (t.y as f32 - v.y) * v.k - TOOLBAR_H - 8.0;
        let y = if below + TOOLBAR_H > h { above } else { below };
        Some((x.clamp(8.0, (w - TOOLBAR_W - 8.0).max(8.0)), y.clamp(8.0, (h - TOOLBAR_H - 8.0).max(8.0))))
    }

    /// Fit everything: never above actual size, so a small board is not blown up.
    fn fit(&self) -> View {
        let (w, h) = (f32::from(self.canvas.size.width).max(1.0), f32::from(self.canvas.size.height).max(1.0));
        let e = &self.extent;
        let bw = (e.x1 - e.x0) as f32 + 120.0;
        let bh = (e.y1 - e.y0) as f32 + 120.0;
        let k = (w / bw).min(h / bh).min(1.0);
        View { k, x: (e.x0 + e.x1) as f32 / 2.0 - w / (2.0 * k), y: (e.y0 + e.y1) as f32 / 2.0 - h / (2.0 * k) }
    }

    fn shown(&self) -> View {
        self.view.unwrap_or_else(|| self.fit())
    }

    /// New content keeps the view while it fits, so the board does not rescale with every line.
    fn follow(&mut self, next: &BoardBounds) {
        if self.reading {
            return;
        }
        let cur = self.shown();
        let (w, h) = (f32::from(self.canvas.size.width), f32::from(self.canvas.size.height));
        let inside = self.extent.x1 > self.extent.x0
            && next.x0 as f32 >= cur.x
            && next.y0 as f32 >= cur.y
            && next.x1 as f32 <= cur.x + w / cur.k
            && next.y1 as f32 <= cur.y + h / cur.k;
        let view = inside.then_some(cur);
        if view.map(|v| v.k) != self.view.map(|v| v.k) {
            self.zoomed_at = Instant::now();
        }
        self.view = view;
    }

    /// Zoom around a point on the canvas, as the current app does.
    fn zoom_at(&mut self, at: Point<Pixels>, factor: f32) {
        let v = self.shown();
        let (px_, py) = (f32::from(at.x) - f32::from(self.canvas.origin.x), f32::from(at.y) - f32::from(self.canvas.origin.y));
        let k = (v.k * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let (mx, my) = (v.x + px_ / v.k, v.y + py / v.k);
        self.view = Some(View { k, x: mx - px_ / k, y: my - py / k });
        self.reading = true;
        self.zoomed_at = Instant::now();
    }

    fn show_latest(&mut self) {
        self.reading = false;
        self.view = None;
        self.zoomed_at = Instant::now();
    }

    /// The board area visible now.
    fn visible(&self) -> Area {
        let v = self.shown();
        let (w, h) = (f32::from(self.canvas.size.width), f32::from(self.canvas.size.height));
        [v.x, v.y, v.x + w / v.k, v.y + h / v.k]
    }

    /// Render finished ink at the sharpness this view needs, off the UI thread. Until it arrives
    /// the existing textures stretch, so zooming and panning never wait for the rasterizer.
    fn maintain_sharpness(&mut self) {
        if let Some(rx) = &self.rebuilding {
            let Ok(done) = rx.try_recv() else { return };
            self.rebuilding = None;
            if done.generation == self.generation {
                let mut tier = done.tier;
                for stroke in &self.finished[done.covers..] {
                    if done.cover.is_none_or(|c| meets(stroke.area, c)) {
                        tier.fill(stroke);
                    }
                }
                if let Some(cover) = done.cover {
                    self.retire_detail();
                    self.detail = Some((tier, cover));
                } else {
                    let old = std::mem::replace(&mut self.base, tier);
                    self.retired.extend(old.release());
                }
                if let Some(test) = &mut self.zoom_test {
                    test.rebuilds.push(done.ms);
                }
            }
        }
        if self.zoomed_at.elapsed() < RERASTER_AFTER {
            return;
        }
        let k = self.shown().k;
        let base_wanted = raster_level(k.min(self.fit().k), self.dpr);
        let wanted = raster_level(k, self.dpr);
        let job = if base_wanted != self.base.level {
            Some((base_wanted, None))
        } else if wanted <= self.base.level {
            self.retire_detail();
            None
        } else {
            let view = self.visible();
            let current = self.detail.as_ref().is_some_and(|(t, cover)| t.level == wanted && inside(view, *cover));
            // One screen of margin on every side, so a short pan is already sharp.
            let (w, h) = (view[2] - view[0], view[3] - view[1]);
            (!current).then_some((wanted, Some([view[0] - w, view[1] - h, view[2] + w, view[3] + h])))
        };
        let Some((level, cover)) = job else { return };
        let strokes = self.finished.clone();
        let generation = self.generation;
        let (tx, rx) = channel();
        self.rebuilding = Some(rx);
        std::thread::spawn(move || {
            let started = Instant::now();
            let mut tier = Tier::new(level);
            for s in &strokes {
                if cover.is_none_or(|c: Area| meets(s.area, c)) {
                    tier.fill(s);
                }
            }
            let _ = tx.send(Rebuilt { tier, cover, covers: strokes.len(), generation, ms: started.elapsed().as_millis() });
        });
    }

    /// During a zoom test, says what each slow frame was doing, so a stall can be attributed.
    fn note_frame(&mut self, paint: Duration, uploads_before: usize) {
        let uploads = self.base.uploads() + self.detail.as_ref().map_or(0, |(t, _)| t.uploads()) - uploads_before;
        let k = self.shown().k;
        let detail = self.detail.is_some();
        let rebuilding = self.rebuilding.is_some();
        let Some(test) = &mut self.zoom_test else { return };
        let gap = test.frames.last().copied().unwrap_or(0.0);
        if gap > 50.0 || paint.as_millis() > 20 {
            eprintln!("slow frame: gap {gap:.0} ms, paint {} ms, uploads {uploads}, k {k:.2}, detail {detail}, rebuilding {rebuilding}", paint.as_millis());
        }
    }

    /// Advance the scripted sweep by one frame; returns true while it is running. It pauses at
    /// each extreme, because the sharper textures are only built once the view stops moving.
    fn sweep(&mut self) -> bool {
        if !self.queue.is_empty() {
            return false;
        }
        let k_fit = self.fit().k;
        let Some(test) = &mut self.zoom_test else { return false };
        let now = Instant::now();
        let started = *test.started.get_or_insert(now);
        if test.base == 0.0 {
            test.base = k_fit;
        }
        if let Some(last) = test.last.replace(now) {
            test.frames.push((now - last).as_secs_f32() * 1000.0);
        }
        let t = (now - started).as_secs_f32();
        let base = test.base;
        let ease = |from: f32, to: f32, p: f32| from * (to / from).powf(p.clamp(0.0, 1.0));
        // In to 4x, hold, pan across the board, hold, out to the minimum, hold, back to the fit.
        let (k, pan, moving) = match t {
            t if t < 2.0 => (ease(base, MAX_ZOOM, t / 2.0), 0.0, true),
            t if t < 3.0 => (MAX_ZOOM, 0.0, false),
            t if t < 6.0 => (MAX_ZOOM, (t - 3.0) * 800.0, false),
            t if t < 7.0 => (MAX_ZOOM, 2400.0, false),
            t if t < 9.0 => (ease(MAX_ZOOM, MIN_ZOOM, (t - 7.0) / 2.0), 0.0, true),
            t if t < 10.0 => (MIN_ZOOM, 0.0, false),
            t if t < 12.0 => (ease(MIN_ZOOM, base, (t - 10.0) / 2.0), 0.0, true),
            _ => (base, 0.0, false),
        };
        test.peak_rss = test.peak_rss.max(rss_mb());
        if t > 12.5 {
            let mut f = std::mem::take(&mut test.frames);
            f.sort_by(f32::total_cmp);
            let q = |p: f32| f[((f.len() - 1) as f32 * p) as usize];
            println!(
                "{{\"zoom_test\":true,\"strokes\":{},\"frames\":{},\"frame_p50\":{:.2},\"frame_p95\":{:.2},\"frame_p99\":{:.2},\"frame_max\":{:.1},\"rebuilds_ms\":{:?},\"peak_rss_mb\":{}}}",
                self.finished.len(), f.len(), q(0.5), q(0.95), q(0.99), f[f.len() - 1], test.rebuilds, test.peak_rss,
            );
            std::process::exit(0);
        }
        let e = &self.extent;
        let (w, h) = (f32::from(self.canvas.size.width), f32::from(self.canvas.size.height));
        self.view = Some(View { k, x: (e.x0 + e.x1) as f32 / 2.0 - w / (2.0 * k) + pan / k, y: (e.y0 + e.y1) as f32 / 2.0 - h / (2.0 * k) });
        self.reading = true;
        if moving {
            self.zoomed_at = now;
        }
        true
    }

    fn advance(&mut self) {
        let now = Instant::now();
        let dt = self.last.replace(now).map_or(0.016, |l| (now - l).as_secs_f32()).min(0.1);
        let mut budget = if self.instant { f32::MAX } else { PEN_PX_PER_SECOND * (1.0 + self.queue.len() as f32 / 60.0) * dt };
        while budget > 0.0 {
            let Some(front) = self.queue.front() else { break };
            let (id, length) = (front.batch, front.inked.length);
            if let Some(t) = self.timings.iter_mut().find(|t| t.id == id && t.first_ink.is_none()) {
                t.first_ink = Some(now);
            }
            let left = length * (1.0 - self.live);
            if left > budget {
                self.live += budget / length.max(0.01);
                return;
            }
            budget -= left;
            self.live = 0.0;
            let done = self.queue.pop_front().expect("front exists");
            let shape = outline(&done.inked, 1.0);
            let stroke = Arc::new(Stroke { key: done.key, area: area_of(&shape), shape, style: done.style });
            self.base.fill(&stroke);
            if let Some((tier, cover)) = &mut self.detail {
                if meets(stroke.area, *cover) {
                    tier.fill(&stroke);
                }
            }
            self.finished.push(stroke);
            if !self.queue.iter().any(|q| q.batch == done.batch) {
                self.report(done.batch, now);
            }
        }
    }

    fn report(&mut self, id: u64, now: Instant) {
        let Some(i) = self.timings.iter().position(|t| t.id == id) else { return };
        let t = self.timings.remove(i);
        let first = t.first_ink.unwrap_or(now);
        println!(
            "{{\"line\":{},\"batch\":{},\"asr_ms\":{},\"llm_ms\":{},\"to_board_ms\":{},\"to_first_ink_ms\":{},\"to_done_ms\":{},\"ui_delay_ms\":{},\"strokes\":{}}}",
            serde_json::Value::from(t.text),
            t.batch,
            t.asr_ms.map_or("null".to_string(), |v| v.to_string()),
            t.llm_ms,
            (t.arrived - t.ended).as_millis(),
            (first - t.ended).as_millis(),
            (now - t.ended).as_millis(),
            (first - t.arrived).as_millis(),
            t.strokes,
        );
    }
}

struct Heard {
    speaker: Option<String>,
    text: String,
    note: String,
}

struct Workbench {
    ink: Rc<RefCell<Ink>>,
    pipeline: Pipeline,
    input: Entity<InputState>,
    lines: Vec<Heard>,
    status: String,
    level: f32,
    next_batch: u64,
    exit_when_done: bool,
}

impl Workbench {
    fn pump(&mut self, cx: &mut Context<Self>) {
        let mut changed = false;
        {
            let mut ink = self.ink.borrow_mut();
            let before = (ink.base.level, ink.detail.as_ref().map(|(t, c)| (t.level, *c)));
            ink.maintain_sharpness();
            changed |= (ink.base.level, ink.detail.as_ref().map(|(t, c)| (t.level, *c))) != before;
        }
        while let Ok(event) = self.pipeline.events.try_recv() {
            changed = true;
            match event {
                Event::Agent(Update::Status(s)) => self.status = s,
                Event::Level(l) => self.level = l,
                Event::Heard { speaker, text, asr_ms } => {
                    let note = asr_ms.map_or(String::new(), |ms| format!("识别 {ms} ms"));
                    self.lines.push(Heard { speaker, text, note });
                }
                Event::Agent(Update::Drawn { graph, ops, text, asr_ms, llm_ms, batch, ended }) => {
                    println!("{{\"ops\":{},\"board\":{}}}", serde_json::Value::from(ops.clone()), serde_json::Value::from(graph.describe()));
                    self.next_batch += 1;
                    let id = self.next_batch;
                    let arrived = Instant::now();
                    let mut ink = self.ink.borrow_mut();
                    let strokes = ink.update(&graph, id);
                    if strokes > 0 {
                        ink.timings.push(Timing { id, text: text.clone(), asr_ms, llm_ms, batch, ended, arrived, first_ink: None, strokes });
                    }
                    drop(ink);
                    if let Some(line) = self.lines.iter_mut().rev().find(|l| l.text == text) {
                        line.note = format!("{} 模型 {llm_ms} ms", line.note).trim().to_string();
                    }
                    self.status = format!("板书 {}", ops.replace('\n', "  "));
                }
                Event::Agent(Update::Failed(e)) | Event::Failed(e) => {
                    eprintln!("failed: {e}");
                    self.status = format!("出错：{e}");
                }
                Event::Done => {
                    println!("{{\"done\":true,\"rss_mb\":{}}}", rss_mb());
                    if self.exit_when_done {
                        cx.spawn(async move |_, cx| {
                            cx.background_executor().timer(Duration::from_secs(8)).await;
                            std::process::exit(0);
                        })
                        .detach();
                    }
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    /// Toggle a mark by hand. It is drawn at once, in the same pen as a mark from the model.
    fn mark(&mut self, id: &str, mark: &str, cx: &mut Context<Self>) {
        let graph = self.pipeline.toggle_mark(id, mark);
        self.next_batch += 1;
        self.ink.borrow_mut().update(&graph, self.next_batch);
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        self.input.update(cx, |state, cx| state.set_value("", window, cx));
        self.lines.push(Heard { speaker: None, text: text.clone(), note: String::new() });
        let _ = self.pipeline.said.send(Said { line: Utterance { speaker: None, text }, ended: Instant::now(), asr_ms: None });
        cx.notify();
    }
}

/// A recorded sample replayed from its stored model output, with no model involved.
fn demo_board(root: &std::path::Path, name: &str) -> Result<Graph> {
    #[derive(Deserialize)]
    struct Demo {
        lines: Vec<DemoLine>,
    }
    #[derive(Deserialize)]
    struct DemoLine {
        ops: Option<String>,
    }
    let demo: Demo = serde_json::from_slice(&std::fs::read(root.join(format!("public/demos/{name}.json")))?)?;
    let mut g = Graph::default();
    for line in demo.lines {
        if let Some(ops) = line.ops {
            g.apply(&parse_ops(&ops));
        }
    }
    Ok(g)
}

/// A big board from plain operations, for measuring size rather than the model.
fn synthetic_board(chars: usize) -> Graph {
    const NAMES: [&str; 8] = ["灰度发布", "回滚演练", "负载测试", "Release notes", "支付回调", "消息队列", "无障碍审计", "数据库缓存"];
    let mut ops = String::new();
    let mut written = 0;
    let mut i = 0;
    while written < chars {
        let topic = format!("话题{i}");
        ops += &format!("T 「{topic}」\n");
        written += topic.chars().count();
        for j in 0..6 {
            let item = format!("{}{i}-{j}", NAMES[(i + j) % NAMES.len()]);
            written += item.chars().count();
            ops += &format!("L 「{topic}」 「{item}」\n");
            match (i + j) % 7 {
                0 => ops += &format!("M 「{item}」 划掉\n"),
                1 => ops += &format!("M 「{item}」 圈出\n"),
                2 => ops += &format!("M 「{item}」 下划线\n"),
                3 => ops += &format!("M 「{item}」 星标\n"),
                4 => ops += &format!("M 「{item}」 高亮\n"),
                _ => {}
            }
        }
        if i % 4 == 0 && i < 40 {
            ops += &format!("E 「服务{i}」 -> 「服务{}」 : 「调用」\n", i + 4);
            ops += &format!("E 「服务{i}」 -> 「存储{i}」\n");
            written += 8;
        }
        i += 1;
    }
    let mut g = Graph::default();
    g.apply(&parse_ops(&ops));
    g
}

fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map_or(0, |pages| pages * 4096 / 1_048_576)
}

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Only animate while there is ink to write; an idle board costs no frames.
        {
            let ink = self.ink.borrow();
            if !ink.queue.is_empty() || ink.zoom_test.is_some() {
                window.request_animation_frame();
            }
        }
        // Hints go to the model with the next line it is asked about.
        if let Some(hint) = self.ink.borrow_mut().hints.take() {
            self.pipeline.set_hint(hint);
        }
        let (reading, zoom) = {
            let ink = self.ink.borrow();
            (ink.reading, ink.shown().k)
        };
        let ink = self.ink.clone();
        let wheel = self.ink.clone();
        let press = self.ink.clone();
        let moved = self.ink.clone();
        let release = self.ink.clone();
        let listening = self.pipeline.listening();
        let mut board = div()
            .id("board")
            .relative()
            .flex_1()
            .h_full()
            .overflow_hidden()
            .bg(rgb(PAPER))
            .on_scroll(cx.listener(move |_, e: &ScrollEvent, window, cx| {
                let dy = f32::from(e.delta.pixel_delta(window.line_height()).y);
                // Same response as the current app's wheel: exponential in the scrolled distance.
                wheel.borrow_mut().zoom_at(e.position, (dy * 0.0015).exp());
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(move |_, e: &MouseDownEvent, _, _| {
                let mut b = press.borrow_mut();
                let view = b.shown();
                b.drag = Some((e.position, view));
            }))
            .on_mouse_move(cx.listener(move |_, e: &MouseMoveEvent, _, cx| {
                let mut b = moved.borrow_mut();
                if let Some((from, view)) = b.drag.filter(|_| e.pressed_button == Some(MouseButton::Left)) {
                    let d = e.position - from;
                    b.view = Some(View { x: view.x - f32::from(d.x) / view.k, y: view.y - f32::from(d.y) / view.k, k: view.k });
                    b.reading = true;
                    b.zoomed_at = Instant::now();
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(move |_, e: &MouseUpEvent, _, cx| {
                let mut b = release.borrow_mut();
                if let Some((from, _)) = b.drag.take() {
                    let d = e.position - from;
                    if f32::from(d.x).hypot(f32::from(d.y)) < CLICK_SLOP {
                        b.click(e.position);
                        cx.notify();
                    }
                }
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds: Bounds<Pixels>, (), window: &mut Window, _: &mut App| {
                        let mut b = ink.borrow_mut();
                        b.canvas = bounds;
                        b.dpr = window.scale_factor();
                        let started = Instant::now();
                        let uploads_before = b.base.uploads() + b.detail.as_ref().map_or(0, |(t, _)| t.uploads());
                        // Before the first stroke: a big board must not start at full resolution.
                        if b.finished.is_empty() {
                            let level = raster_level(b.fit().k, b.dpr);
                            if b.base.level != level {
                                b.base = Tier::new(level);
                            }
                        }
                        b.sweep();
                        b.advance();
                        let v = b.shown();
                        let view = b.visible();
                        let origin = bounds.origin + point(px(-v.x * v.k), px(-v.y * v.k));
                        for image in std::mem::take(&mut b.retired) {
                            let _ = window.drop_image(image);
                        }
                        window.with_content_mask(Some(gpui_kit::ContentMask { bounds }), |window| {
                            let state = &mut *b;
                            // The detail tier replaces the base rather than overlaying it, or
                            // the two copies of each stroke would thicken and blur together.
                            let tier = match &mut state.detail {
                                Some((tier, cover)) if inside(view, *cover) => tier,
                                _ => &mut state.base,
                            };
                            tier.under.paint(window, origin, v.k, Some(bounds));
                            tier.layer.paint(window, origin, v.k, Some(bounds));
                            if let Some(front) = b.queue.front().filter(|_| b.live > 0.0) {
                                let shape = outline(&front.inked, b.live);
                                let mut pb = fill_builder();
                                if let Some(first) = shape.first() {
                                    let at = |p: &Pt| origin + point(px(p[0] * v.k), px(p[1] * v.k));
                                    pb.move_to(at(first));
                                    for p in &shape[1..] {
                                        pb.line_to(at(p));
                                    }
                                    pb.close();
                                }
                                if let Ok(path) = pb.build() {
                                    window.paint_path(path, Background::from(color(front.style)));
                                }
                            }
                        });
                        b.note_frame(started.elapsed(), uploads_before);
                    },
                )
                .size_full(),
            );
        let toolbar = {
            let ink = self.ink.borrow();
            ink.toolbar_at().zip(ink.selected.clone())
        };
        if let Some(((x, y), id)) = toolbar {
            let active: Vec<String> = self
                .ink
                .borrow()
                .graph
                .as_ref()
                .and_then(|g| g.nodes.iter().find(|n| n.id == id))
                .map_or(vec![], |n| n.marks.clone());
            board = board.child(
                div()
                    .id("marks")
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .h(px(TOOLBAR_H))
                    .flex()
                    .items_center()
                    .gap_1()
                    .px_1()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0xdd_dd_d8))
                    .bg(rgb(0xff_fe_fa))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(MARKS.iter().map(|mark| {
                        let on = active.iter().any(|m| m == mark);
                        let id = id.clone();
                        div()
                            .id(*mark)
                            .px_2()
                            .py_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .text_sm()
                            .when(on, |d| d.bg(rgb(0x24_27_20)).text_color(rgb(0xff_ff_ff)))
                            .child(*mark)
                            .on_click(cx.listener(move |this, _, _, cx| this.mark(&id, mark, cx)))
                    })),
            );
        }
        if reading {
            let back = self.ink.clone();
            board = board.child(
                div()
                    .id("latest")
                    .absolute()
                    .bottom(px(16.))
                    .left(px(16.))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .bg(rgb(0x24_27_20))
                    .text_color(rgb(0xff_ff_ff))
                    .text_sm()
                    .child("回到最新")
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |_, _, _, cx| {
                        back.borrow_mut().show_latest();
                        cx.notify();
                    })),
            );
        }
        let meter = (self.level.min(1.0) * 100.0).max(2.0);
        let bar = div()
            .flex()
            .items_center()
            .gap_3()
            .px_3()
            .h(px(44.))
            .border_b_1()
            .border_color(rgb(0xdd_dd_d8))
            .child(div().font_weight(gpui_kit::FontWeight::SEMIBOLD).child("boardcast · GPUI 原型"))
            .child(
                div()
                    .id("mic")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .text_color(rgb(0xff_ff_ff))
                    .bg(rgb(if listening { 0xc7_44_32 } else { 0x24_27_20 }))
                    .child(if listening { "停止收音" } else { "开始收音" })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.pipeline.toggle_mic();
                        cx.notify();
                    })),
            )
            .child(div().w(px(80.)).h(px(6.)).rounded_sm().bg(rgb(0xe4_e4_df)).child(
                div().h_full().rounded_sm().w(px(meter * 0.8)).bg(rgb(0x28_61_43)),
            ))
            .child(div().text_sm().text_color(rgb(0x62_66_5c)).child(format!("{:.0}%", zoom * 100.0)))
            .child(
                div()
                    .id("tidy")
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .text_sm()
                    .border_1()
                    .border_color(rgb(0xdd_dd_d8))
                    .child("整理排版")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.ink.borrow_mut().tidy();
                        cx.notify();
                    })),
            )
            .child(div().flex_1().overflow_hidden().text_sm().text_color(rgb(0x62_66_5c)).child(self.status.clone()));
        let transcript = div()
            .id("transcript")
            .flex_1()
            .overflow_y_scroll()
            .p_3()
            .gap_3()
            .flex()
            .flex_col()
            .children(self.lines.iter().rev().map(|l| {
                div()
                    .flex()
                    .flex_col()
                    .when_some(l.speaker.clone(), |d, s| d.child(div().text_xs().font_weight(gpui_kit::FontWeight::SEMIBOLD).child(s)))
                    .child(div().text_sm().child(l.text.clone()))
                    .when(!l.note.is_empty(), |d| d.child(div().text_xs().text_color(rgb(0x85_8a_80)).child(l.note.clone())))
            }));
        let side = div()
            .w(px(320.))
            .h_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(rgb(0xdd_dd_d8))
            .child(transcript)
            .child(div().p_3().border_t_1().border_color(rgb(0xdd_dd_d8)).child(Input::new(&self.input)));
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xff_fe_fa))
            .text_color(rgb(INK_RGB))
            .child(bar)
            .child(div().flex_1().flex().min_h_0().child(board).child(side))
    }
}

#[derive(Deserialize)]
struct Case {
    id: String,
    turns: Vec<Turn>,
}

#[derive(Deserialize)]
struct Turn {
    speaker: Option<String>,
    text: String,
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("boardcast root")?);
    let mut case = None;
    let mut wav = None;
    let mut llm = None;
    let mut exit_when_done = false;
    let mut synthetic: Option<usize> = None;
    let mut zoom_test = false;
    let mut demo: Option<String> = None;
    let mut zoom: Option<f32> = None;
    let mut skill = false;
    let mut deepseek = false;
    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--case" => case = args.next(),
            "--wav" => wav = args.next().map(PathBuf::from),
            "--llm" => llm = args.next(),
            "--exit-when-done" => exit_when_done = true,
            "--synthetic" => synthetic = args.next().map(|n| n.parse()).transpose()?,
            "--zoom-test" => zoom_test = true,
            "--demo" => demo = args.next(),
            "--zoom" => zoom = args.next().map(|v| v.parse()).transpose()?,
            "--skill" => skill = true,
            "--deepseek" => deepseek = true,
            other => anyhow::bail!("unknown flag {other}"),
        }
    }
    let mut turns = vec![];
    if let Some(id) = case {
        let cases: Vec<Case> = serde_json::from_slice(&std::fs::read(root.join("eval/cases.json"))?)?;
        let found = cases.into_iter().find(|c| c.id == id).with_context(|| format!("no case {id}"))?;
        turns = found.turns.into_iter().map(|t| Utterance { speaker: t.speaker, text: t.text }).collect();
    }
    let home = std::env::var("HOME")?;
    let models = PathBuf::from(home).join(".local/share/games.mmstudio.boardcast/models");
    let backend = if deepseek {
        Backend::Ready(Provider::deepseek_from_env()?)
    } else if let Some(base) = llm {
        Backend::Ready(Provider::Local { base })
    } else {
        Backend::ManagedLocal(LocalServer { binaries: root.join("src-tauri/llama"), model: models.join("Qwen3.5-4B-Q4_K_M.gguf") })
    };
    let guidance = skill.then_some(Guidance { rules: true, hint: true });
    let glyphs = Glyphs::load(&root)?;
    let start_board = match &demo {
        Some(name) => Some(demo_board(&root, name)?),
        None => synthetic.map(synthetic_board),
    };
    let pipeline = Pipeline::start(backend, guidance, models.join("voice"), Script { turns, wav });
    if let Some(board) = &start_board {
        pipeline.seed(board.clone());
    }
    gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(move |cx: &mut App| {
        gpui_kit::init(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(point(px(0.), px(0.)), size(px(1280.), px(800.))))),
            ..Default::default()
        };
        let mut state = Ink {
            glyphs,
            board: StableLayout::default(),
            base: Tier::new(1.0),
            detail: None,
            retired: vec![],
            queue: VecDeque::new(),
            live: 0.0,
            finished: vec![],
            drawn: HashSet::new(),
            last: None,
            timings: vec![],
            graph: None,
            next_tidy_batch: 0,
            targets: vec![],
            selected: None,
            hints: None,
            extent: BoardBounds::default(),
            view: None,
            reading: false,
            canvas: Bounds::new(point(px(0.), px(0.)), size(px(960.), px(756.))),
            dpr: 1.0,
            zoomed_at: Instant::now(),
            generation: 0,
            rebuilding: None,
            drag: None,
            zoom_test: zoom_test.then_some(ZoomTest { started: None, frames: vec![], last: None, base: 0.0, rebuilds: vec![], peak_rss: 0 }),
            instant: start_board.is_some(),
        };
        if let Some(graph) = &start_board {
            let started = Instant::now();
            let strokes = state.update(graph, 0);
            println!("{{\"synthetic_nodes\":{},\"strokes\":{strokes},\"layout_and_compose_ms\":{}}}", graph.nodes.len(), started.elapsed().as_millis());
        }
        if let Some(k) = zoom {
            let e = &state.extent;
            state.view = Some(View { k, x: e.x0 as f32 - 20.0 / k, y: e.y0 as f32 - 20.0 / k });
            state.reading = true;
        }
        let ink = Rc::new(RefCell::new(state));
        cx.open_window(options, move |window, cx| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("输入一句话，回车上板"));
            let view = cx.new(|cx| {
                cx.subscribe_in(&input, window, |this: &mut Workbench, _, event, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.submit(window, cx);
                    }
                })
                .detach();
                cx.spawn(async move |this, cx| loop {
                    cx.background_executor().timer(Duration::from_millis(33)).await;
                    if this.update(cx, Workbench::pump).is_err() {
                        break;
                    }
                })
                .detach();
                Workbench {
                    ink,
                    pipeline,
                    input,
                    lines: vec![],
                    status: "启动中…".into(),
                    level: 0.0,
                    next_batch: 0,
                    exit_when_done,
                }
            });
            cx.new(|cx| gpui_kit::base::Root::new(view, window, cx))
        })
        .expect("open window");
    });
    Ok(())
}
