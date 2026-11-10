//! Importing a PDF, ported from src/board/importPdf.ts: one board per page, the page as a picture with
//! its text pinned where it is printed. A page with no text layer is just the picture, and the part of
//! it holding anything counts as one block. PDFium does the reading and rendering.

use crate::paper::{analyze_page, BlockKind, PaperArea, PaperState, TextPiece};
use crate::regions::{page_graph, utf16_len, Area};
use anyhow::{anyhow, bail, Context, Result};
use boardcast_board::graph::{parse_ops, Graph};
use boardcast_board::spans::rotate_point;
use image::codecs::jpeg::JpegEncoder;
use pdfium_render::prelude::*;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

/// A page's width on the board. The picture has twice that, to stay clear when zoomed.
const WIDTH: f64 = 900.0;
const PICTURE_WIDTH: i32 = 1800;
const JPEG_QUALITY: u8 = 82;
const HYPHEN_AT_LINE_END: char = '\u{2}';
/// The most boards made from one file, as each keeps a picture.
pub const MAX_PAGES: usize = 40;

static ONLY_REFERENCES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(References|Bibliography|参考文献)").expect("valid"));
static NOT_SECTIONS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(Abstract|摘要|References|Bibliography|参考文献)").expect("valid")
});
static FIRST_SECTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^§1(?-u:\b)").expect("valid"));
static NUMBERED_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^§(\S+)\s+(.*)$").expect("valid"));

pub struct NewPage {
    pub graph: Graph,
    pub jpeg: Vec<u8>,
}

pub struct PdfImport {
    pub pages: Vec<NewPage>,
    /// How many pages the file has.
    pub total: usize,
    /// Pages left over once the limit on boards was reached.
    pub unread: usize,
    /// Pages with no text layer.
    pub scanned: usize,
    /// It looks like a paper, so the first board has an outline of its sections.
    pub paper: bool,
    /// Pages of nothing but references, which get no board.
    pub refs_skipped: usize,
    /// The numbered references, kept though their pages get no board, so "[12]" can be looked up.
    pub references: Vec<(u32, String)>,
    pub title: Option<String>,
}

/// A page's text as pieces in board coordinates.
pub struct PageText {
    pub w: f64,
    pub h: f64,
    pub pieces: Vec<TextPiece>,
}

/// The library's file in its folder, whose name depends on the platform.
pub fn library_file(folder: &Path) -> PathBuf {
    Pdfium::pdfium_platform_library_name_at_path(folder)
}

fn bind(library: &Path) -> Result<Pdfium> {
    let path = Pdfium::pdfium_platform_library_name_at_path(library);
    let bindings = Pdfium::bind_to_library(&path).map_err(|e| {
        anyhow!(
            "PDF support could not be loaded from {}: {e:?}",
            path.display()
        )
    })?;
    Ok(Pdfium::new(bindings))
}

fn open<'a>(pdfium: &'a Pdfium, bytes: &'a [u8], english: bool) -> Result<PdfDocument<'a>> {
    pdfium
        .load_pdf_from_byte_slice(bytes, None)
        .map_err(|e| match e {
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
                anyhow!(if english {
                    "This PDF is password-protected; remove the password and import it again"
                } else {
                    "这个 PDF 有密码，先去掉密码再导入"
                })
            }
            PdfiumError::PdfiumLibraryInternalError(
                PdfiumInternalError::FormatError | PdfiumInternalError::FileError,
            ) => {
                anyhow!(if english {
                    "Can't open the PDF: the file is damaged or is not really a PDF"
                } else {
                    "PDF 打不开：文件损坏了，或者其实不是 PDF"
                })
            }
            other => anyhow!(
                "{}{other:?}",
                if english {
                    "Can't open the PDF: "
                } else {
                    "PDF 打不开："
                }
            ),
        })
}

/// A run of characters is one piece while it keeps going in one direction at one size.
struct Run {
    text: String,
    angle: f64,
    size: f64,
    /// Along the text's own direction.
    start: f64,
    end: f64,
    baseline: f64,
}

