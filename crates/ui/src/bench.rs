//! Spike: can GPUI draw boardcast's handwriting? It writes a growing board and reports frame
//! timing as JSON lines, so the limit comes from measurement rather than opinion.

use anyhow::{Context as _, Result};
use boardcast_board::glyphs::{Glyphs, Pt};
use gpui_kit::{
    canvas, div, point, prelude::*, px, rgb, size, App, Background, Bounds, Context, FillOptions,
    Hsla, Path, PathBuilder, PathStyle, Pixels, Render, Window, WindowBounds, WindowOptions,
};
use boardcast_board::pen::{arrow, circle_mark, ink, line, outline, place, rough_box, star, strike, Inked, Rng};
use crate::raster::Layer;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

const TEXT: &str = "用户打开小程序调用登录接口网关转发到订单服务库存服务支付回调数据库缓存消息队列灰度发布回滚演练负载测试无障碍审计发布说明Release checklist 上线前必须完成压测";
const FONT: f32 = 22.0;
const LINE: f32 = 34.0;
const MARGIN: f32 = 24.0;
const INK: u32 = 0x1e1e1e;
const BOARD_WIDTH: f32 = 1280.0;
const BOARD_BOTTOM: f32 = 760.0;

/// How finished strokes reach the GPU. GPUI re-submits every path each frame, so this decides how
/// large a board stays smooth.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Batch {
    /// One Path per stroke, the direct translation of the SVG renderer.
    Stroke,
    /// Finished strokes share one Path per this many strokes.
    Merged(usize),
    /// Finished strokes become tiled textures; only the stroke being written is a Path.
    Raster,
}

/// Color and layer of one stroke. Highlights sit under the ink, as `under` does in compose.ts.
#[derive(Clone, Copy)]
struct Style {
    rgb: u32,
    alpha: f32,
    under: bool,
}

const INK_STYLE: Style = Style { rgb: INK, alpha: 1.0, under: false };
const RED: Style = Style { rgb: 0xe0_31_31, alpha: 1.0, under: false };
const HIGHLIGHT: Style = Style { rgb: 0xff_ec_99, alpha: 0.75, under: true };
/// Characters per label; each finished label receives the next mark in turn.
const LABEL: usize = 6;

impl Style {
    fn color(self) -> Hsla {
        let mut c: Hsla = rgb(self.rgb).into();
        c.a = self.alpha;
        c
    }
}

struct Pending {
    strokes: Vec<(Inked, Style)>,
    index: usize,
    drawn: f32,
}

struct Bench {
    targets: Vec<usize>,
    target: usize,
    frames: Vec<f32>,
    paints: Vec<f32>,
    writing: Vec<f32>,
    last: Option<Instant>,
    settle: u32,
    started: Instant,
}

struct Board {
    glyphs: Glyphs,
    text: Vec<char>,
    next_char: usize,
    x: f32,
    y: f32,
    pending: Option<Pending>,
    finished: Vec<(Path<Pixels>, Hsla)>,
    layer: Layer,
    under: Layer,
    marks: bool,
    next_mark: usize,
    drawn_marks: usize,
    /// Where the current label started, and how many characters it has.
    label: (f32, f32, usize),
    /// The middle of the previous label's right edge, for arrows between labels.
    previous: Option<Pt>,
    open: Option<PathBuilder>,
    open_count: usize,
    batch: Batch,
    strokes: usize,
    chars: usize,
    vertices: usize,
    /// Pen speed in pixels per frame.
    speed: f32,
    bench: Bench,
}

fn fill() -> PathBuilder {
    // Non-zero: a stroke crossing itself must stay solid rather than punch a hole.
    PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::non_zero()))
}

fn add_shape(b: &mut PathBuilder, shape: &[Pt]) {
    let Some(first) = shape.first() else { return };
    b.move_to(point(px(first[0]), px(first[1])));
    for p in &shape[1..] {
        b.line_to(point(px(p[0]), px(p[1])));
    }
    b.close();
}

fn pen_for(size: f32) -> f32 {
    (size * 0.1).max(1.8)
}

impl Board {
    fn advance(&mut self) {
        if self.pending.is_none() {
            self.start_char();
        }
        let Some(mut p) = self.pending.take() else {
            self.flush();
            return;
        };
        let mut budget = self.speed;
        while p.index < p.strokes.len() && budget > 0.0 {
            let (s, style) = &p.strokes[p.index];
            let style = *style;
            let left = s.length * (1.0 - p.drawn);
            if left > budget {
                p.drawn += budget / s.length.max(0.01);
                budget = 0.0;
            } else {
                budget -= left;
                let shape = outline(s, 1.0);
                self.finish(&shape, style);
                p.index += 1;
                p.drawn = 0.0;
            }
        }
        if p.index < p.strokes.len() {
            self.pending = Some(p);
        }
    }

