//! A board as filled shapes: the same layout and pen as on screen, written out as SVG or PNG.

use anyhow::{Context, Result};
use boardcast_board::board_layout::{Bounds, StableLayout};
use boardcast_board::compose::{compose, Piece, Style, PAPER};
use boardcast_board::glyphs::{Glyphs, Pt};
use boardcast_board::graph::Graph;
use boardcast_board::pen::{outline, Rng};
use boardcast_board::sugiyama::first_diagram;
use boardcast_meetings::PageImage;
use serde_json::Value;
use std::fmt::Write;
use tiny_skia::{BlendMode, Color, FillRule, FilterQuality, IntSize, Paint, PathBuilder, Pixmap, PixmapPaint, Transform};

/// Room around the ink in a picture.
const PADDING: f64 = 48.0;
const PNG_SCALE: f64 = 2.0;
const PNG_MAX_SIDE: f64 = 8000.0;

/// A board laid out, not yet drawn. Drawing a big board takes a while, so it can happen elsewhere.
pub struct Composed {
    pub(crate) pieces: Vec<Piece>,
    pub(crate) bounds: Bounds,
    /// Each region's id and the nodes in it, which settle when an unnamed region appeared.
    pub(crate) regions: Vec<(String, Vec<String>)>,
    pub(crate) page: Option<Page>,
}

/// An imported page under a board, with its size on the board.
#[derive(Clone)]
pub struct Page {
    pub image: PageImage,
    pub w: f64,
    pub h: f64,
}

pub struct Shape {
    pub outline: Vec<Pt>,
    pub style: Style,
}

pub struct Drawn {
    /// In the order they are painted: highlights first, so ink stays on top.
    pub strokes: Vec<Shape>,
    pub bounds: Bounds,
    pub page: Option<Page>,
}

/// Lay a board out from where its things already are, leaving the live layout alone.
pub fn compose_board(graph: &Graph, memory: &Value, image: Option<&PageImage>, glyphs: &mut Glyphs) -> Composed {
    let mut layout = StableLayout::default();
    layout.restore(memory);
    let laid = layout.layout(graph, &mut first_diagram);
    let (pieces, _) = compose(graph, &laid, glyphs);
    let regions = laid.regions.iter().map(|r| (r.rect.id.clone(), r.rows.iter().map(|row| row.id.clone()).collect())).collect();
    let page = image.zip(graph.underlay()).map(|(image, (w, h))| Page { image: image.clone(), w, h });
    Composed { pieces, bounds: laid.bounds, regions, page }
}

impl Composed {
    pub fn draw(self) -> Drawn {
        let mut strokes = vec![];
        for piece in &self.pieces {
            // The same seed in the same order gives the same wobble as on screen.
            let mut r = Rng::new(&piece.seed);
            for line in &piece.lines {
                let inked = line.inked(&mut r);
                strokes.push(Shape { outline: outline(&inked, 1.0), style: line.style });
            }
        }
        strokes.sort_by_key(|s| !s.style.under);
        Drawn { strokes, bounds: self.bounds, page: self.page }
    }
}

impl Drawn {
    /// The area a picture shows, as left, top, width and height.
    pub fn frame(&self, padding: f64) -> (f64, f64, f64, f64) {
        let b = &self.bounds;
        (b.x0 - padding, b.y0 - padding, b.x1 - b.x0 + 2.0 * padding, b.y1 - b.y0 + 2.0 * padding)
    }
}

/// XML cannot hold control characters, which a saved title might.
pub(crate) fn xml_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => out.push('\u{fffd}'),
            c => out.push(c),
        }
    }
    out
}

fn same_paint(a: Style, b: Style) -> bool {
    a.rgb == b.rgb && a.alpha == b.alpha && a.under == b.under
}