impl Run {
    fn finish(self, out: &mut Vec<TextPiece>) {
        if self.text.trim().is_empty() {
            return;
        }
        let angle = (self.angle != 0.0).then_some(self.angle);
        out.push(TextPiece {
            text: self.text,
            x: self.start,
            y: self.baseline - self.size,
            w: (self.end - self.start).max(0.1),
            h: self.size,
            angle,
        });
    }
}

/// Page coordinates have y going up. A rotated page turns those to how it is shown.
fn to_board(
    x: f64,
    y: f64,
    page_w: f64,
    page_h: f64,
    rotation: PdfPageRenderRotation,
    scale: f64,
) -> [f64; 2] {
    let (bx, by) = match rotation {
        PdfPageRenderRotation::None => (x, page_h - y),
        PdfPageRenderRotation::Degrees90 => (y, x),
        PdfPageRenderRotation::Degrees180 => (page_w - x, y),
        PdfPageRenderRotation::Degrees270 => (page_h - y, page_w - x),
    };
    [bx * scale, by * scale]
}

fn page_text(page: &PdfPage, scale: f64) -> Result<Vec<TextPiece>> {
    let rotation = page.rotation().unwrap_or(PdfPageRenderRotation::None);
    let turned = matches!(
        rotation,
        PdfPageRenderRotation::Degrees90 | PdfPageRenderRotation::Degrees270
    );
    let (shown_w, shown_h) = (
        f64::from(page.width().value),
        f64::from(page.height().value),
    );
    // Before turning, the page's own width and height are the other way around.
    let (page_w, page_h) = if turned {
        (shown_h, shown_w)
    } else {
        (shown_w, shown_h)
    };
    let turn = match rotation {
        PdfPageRenderRotation::None => 0.0,
        PdfPageRenderRotation::Degrees90 => std::f64::consts::FRAC_PI_2,
        PdfPageRenderRotation::Degrees180 => std::f64::consts::PI,
        PdfPageRenderRotation::Degrees270 => -std::f64::consts::FRAC_PI_2,
    };
    let text = page.text().map_err(|e| anyhow!("{e:?}"))?;
    let mut pieces = vec![];
    let mut run: Option<Run> = None;
    for ch in text.chars().iter() {
        let Some(c) = ch.unicode_char() else { continue };
        // PDFium reports a hyphen that breaks a word at a line's end as this control character. It has
        // to stay a hyphen, for the word to be joined up again across the lines.
        let c = if c == HYPHEN_AT_LINE_END { '-' } else { c };
        if c.is_control() {
            if let Some(done) = run.take() {
                done.finish(&mut pieces);
            }
            continue;
        }
        let (Ok(origin), Ok(bounds)) = (ch.origin(), ch.loose_bounds()) else {
            continue;
        };
        let size = f64::from(ch.scaled_font_size().value) * scale;
        if size <= 0.0 {
            continue;
        }
        // Stored to six places so equal directions compare equal.
        let own = f64::from(ch.angle_radians().unwrap_or(0.0));
        let angle = ((own + turn) * 1e6).round() / 1e6;
        let angle = if angle.abs() < 1e-6 { 0.0 } else { angle };
        let board = |x: PdfPoints, y: PdfPoints| {
            to_board(
                f64::from(x.value),
                f64::from(y.value),
                page_w,
                page_h,
                rotation,
                scale,
            )
        };
        let local = rotate_point(board(origin.0, origin.1), -angle);
        let far = [
            (bounds.left(), bounds.top()),
            (bounds.right(), bounds.top()),
            (bounds.left(), bounds.bottom()),
            (bounds.right(), bounds.bottom()),
        ]
        .iter()
        .map(|(x, y)| rotate_point(board(*x, *y), -angle)[0])
        .fold(local[0], f64::max);
        // A new direction or size, another line, or a wide gap such as a table cell starts a new piece.
        let carries = run.as_ref().is_some_and(|r| {
            r.angle == angle
                && (r.size - size).abs() < r.size * 0.05
                && (r.baseline - local[1]).abs() < size * 0.2
                && local[0] - r.end < size * 1.5
                && local[0] > r.start - size * 0.5
        });
        if !carries {
            if let Some(done) = run.take() {
                done.finish(&mut pieces);
            }
        }
        match &mut run {
            Some(r) => {
                r.text.push(c);
                r.end = r.end.max(far);
            }
            None => {
                run = Some(Run {
                    text: c.to_string(),
                    angle,
                    size,
                    start: local[0],
                    end: far,
                    baseline: local[1],
                })
            }
        }
    }
    if let Some(done) = run.take() {
        done.finish(&mut pieces);
    }
    Ok(pieces)
}