    fn start_char(&mut self) {
        if self.marks && self.label.2 >= LABEL {
            self.start_mark();
            return;
        }
        if self.chars >= self.bench.target {
            return;
        }
        let ch = self.text[self.next_char % self.text.len()];
        self.next_char += 1;
        let Some(g) = self.glyphs.get(ch) else {
            self.x += FONT * 0.6;
            return;
        };
        if self.x + FONT * g.advance > BOARD_WIDTH - MARGIN {
            self.x = MARGIN;
            self.y += LINE;
            self.label = (self.x, self.y, 0);
        }
        // Rows wrap back to the top with an offset, so every stroke stays on screen and is
        // really rasterized at high counts instead of being clipped away.
        if self.y > BOARD_BOTTOM {
            self.y = MARGIN + (self.chars % 11) as f32;
            self.label = (self.x, self.y, 0);
        }
        let mut r = Rng::new(&format!("{ch}{}", self.chars));
        let width = pen_for(FONT) * 0.65;
        let placed = place(&g.strokes, self.x, self.y, FONT, &mut r);
        let strokes = placed.iter().map(|s| (ink(s, width, pen_for(FONT) * 0.12, &mut r), INK_STYLE)).collect();
        self.pending = Some(Pending { strokes, index: 0, drawn: 0.0 });
        self.x += FONT * g.advance;
        self.chars += 1;
        self.label.2 += 1;
    }

    /// Draw the next mark over the finished label, stroke by stroke like the writing itself.
    fn start_mark(&mut self) {
        let (x, y, _) = self.label;
        let w = (self.x - x).max(FONT);
        let kind = self.next_mark % 7;
        self.next_mark += 1;
        self.drawn_marks += 1;
        let mut r = Rng::new(&format!("mark{}", self.next_mark));
        let (shapes, style, width) = match kind {
            0 => (strike(x, y + FONT * 0.5, w, &mut r), INK_STYLE, 1.0),
            1 => (vec![line([x - 2.0, y + FONT + 5.0], [x + w + 2.0, y + FONT + 3.0], &mut r, 0.02)], RED, 1.0),
            2 => (vec![circle_mark(x + w / 2.0, y + FONT / 2.0, w / 2.0 + 8.0, FONT * 0.72, &mut r)], RED, 1.0),
            3 => (vec![star(x + w + 12.0, y + FONT * 0.45, 10.0, &mut r)], RED, 1.0),
            4 => (vec![line([x - 2.0, y + FONT / 2.0], [x + w + 2.0, y + FONT / 2.0], &mut r, 0.004)], HIGHLIGHT, 10.0),
            5 => (rough_box(x - 6.0, y - 5.0, w + 12.0, FONT + 10.0, &mut r), INK_STYLE, 1.0),
            _ => match self.previous {
                Some(from) => (arrow(&[from, [x - 3.0, y + FONT / 2.0]], &mut r), INK_STYLE, 1.0),
                None => (Vec::new(), INK_STYLE, 1.0),
            },
        };
        let pen = pen_for(FONT) * 0.65 * width;
        let strokes = shapes.iter().map(|s| (ink(s, pen, pen_for(FONT) * 0.12, &mut r), style)).collect();
        self.pending = Some(Pending { strokes, index: 0, drawn: 0.0 });
        self.previous = Some([x + w + 3.0, y + FONT / 2.0]);
        self.label = (self.x + 8.0, self.y, 0);
        self.x += 8.0;
    }

    fn finish(&mut self, shape: &[Pt], style: Style) {
        self.strokes += 1;
        match self.batch {
            Batch::Raster => {
                let [_, r, g, b] = style.rgb.to_be_bytes();
                let layer = if style.under { &mut self.under } else { &mut self.layer };
                layer.fill(shape, [r, g, b], (style.alpha * 255.0) as u8);
            }
            Batch::Stroke => {
                let mut b = fill();
                add_shape(&mut b, shape);
                self.push(b, style);
            }
            Batch::Merged(limit) => {
                let mut b = self.open.take().unwrap_or_else(fill);
                add_shape(&mut b, shape);
                self.open_count += 1;
                if self.open_count >= limit {
                    self.open_count = 0;
                    self.push(b, INK_STYLE);
                } else {
                    self.open = Some(b);
                }
            }
        }
    }

    fn flush(&mut self) {
        if let Some(b) = self.open.take() {
            self.open_count = 0;
            self.push(b, INK_STYLE);
        }
    }

    fn push(&mut self, b: PathBuilder, style: Style) {
        if let Ok(path) = b.build() {
            self.vertices += path.vertices.len();
            self.finished.push((path, style.color()));
        }
    }

    fn live_shape(&self) -> Option<(Vec<Pt>, Style)> {
        let p = self.pending.as_ref()?;
        let (s, style) = &p.strokes[p.index];
        (p.drawn > 0.0).then(|| (outline(s, p.drawn), *style))
    }
}

fn percentile(sorted: &[f32], q: f32) -> f32 {
    sorted[((sorted.len() - 1) as f32 * q) as usize]
}

fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map_or(0, |pages| pages * 4096 / 1_048_576)
}