pub fn svg(board: &Drawn, title: &str) -> String {
    let (x, y, w, h) = board.frame(PADDING);
    let mut out = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x} {y} {w} {h}" width="{}" height="{}"><title>{}</title><rect x="{x}" y="{y}" width="{w}" height="{h}" fill="#{PAPER:06x}"/>"##,
        w.round(),
        h.round(),
        xml_text(title),
    );
    if let Some(page) = &board.page {
        let _ = write!(out, r#"<image href="{}" x="0" y="0" width="{}" height="{}" preserveAspectRatio="none"/>"#, page.image.data_url(), page.w, page.h);
    }
    let mut open: Option<Style> = None;
    for stroke in &board.strokes {
        if !open.is_some_and(|style| same_paint(style, stroke.style)) {
            if open.is_some() {
                out.push_str("</g>");
            }
            let style = stroke.style;
            let blend = if style.under { r#" style="mix-blend-mode:multiply""# } else { "" };
            let _ = write!(out, r##"<g fill="#{:06x}" opacity="{}"{blend}>"##, style.rgb, style.alpha);
            open = Some(style);
        }
        let Some((first, rest)) = stroke.outline.split_first() else { continue };
        let _ = write!(out, r#"<path d="M{:.2} {:.2}"#, first[0], first[1]);
        for p in rest {
            let _ = write!(out, "L{:.2} {:.2}", p[0], p[1]);
        }
        out.push_str(r#"Z"/>"#);
    }
    if open.is_some() {
        out.push_str("</g>");
    }
    out.push_str("</svg>");
    out
}

pub(crate) fn page_pixmap(image: &PageImage) -> Result<Pixmap> {
    let decoded = image::load_from_memory(&image.bytes).context("reading the page image")?.to_rgba8();
    let (w, h) = decoded.dimensions();
    let mut data = decoded.into_raw();
    // tiny-skia keeps its colours multiplied by their alpha.
    for p in data.chunks_exact_mut(4) {
        let a = u16::from(p[3]);
        for c in &mut p[..3] {
            *c = (u16::from(*c) * a / 255) as u8;
        }
    }
    Pixmap::from_vec(data, IntSize::from_wh(w, h).context("the page image has no size")?).context("the page image is too big")
}

pub fn png(board: &Drawn) -> Result<Vec<u8>> {
    let (x, y, w, h) = board.frame(PADDING);
    let scale = PNG_SCALE.min(PNG_MAX_SIDE / w.max(h));
    let mut canvas = Pixmap::new((w * scale).round().max(1.0) as u32, (h * scale).round().max(1.0) as u32).context("the picture is too big to make")?;
    let [_, r, g, b] = PAPER.to_be_bytes();
    canvas.fill(Color::from_rgba8(r, g, b, 255));
    let at = Transform::from_translate(-x as f32, -y as f32).post_scale(scale as f32, scale as f32);
    if let Some(page) = &board.page {
        let picture = page_pixmap(&page.image)?;
        let fit = Transform::from_scale((page.w / f64::from(picture.width())) as f32, (page.h / f64::from(picture.height())) as f32).post_concat(at);
        canvas.draw_pixmap(0, 0, picture.as_ref(), &PixmapPaint { opacity: 1.0, blend_mode: BlendMode::SourceOver, quality: FilterQuality::Bilinear }, fit, None);
    }
    for stroke in &board.strokes {
        fill_stroke(&mut canvas, &stroke.outline, stroke.style, at);
    }
    canvas.encode_png().context("encoding the picture")
}

/// One stroke's outline, filled in its color; a highlight multiplies, as a marker does over ink.
pub(crate) fn fill_stroke(canvas: &mut Pixmap, outline: &[Pt], style: Style, at: Transform) {
    let Some((first, rest)) = outline.split_first() else { return };
    let mut path = PathBuilder::new();
    path.move_to(first[0], first[1]);
    for p in rest {
        path.line_to(p[0], p[1]);
    }
    path.close();
    let Some(path) = path.finish() else { return };
    let [_, r, g, b] = style.rgb.to_be_bytes();
    let mut paint = Paint::default();
    paint.set_color_rgba8(r, g, b, (style.alpha * 255.0) as u8);
    paint.anti_alias = true;
    if style.under {
        paint.blend_mode = BlendMode::Multiply;
    }
    canvas.fill_path(&path, &paint, FillRule::Winding, at, None);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use boardcast_board::graph::parse_ops;

    pub(crate) fn board(ops: &str) -> Drawn {
        let mut g = Graph::default();
        g.apply(&parse_ops(ops));
        let mut glyphs = Glyphs::from_checkout(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))).unwrap();
        compose_board(&g, &Value::Null, None, &mut glyphs).draw()
    }

    #[test]
    fn drawing_the_same_board_twice_gives_the_same_ink() {
        let (a, b) = (board("E 「用户」 -> 「网关」"), board("E 「用户」 -> 「网关」"));
        assert!(!a.strokes.is_empty());
        assert_eq!(a.strokes.len(), b.strokes.len());
        assert_eq!(a.strokes[0].outline, b.strokes[0].outline);
    }

    #[test]
    fn highlights_are_painted_before_the_ink() {
        let drawn = board("T 「话题」\nL 「话题」 「条目」\nM 「条目」 高亮");
        let first_ink = drawn.strokes.iter().position(|s| !s.style.under).unwrap();
        assert!(drawn.strokes[first_ink..].iter().all(|s| !s.style.under));
    }

    #[test]
    fn an_svg_has_its_title_escaped_and_every_group_closed() {
        let out = svg(&board("E 「用户」 -> 「网关」"), "a<b>&c\u{1}");
        assert!(out.contains("<title>a&lt;b&gt;&amp;c\u{fffd}</title>"));
        assert_eq!(out.matches("<g ").count(), out.matches("</g>").count());
        assert!(out.ends_with("</svg>"));
    }

    #[test]
    fn a_png_has_room_around_the_board_and_a_size_limit() {
        let drawn = board("E 「用户」 -> 「网关」");
        let bytes = png(&drawn).unwrap();
        assert_eq!(&bytes[1..4], b"PNG");
        let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
        let (_, _, w, _) = drawn.frame(PADDING);
        assert_eq!(width, (w * 2.0).round() as u32);
        let huge = Drawn { strokes: vec![], bounds: Bounds { x0: 0.0, y0: 0.0, x1: 20000.0, y1: 10.0 }, page: None };
        let bytes = png(&huge).unwrap();
        assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 8000);
    }
}