/// Every page's text, for checking the reading of a real file against the older engine.
pub fn read_text(bytes: &[u8], library: &Path) -> Result<Vec<PageText>> {
    let pdfium = bind(library)?;
    let document = open(&pdfium, bytes, true)?;
    let mut out = vec![];
    for page in document.pages().iter() {
        let scale = WIDTH / f64::from(page.width().value);
        out.push(PageText {
            w: WIDTH,
            h: f64::from(page.height().value) * scale,
            pieces: page_text(&page, scale)?,
        });
    }
    Ok(out)
}

/// The part of a page holding anything, in pixels. Looking at every few pixels is enough.
fn ink_box(rgba: &[u8], width: usize, height: usize) -> Option<[f64; 4]> {
    const STEP: usize = 4;
    let (mut x0, mut y0, mut x1, mut y1) = (width, height, None, None::<usize>);
    for y in (0..height).step_by(STEP) {
        for x in (0..width).step_by(STEP) {
            let p = &rgba[(y * width + x) * 4..][..4];
            if p[3] > 40 && p[0].min(p[1]).min(p[2]) < 230 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(Some(x));
                y1 = y1.max(Some(y));
            }
        }
    }
    Some([
        x0 as f64,
        y0 as f64,
        (x1? - x0 + STEP) as f64,
        (y1? - y0 + STEP) as f64,
    ])
}

fn utf16_prefix(s: &str, units: usize) -> String {
    let kept: Vec<u16> = s.encode_utf16().take(units).collect();
    String::from_utf16_lossy(&kept)
}

/// A heading as an item on the outline: the number goes last, as the board numbers its own items.
fn outline_item(heading: &str) -> String {
    let clean: String = heading
        .chars()
        .filter(|c| !matches!(c, '「' | '」' | '\n'))
        .collect();
    match NUMBERED_NAME.captures(&clean) {
        Some(m) => format!("{}（§{}）", utf16_prefix(&m[2], 28), &m[1]),
        None => utf16_prefix(&clean, 30),
    }
}

/// With no title in the file, the largest line on the first page, within reason.
fn title_from_page(pieces: &[TextPiece]) -> Option<String> {
    let flat: Vec<&TextPiece> = pieces.iter().filter(|p| p.angle.is_none()).collect();
    let big = flat.iter().map(|p| p.h).fold(f64::NEG_INFINITY, f64::max);
    let mut line: Vec<&&TextPiece> = flat.iter().filter(|p| p.h >= big * 0.95).collect();
    line.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let words: Vec<&str> = line.iter().map(|p| p.text.as_str()).collect();
    let text = words
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (4..=120).contains(&utf16_len(&text)).then_some(text)
}