fn report(b: &mut Board, paint: Duration) {
    let now = Instant::now();
    let reached = b.chars >= b.bench.target && b.pending.is_none();
    if let Some(last) = b.bench.last.replace(now) {
        if !reached {
            b.bench.writing.push((now - last).as_secs_f32() * 1000.0);
        } else {
            if b.bench.settle > 0 {
                b.bench.settle -= 1;
            } else {
                b.bench.frames.push((now - last).as_secs_f32() * 1000.0);
                b.bench.paints.push(paint.as_secs_f32() * 1000.0);
            }
        }
    }
    if b.bench.frames.len() < 240 {
        return;
    }
    let mut frames = std::mem::take(&mut b.bench.frames);
    let mut paints = std::mem::take(&mut b.bench.paints);
    frames.sort_by(f32::total_cmp);
    paints.sort_by(f32::total_cmp);
    let mut writing = std::mem::take(&mut b.bench.writing);
    writing.sort_by(f32::total_cmp);
    let (write_p50, write_p95, write_max) = if writing.is_empty() {
        (0.0, 0.0, 0.0)
    } else {
        (percentile(&writing, 0.5), percentile(&writing, 0.95), writing[writing.len() - 1])
    };
    println!(
        "{{\"batch\":\"{:?}\",\"chars\":{},\"marks\":{},\"strokes\":{},\"paths\":{},\"vertices\":{},\"frame_p50\":{:.2},\"frame_p95\":{:.2},\"frame_p99\":{:.2},\"paint_p50\":{:.2},\"paint_p95\":{:.2},\"write_p50\":{:.2},\"write_p95\":{:.2},\"write_max\":{:.1},\"tiles\":{},\"uploads\":{},\"rss_mb\":{},\"elapsed_s\":{:.1}}}",
        b.batch, b.chars, b.drawn_marks, b.strokes, b.finished.len(), b.vertices,
        percentile(&frames, 0.5), percentile(&frames, 0.95), percentile(&frames, 0.99),
        percentile(&paints, 0.5), percentile(&paints, 0.95), write_p50, write_p95, write_max,
        b.layer.tile_count() + b.under.tile_count(), b.layer.uploads + b.under.uploads, rss_mb(), b.bench.started.elapsed().as_secs_f32(),
    );
    if b.bench.targets.is_empty() {
        std::process::exit(0);
    }
    b.bench.target = b.bench.targets.remove(0);
    b.bench.settle = 30;
}

struct View {
    board: Rc<RefCell<Board>>,
}

impl Render for View {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        window.request_animation_frame();
        let board = self.board.clone();
        div().size_full().bg(rgb(0xfb_fb_f9)).child(
            canvas(
                |_, _, _| (),
                move |_bounds, (), window: &mut Window, _cx: &mut App| {
                    let started = Instant::now();
                    let mut b = board.borrow_mut();
                    b.advance();
                    let origin = point(px(0.), px(0.));
                    b.under.paint(window, origin, 1.0, None);
                    b.layer.paint(window, origin, 1.0, None);
                    for (path, color) in &b.finished {
                        window.paint_path(path.clone(), Background::from(*color));
                    }
                    if let Some((shape, style)) = b.live_shape() {
                        let mut pb = fill();
                        add_shape(&mut pb, &shape);
                        if let Ok(path) = pb.build() {
                            window.paint_path(path, Background::from(style.color()));
                        }
                    }
                    report(&mut b, started.elapsed());
                },
            )
            .size_full(),
        )
    }
}

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let root = PathBuf::from(args.next().context("usage: spike <boardcast-root> <stroke|merged:N> <speed> <targets,..>")?);
    let batch = match args.next().as_deref() {
        Some("stroke") | None => Batch::Stroke,
        Some("raster") => Batch::Raster,
        Some(s) => Batch::Merged(s.strip_prefix("merged:").context("batch")?.parse()?),
    };
    let speed: f32 = args.next().as_deref().unwrap_or("100000").parse()?;
    let mut targets: Vec<usize> = args
        .next()
        .as_deref()
        .unwrap_or("100,500,1000,2000")
        .split(',')
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    let first = targets.remove(0);
    let marks = args.next().as_deref() == Some("marks");
    let board = Board {
        glyphs: Glyphs::from_checkout(&root)?,
        text: TEXT.chars().collect(),
        next_char: 0,
        x: MARGIN,
        y: MARGIN,
        pending: None,
        finished: Vec::new(),
        layer: Layer::default(),
        under: Layer::default(),
        marks,
        next_mark: 0,
        drawn_marks: 0,
        label: (MARGIN, MARGIN, 0),
        previous: None,
        open: None,
        open_count: 0,
        batch,
        strokes: 0,
        chars: 0,
        vertices: 0,
        speed,
        bench: Bench {
            targets,
            target: first,
            frames: Vec::new(),
            paints: Vec::new(),
            writing: Vec::new(),
            last: None,
            settle: 30,
            started: Instant::now(),
        },
    };
    gpui_kit::application().run(move |cx: &mut App| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(px(BOARD_WIDTH), px(800.)),
            ))),
            ..Default::default()
        };
        let board = Rc::new(RefCell::new(board));
        cx.open_window(options, |_, cx| cx.new(|_| View { board })).expect("open window");
    });
    Ok(())
}