pub fn import_pdf(
    bytes: &[u8],
    library: &Path,
    english: bool,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(usize, usize),
) -> Result<PdfImport> {
    let pdfium = bind(library)?;
    let document = open(&pdfium, bytes, english)?;
    let total = usize::from(document.pages().len());
    let mut title = document
        .metadata()
        .get(PdfDocumentMetadataTagType::Title)
        .map(|t| t.value().trim().to_string())
        .filter(|t| !t.is_empty());
    let mut pages: Vec<NewPage> = vec![];
    let (mut scanned, mut refs_skipped, mut captions, mut read) = (0, 0, 0, 0);
    let mut reference_text: Vec<String> = vec![];
    let mut state = PaperState::default();
    let mut headings: Vec<String> = vec![];
    for (i, page) in document.pages().iter().enumerate() {
        if pages.len() >= MAX_PAGES {
            break;
        }
        if cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        progress(i, total);
        read = i + 1;
        let scale = WIDTH / f64::from(page.width().value);
        let height = f64::from(page.height().value) * scale;
        let pieces = page_text(&page, scale)?;
        if i == 0 && title.is_none() {
            title = title_from_page(&pieces);
        }
        let was_in_refs = state.in_refs;
        let analysis = analyze_page(&pieces, WIDTH, height, &mut state);
        let mut areas: Vec<PaperArea> = analysis.areas;
        headings.extend(
            analysis
                .headings
                .into_iter()
                .filter(|h| !NOT_SECTIONS.is_match(h)),
        );
        captions += areas
            .iter()
            .filter(|a| matches!(a.kind, BlockKind::Figure | BlockKind::Table))
            .count();
        reference_text.extend(
            areas
                .iter()
                .filter(|a| a.kind == BlockKind::Reference)
                .map(|a| a.area.text.clone()),
        );
        // A page of nothing but references gets no board: nobody needs to turn past them.
        let refs_only = !areas.is_empty()
            && areas.iter().all(|a| {
                a.kind == BlockKind::Reference
                    || (a.kind == BlockKind::Heading && ONLY_REFERENCES.is_match(&a.area.text))
            });
        if refs_only && (was_in_refs || state.in_refs) && !pages.is_empty() {
            refs_skipped += 1;
            continue;
        }
        let number = i + 1;
        let unusable = || {
            if english {
                format!("Page {number} could not produce a valid image. Crop the page or adjust its dimensions, then import the PDF again.")
            } else {
                format!("第 {number} 页无法生成有效的图片。请裁剪页面或调整页面尺寸后重新导入。")
            }
        };
        let bitmap = page
            .render_with_config(
                &PdfRenderConfig::new()
                    .set_target_width(PICTURE_WIDTH)
                    .set_clear_color(PdfColor::WHITE),
            )
            .map_err(|_| anyhow!(unusable()))?;
        let (pw, ph) = (bitmap.width() as usize, bitmap.height() as usize);
        let rgba = bitmap.as_rgba_bytes();
        if pw == 0 || ph == 0 || rgba.len() < pw * ph * 4 {
            bail!(unusable());
        }
        // No text layer: where anything is drawn counts as one block, so it can still be pointed at.
        if areas.is_empty() {
            scanned += 1;
            if let Some(b) = ink_box(&rgba, pw, ph) {
                let k = WIDTH / pw as f64;
                let name = if english {
                    format!("Figure on page {number}")
                } else {
                    format!("第 {number} 页的图")
                };
                areas.push(PaperArea {
                    kind: BlockKind::Figure,
                    area: Area {
                        name: Some(name),
                        x: b[0] * k,
                        y: b[1] * k,
                        w: b[2] * k,
                        h: b[3] * k,
                        ..Area::default()
                    },
                });
            }
        }
        let rgb: Vec<u8> = rgba
            .chunks_exact(4)
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        let mut jpeg = vec![];
        JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY)
            .encode(&rgb, pw as u32, ph as u32, image::ExtendedColorType::Rgb8)
            .context("encoding the page")?;
        let plain: Vec<Area> = areas.into_iter().map(|a| a.area).collect();
        pages.push(NewPage {
            graph: page_graph(WIDTH, height, Some(number as u32), &plain),
            jpeg,
        });
    }
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    progress(total, total);
    // A paper gets an outline of its sections on the first board, to hang notes and questions on.
    let paper =
        headings.len() >= 2 && (captions > 0 || headings.iter().any(|h| FIRST_SECTION.is_match(h)));
    if paper {
        if let Some(first) = pages.first_mut() {
            let outline = if english {
                "Paper outline"
            } else {
                "论文结构"
            };
            let mut ops = vec![format!("T 「{outline}」")];
            ops.extend(
                headings
                    .iter()
                    .take(30)
                    .map(|h| format!("L 「{outline}」 「{}」", outline_item(h))),
            );
            first.graph.apply(&parse_ops(&ops.join("\n")));
        }
    }
    let references = numbered_references(&reference_text);
    if let Some(first) = pages.first_mut().filter(|_| !references.is_empty()) {
        first
            .graph
            .extra
            .insert("references".into(), serde_json::json!(references));
    }
    Ok(PdfImport {
        pages,
        total,
        unread: total - read,
        scanned,
        paper,
        refs_skipped,
        references,
        title: title.map(|t| utf16_prefix(&t, 80)),
    })
}

static ENTRY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)(?:\[(\d{1,3})\]|(\d{1,3})\.)\s+").expect("valid"));

/// Reference entries by their number: "[12] A. Author. Title." or "12. A. Author. Title.", split where
/// a block holds several. Author–year lists have no numbers and are left out.
fn numbered_references(blocks: &[String]) -> Vec<(u32, String)> {
    let text = blocks.join("\n");
    let starts: Vec<(usize, usize, u32)> = ENTRY
        .captures_iter(&text)
        .filter_map(|c| {
            let n = c.get(1).or(c.get(2))?;
            Some((c.get(0)?.start(), c.get(0)?.end(), n.as_str().parse().ok()?))
        })
        .collect();
    let mut out: Vec<(u32, String)> = vec![];
    let mut expected = 1;
    for (i, &(_, end, n)) in starts.iter().enumerate() {
        // Entries count up from one; a number out of step is a page number or a year's tail, not an entry.
        if n != expected {
            continue;
        }
        let stop = starts
            .iter()
            .skip(i + 1)
            .find(|s| s.2 == n + 1)
            .map_or(text.len(), |s| s.0);
        let entry: String = text[end..stop]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if !entry.is_empty() {
            out.push((n, entry.chars().take(240).collect()));
            expected += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn numbered_references_are_kept_by_number() {
        let blocks = vec![
            "[1] A. Vaswani et al. Attention is all you need. NeurIPS 2017.".to_string(),
            "[2] J. Devlin et al. BERT: pre-training of deep bidirectional transformers. 2019. [3] T. Brown et al. Language models are few-shot learners.".to_string(),
            "12 Page footer".to_string(),
        ];
        let refs = super::numbered_references(&blocks);
        assert_eq!(refs.iter().map(|r| r.0).collect::<Vec<_>>(), [1, 2, 3]);
        assert!(refs[1].1.starts_with("J. Devlin et al. BERT"));
        assert!(
            !refs[1].1.contains("Brown"),
            "an entry ends where the next begins"
        );
        let dotted = super::numbered_references(&[
            "1. Smith, A. Title one. 2. Jones, B. Title two.".to_string(),
        ]);
        assert_eq!(dotted.len(), 2);
        assert!(
            super::numbered_references(&["Smith, A. (2020). Title. Jones, B. (2021).".to_string()])
                .is_empty(),
            "author-year has no numbers"
        );
    }

    use super::*;
    use image::GenericImageView;

    fn library() -> Option<PathBuf> {
        let dir = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/pdfium"));
        let found = Pdfium::pdfium_platform_library_name_at_path(&dir).is_file();
        // CI fetches it, so there a missing library is a failure rather than a skipped test.
        assert!(
            found || std::env::var_os("CI").is_none(),
            "run `boardcast-tools fetch pdfium` first"
        );
        found.then_some(dir)
    }

    /// A small PDF of text lines, each given as x, y from the bottom, and the text.
    fn pdf(
        width: u32,
        height: u32,
        rotate: u32,
        title: Option<&str>,
        pages: &[Vec<(u32, u32, &str)>],
    ) -> Vec<u8> {
        let mut objects: Vec<String> = vec![];
        let kids: Vec<String> = (0..pages.len())
            .map(|i| format!("{} 0 R", 4 + i * 2))
            .collect();
        objects.push("<< /Type /Catalog /Pages 2 0 R >>".to_string());
        objects.push(format!(
            "<< /Type /Pages /Count {} /Kids [{}] >>",
            pages.len(),
            kids.join(" ")
        ));
        objects.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string());
        for (i, lines) in pages.iter().enumerate() {
            let content: String = lines
                .iter()
                .map(|(x, y, text)| format!("BT /F1 12 Tf {x} {y} Td ({text}) Tj ET\n"))
                .collect();
            objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] /Rotate {rotate} /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>", 5 + i * 2));
            objects.push(format!(
                "<< /Length {} >>\nstream\n{content}endstream",
                content.len()
            ));
        }
        let mut info = None;
        if let Some(title) = title {
            objects.push(format!("<< /Title ({title}) >>"));
            info = Some(objects.len());
        }
        let mut out = String::from("%PDF-1.4\n");
        let mut offsets = vec![];
        for (i, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out += &format!("{} 0 obj\n{object}\nendobj\n", i + 1);
        }
        let xref = out.len();
        out += &format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
        for offset in offsets {
            out += &format!("{offset:010} 00000 n \n");
        }
        let info = info.map_or(String::new(), |n| format!(" /Info {n} 0 R"));
        out += &format!(
            "trailer\n<< /Size {} /Root 1 0 R{info} >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        );
        out.into_bytes()
    }

    fn run(bytes: &[u8]) -> Result<PdfImport> {
        import_pdf(
            bytes,
            &library().unwrap(),
            true,
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
    }

    fn dark_inside(picture: &image::DynamicImage, area: [f64; 4]) -> u32 {
        // The picture has twice the board's pixels.
        let [x, y, w, h] = area.map(|v| (v * 2.0).max(0.0) as u32);
        let mut dark = 0;
        for py in y..(y + h).min(picture.height()) {
            for px in x..(x + w).min(picture.width()) {
                if picture.get_pixel(px, py).0[0] < 128 {
                    dark += 1;
                }
            }
        }
        dark
    }

    #[test]
    fn text_sits_exactly_on_the_picture_at_every_rotation() {
        if library().is_none() {
            return;
        }
        for rotate in [0, 90, 180, 270] {
            let lines = vec![(
                100,
                700,
                "Hello there, this line is near the top of the page.",
            )];
            let imported = run(&pdf(612, 792, rotate, None, &[lines])).unwrap();
            let page = &imported.pages[0];
            let block = page
                .graph
                .nodes
                .iter()
                .find(|n| n.kind == boardcast_board::graph::Kind::Region)
                .expect("a block of text");
            assert!(block.label.starts_with("Hello"), "{}", block.label);
            let picture = image::load_from_memory(&page.jpeg).unwrap();
            let at = block.at().unwrap();
            let rows = block.rows();
            // Rotated text has its rows in its own direction, and the block is the box around them.
            let inside = dark_inside(&picture, at);
            let total = dark_inside(&picture, [0.0, 0.0, 900.0, 2000.0]);
            assert!(total > 500, "the page drew something, {rotate}");
            assert!(f64::from(inside) > f64::from(total) * 0.85, "rotation {rotate}: {inside} of {total} dark pixels are inside {at:?}, rows {rows:?}");
        }
    }

    #[test]
    fn a_marked_word_lands_on_that_word() {
        if library().is_none() {
            return;
        }
        let imported = run(&pdf(
            612,
            792,
            0,
            None,
            &[vec![(
                100,
                700,
                "alpha beta gamma delta epsilon zeta eta theta",
            )]],
        ))
        .unwrap();
        let block = &imported.pages[0].graph.nodes[0];
        let span = boardcast_board::spans::locate(
            &block.rows(),
            "gamma",
            "圈出",
            boardcast_board::spans::Unit::Word,
        )
        .unwrap();
        let picture = image::load_from_memory(&imported.pages[0].jpeg).unwrap();
        let b = &span.boxes[0];
        assert!(dark_inside(&picture, [b.x as f64, b.y as f64, b.w as f64, b.h as f64]) > 50);
        // The gap before the word is left of its box, within the estimate's accuracy.
        assert!(b.x > 100 && b.w > 20 && b.w < 90, "{b:?}");
    }

    #[test]
    fn a_word_broken_across_lines_is_whole_again() {
        if library().is_none() {
            return;
        }
        let lines = vec![
            (72, 700, "This paragraph talks about the automa-"),
            (72, 686, "tion of many things that are done by hand."),
        ];
        let imported = run(&pdf(612, 792, 0, None, &[lines])).unwrap();
        let block = &imported.pages[0].graph.nodes[0];
        assert!(
            block.extra["text"]
                .as_str()
                .unwrap()
                .contains("automation of"),
            "{}",
            block.extra["text"]
        );
        assert!(boardcast_board::spans::locate(
            &block.rows(),
            "automation",
            "圈出",
            boardcast_board::spans::Unit::Word
        )
        .is_some());
    }

    #[test]
    fn a_page_of_a_document_becomes_a_board_with_its_size() {
        if library().is_none() {
            return;
        }
        let imported = run(&pdf(
            600,
            800,
            0,
            Some("Stored Title"),
            &[
                vec![(72, 700, "First page of the document")],
                vec![(72, 700, "Second page of the document")],
            ],
        ))
        .unwrap();
        assert_eq!(
            (
                imported.total,
                imported.pages.len(),
                imported.unread,
                imported.scanned
            ),
            (2, 2, 0, 0)
        );
        assert_eq!(imported.title.as_deref(), Some("Stored Title"));
        assert_eq!(imported.pages[1].graph.underlay(), Some((900.0, 1200.0)));
        assert!(!imported.paper);
        assert_eq!(&imported.pages[0].jpeg[..2], &[0xff, 0xd8]);
        assert_eq!(
            image::load_from_memory(&imported.pages[0].jpeg)
                .unwrap()
                .width(),
            1800
        );
    }

    #[test]
    fn without_a_stored_title_the_largest_line_is_used() {
        if library().is_none() {
            return;
        }
        let imported = run(&pdf(
            600,
            800,
            0,
            None,
            &[vec![(72, 700, "All one size here")]],
        ))
        .unwrap();
        assert_eq!(imported.title.as_deref(), Some("All one size here"));
    }

    #[test]
    fn a_blank_page_is_a_board_with_nothing_to_point_at() {
        if library().is_none() {
            return;
        }
        let imported = run(&pdf(600, 800, 0, None, &[vec![]])).unwrap();
        assert_eq!((imported.pages.len(), imported.scanned), (1, 1));
        assert!(imported.pages[0].graph.nodes.is_empty());
    }

    #[test]
    fn too_many_pages_stop_at_the_limit_and_say_so() {
        if library().is_none() {
            return;
        }
        let pages: Vec<Vec<(u32, u32, &str)>> = (0..MAX_PAGES + 3)
            .map(|_| vec![(72, 700, "A page of text")])
            .collect();
        let imported = run(&pdf(100, 100, 0, None, &pages)).unwrap();
        assert_eq!(
            (imported.total, imported.pages.len(), imported.unread),
            (MAX_PAGES + 3, MAX_PAGES, 3)
        );
    }

    #[test]
    fn a_paper_gets_an_outline_on_its_first_board() {
        if library().is_none() {
            return;
        }
        let lines = vec![
            (72, 700, "1. Introduction"),
            (72, 650, "2. Proposed Method"),
            (72, 600, "Figure 1. A picture of things."),
        ];
        let imported = run(&pdf(612, 792, 0, None, &[lines])).unwrap();
        assert!(imported.paper);
        let graph = &imported.pages[0].graph;
        assert!(graph.nodes.iter().any(|n| n.label == "Paper outline"));
        assert!(
            graph.nodes.iter().any(|n| n.label == "Introduction（§1）"),
            "{}",
            graph.describe()
        );
    }

    #[test]
    fn a_damaged_file_or_a_stop_says_so() {
        if library().is_none() {
            return;
        }
        let Err(error) = run(b"%PDF-1.4 this is not really one") else {
            panic!("should fail")
        };
        assert_eq!(
            format!("{error}"),
            "Can't open the PDF: the file is damaged or is not really a PDF"
        );
        let stop = AtomicBool::new(true);
        let result = import_pdf(
            &pdf(100, 100, 0, None, &[vec![]]),
            &library().unwrap(),
            true,
            &stop,
            &mut |_, _| {},
        );
        assert!(result.is_err());
    }

    #[test]
    fn a_missing_library_is_a_clear_error() {
        let Err(error) = import_pdf(
            b"",
            Path::new("/nonexistent"),
            true,
            &AtomicBool::new(false),
            &mut |_, _| {},
        ) else {
            panic!("should fail")
        };
        assert!(
            format!("{error}").contains("PDF support could not be loaded"),
            "{error}"
        );
    }
}
