//! Importing an image, ported from src/board/importImage.ts and segment.ts: one board with the picture
//! under it. Ordinary image processing finds where the blocks of content are, since a small vision
//! model reads text well but places its boxes badly, and the model then reads them one at a time.

use crate::pdf::NewPage;
use crate::regions::{js_round, page_graph, Area};
use anyhow::{anyhow, bail, Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::{crop_imm, resize, FilterType};
use image::{ImageEncoder, Rgba, RgbaImage};
use regex::Regex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

/// An image's width on the board, the same as a PDF page.
const WIDTH: f64 = 900.0;
/// The most resolution worth looking for blocks at: more is slow, and less runs fine text together.
const WORK: u32 = 1800;
/// Each block is one question for the model.
pub const MAX_AREAS: usize = 60;
const JPEG_QUALITY: u8 = 85;
/// The side of the cells that ink is coarsened to, in pixels.
const CELL: usize = 3;

pub struct ImageImport {
    pub page: NewPage,
    /// How many blocks were found, and how many of those held text.
    pub found: usize,
    pub read: usize,
    /// There were too many blocks, so only the first were read.
    pub capped: bool,
}

/// Tidy what the model said: no thinking, quotes or fences, and its ways of saying "nothing here" are empty.
pub fn clean_read(text: &str) -> String {
    static THINKING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<think>[\s\S]*?(</think>|$)").expect("valid"));
    static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)```[a-z]*\n?").expect("valid"));
    static OPENING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^[「“"'『]+"#).expect("valid"));
    static CLOSING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"[」”"'』]+$"#).expect("valid"));
    static NOTHING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(无|没有|空|没有文字|图里没有字|（无）|\(无\)|none|n/a)[。.]?$").expect("valid"));
    let t = THINKING.replace_all(text, "");
    let t = FENCE.replace_all(&t, "");
    let t = OPENING.replace(&t, "");
    let t = CLOSING.replace(&t, "");
    // Some models read the text and then add a line saying "nothing", so each line is judged alone.
    t.lines().filter(|line| !NOTHING.is_match(line.trim())).flat_map(str::split_whitespace).collect::<Vec<_>>().join(" ")
}

/// Whether it reads as text: arrowheads often come back as "1>1" or "->".
pub fn meaningful(text: &str) -> bool {
    text.chars().any(char::is_alphabetic) || text.chars().filter(char::is_ascii_digit).count() >= 3
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Find the blocks of content in an image, by its pixels alone.
///
/// The commonest brightness is the background and whatever differs from it is ink, which suits
/// whiteboards, blackboards and screenshots. Long straight lines are removed so they cannot tie the
/// image into one piece. Ink is coarsened to cells with the gaps between characters filled, so each
/// connected patch is a line of text, and those lines then join into paragraphs.
pub fn segment(width: usize, height: usize, rgba: &[u8]) -> Vec<Rect> {
    let (w, h) = (width, height);
    let mut lum = vec![0_u8; w * h];
    let mut hist = [0_u32; 256];
    for i in 0..w * h {
        let p = &rgba[i * 4..i * 4 + 4];
        let a = f64::from(p[3]) / 255.0;
        // Where it is transparent counts as white.
        let v = js_round((0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2])) * a + 255.0 * (1.0 - a));
        lum[i] = v as u8;
        hist[usize::from(lum[i])] += 1;
    }
    let mut bg = 0;
    for v in 1..256 {
        if hist[v] > hist[bg] {
            bg = v;
        }
    }
    let mut ink: Vec<bool> = lum.iter().map(|v| (i32::from(*v) - bg as i32).abs() > 48).collect();

    // A hand-drawn line wobbles, so it counts as joined when within a couple of pixels either way.
    let long = 40.max(js_round(w.min(h) as f64 * 0.08) as usize);
    const WOBBLE: i64 = 2;
    let near = |ink: &[bool], x: usize, y: usize, dx: i64, dy: i64| {
        (-WOBBLE..=WOBBLE).any(|k| {
            let (xx, yy) = (x as i64 + dx * k, y as i64 + dy * k);
            xx >= 0 && xx < w as i64 && yy >= 0 && yy < h as i64 && ink[yy as usize * w + xx as usize]
        })
    };
    let mut line = vec![false; w * h];
    for y in 0..h {
        let mut start = None;
        for x in 0..=w {
            let on = x < w && near(&ink, x, y, 0, 1);
            match (on, start) {
                (true, None) => start = Some(x),
                (false, Some(s)) => {
                    if x - s >= long {
                        line[y * w + s..y * w + x].fill(true);
                    }
                    start = None;
                }
                _ => {}
            }
        }
    }
    for x in 0..w {
        let mut start = None;
        for y in 0..=h {
            let on = y < h && near(&ink, x, y, 1, 0);
            match (on, start) {
                (true, None) => start = Some(y),
                (false, Some(s)) => {
                    if y - s >= long {
                        for k in s..y {
                            line[k * w + x] = true;
                        }
                    }
                    start = None;
                }
                _ => {}
            }
        }
    }
    // A line of text can also run on across its width, so only a thin band counts as a line.
    let thin = |ink: &[bool], i: usize, dx: i64, dy: i64| {
        let (x, y) = ((i % w) as i64, (i / w) as i64);
        let n = (-(WOBBLE + 4)..=WOBBLE + 4)
            .filter(|k| {
                let (xx, yy) = (x + dx * k, y + dy * k);
                xx >= 0 && xx < w as i64 && yy >= 0 && yy < h as i64 && ink[yy as usize * w + xx as usize]
            })
            .count() as i64;
        n <= WOBBLE * 2 + 3
    };
    let erase: Vec<usize> = (0..w * h).filter(|i| line[*i] && ink[*i] && (thin(&ink, *i, 0, 1) || thin(&ink, *i, 1, 0))).collect();
    for i in erase {
        ink[i] = false;
    }

    let (gw, gh) = (w.div_ceil(CELL), h.div_ceil(CELL));
    let mut grid = vec![false; gw * gh];
    for y in 0..h {
        for x in 0..w {
            if ink[y * w + x] {
                grid[y / CELL * gw + x / CELL] = true;
            }
        }
    }
    // Gaps between characters are usually under half a character wide.
    const GAP: i64 = 4;
    let mut filled = grid.clone();
    for y in 0..gh {
        let mut last: Option<i64> = None;
        for x in 0..gw as i64 {
            if !grid[y * gw + x as usize] {
                continue;
            }
            if let Some(l) = last {
                if x - l > 1 && x - l <= GAP + 1 {
                    filled[y * gw + l as usize + 1..y * gw + x as usize].fill(true);
                }
            }
            last = Some(x);
        }
    }

    let mut seen = vec![false; gw * gh];
    let mut lines: Vec<Rect> = vec![];
    let mut stack = vec![];
    for s in 0..gw * gh {
        if !filled[s] || seen[s] {
            continue;
        }
        let (mut x0, mut y0, mut x1, mut y1, mut n) = (gw, gh, 0, 0, 0);
        stack.push(s);
        seen[s] = true;
        while let Some(c) = stack.pop() {
            let (cx, cy) = (c % gw, c / gw);
            x0 = x0.min(cx);
            x1 = x1.max(cx);
            y0 = y0.min(cy);
            y1 = y1.max(cy);
            if grid[c] {
                n += 1;
            }
            let mut visit = |d: usize| {
                if filled[d] && !seen[d] {
                    seen[d] = true;
                    stack.push(d);
                }
            };
            if cx > 0 {
                visit(c - 1);
            }
            if cx + 1 < gw {
                visit(c + 1);
            }
            if cy > 0 {
                visit(c - gw);
            }
            if cy + 1 < gh {
                visit(c + gw);
            }
        }
        let (lx, ly) = (x0 * CELL, y0 * CELL);
        let (lw, lh) = ((x1 - x0 + 1) * CELL, (y1 - y0 + 1) * CELL);
        // Too small or thin is noise, a speck of punctuation, or the stub of a line.
        if lh >= CELL * 2 && lw > CELL * 2 && n >= 3 {
            lines.push(Rect { x: lx as f64, y: ly as f64, w: lw.min(w - lx) as f64, h: lh.min(h - ly) as f64 });
        }
    }
    let out = blocks(lines);
    // A block wholly inside another is one stroke of a character, cut off on its own.
    let within = |a: &Rect, b: &Rect| a.x >= b.x - 2.0 && a.y >= b.y - 2.0 && a.x + a.w <= b.x + b.w + 2.0 && a.y + a.h <= b.y + b.h + 2.0;
    out.iter().enumerate().filter(|(i, a)| !out.iter().enumerate().any(|(j, b)| j != *i && within(a, b))).map(|(_, a)| *a).collect()
}

/// Join lines side by side, then lines stacked closely, overlapping and of similar size, into paragraphs.
fn blocks(pieces: Vec<Rect>) -> Vec<Rect> {
    let mut ps: Vec<Rect> = pieces.into_iter().filter(|p| p.w > 0.0 && p.h > 0.0).collect();
    ps.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut lines: Vec<Rect> = vec![];
    for p in ps {
        let hit = lines.iter_mut().find(|l| (l.y + l.h / 2.0 - (p.y + p.h / 2.0)).abs() < l.h.min(p.h) * 0.5 && p.x >= l.x - 1.0 && p.x - (l.x + l.w) < l.h.max(p.h) * 1.5);
        match hit {
            Some(l) => {
                let x1 = (l.x + l.w).max(p.x + p.w);
                let y0 = l.y.min(p.y);
                l.h = (l.y + l.h).max(p.y + p.h) - y0;
                l.y = y0;
                l.w = x1 - l.x;
            }
            None => lines.push(p),
        }
    }
    lines.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut out: Vec<(Rect, f64)> = vec![];
    for l in lines {
        let hit = out.iter_mut().find(|(b, lh)| {
            l.y - (b.y + b.h) < lh.max(l.h) * 0.9 && l.y >= b.y + b.h - *lh * 0.5 && l.x < b.x + b.w && l.x + l.w > b.x && lh.max(l.h) / lh.min(l.h) < 1.3
        });
        match hit {
            Some((b, _)) => {
                let x0 = b.x.min(l.x);
                b.w = (b.x + b.w).max(l.x + l.w) - x0;
                b.x = x0;
                b.h = l.y + l.h - b.y;
            }
            None => out.push((l, l.h)),
        }
    }
    out.into_iter().map(|(b, _)| b).collect()
}

pub fn import_image(bytes: &[u8], english: bool, cancel: &AtomicBool, read: &mut dyn FnMut(&[u8]) -> Result<String>, progress: &mut dyn FnMut(usize, usize)) -> Result<ImageImport> {
    let broken = || if english { "Can't open the image (PNG, JPEG, WebP and GIF are supported)" } else { "图片打不开（支持 PNG、JPEG、WebP、GIF）" };
    let picture = image::load_from_memory(bytes).map_err(|_| anyhow!(broken()))?.to_rgba8();
    if picture.width() == 0 || picture.height() == 0 {
        bail!(broken());
    }
    let k = (f64::from(WORK) / f64::from(picture.width())).min(1.0);
    let (w, h) = (js_round(f64::from(picture.width()) * k).max(1.0) as u32, js_round(f64::from(picture.height()) * k).max(1.0) as u32);
    let sized = if k < 1.0 { resize(&picture, w, h, FilterType::Triangle) } else { picture };
    // Transparent parts get a white ground, as the picture on the board will show them.
    let mut work = RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]));
    image::imageops::overlay(&mut work, &sized, 0, 0);

    let all = segment(w as usize, h as usize, work.as_raw());
    let capped = all.len() > MAX_AREAS;
    let chosen = &all[..all.len().min(MAX_AREAS)];
    let to_board = WIDTH / f64::from(w);
    let mut found = vec![];
    for (i, a) in chosen.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        progress(i, chosen.len());
        // A little room around it, with small text enlarged, as the model then reads it better.
        const PAD: f64 = 6.0;
        let x = (a.x - PAD).max(0.0);
        let y = (a.y - PAD).max(0.0);
        let cw = (f64::from(w) - x).min(a.w + PAD * 2.0);
        let ch = (f64::from(h) - y).min(a.h + PAD * 2.0);
        let up = (48.0 / ch).clamp(1.0, 3.0);
        let piece = crop_imm(&work, x as u32, y as u32, cw as u32, ch as u32).to_image();
        let piece = resize(&piece, js_round(cw * up).max(1.0) as u32, js_round(ch * up).max(1.0) as u32, FilterType::Triangle);
        let mut png = vec![];
        PngEncoder::new(&mut png).write_image(piece.as_raw(), piece.width(), piece.height(), image::ExtendedColorType::Rgba8).context("encoding part of the image")?;
        let text = clean_read(&read(&png)?);
        if cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        if meaningful(&text) {
            found.push(Area { text, name: None, x: a.x * to_board, y: a.y * to_board, w: a.w * to_board, h: a.h * to_board, rows: None });
        }
    }
    progress(chosen.len(), chosen.len());
    let rgb: Vec<u8> = work.as_raw().chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
    let mut jpeg = vec![];
    JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY).encode(&rgb, w, h, image::ExtendedColorType::Rgb8).context("encoding the image")?;
    let graph = page_graph(WIDTH, f64::from(h) * to_board, None, &found);
    Ok(ImageImport { page: NewPage { graph, jpeg }, found: chosen.len(), read: found.len(), capped })
}

#[cfg(test)]
mod tests {
    use super::*;
    use boardcast_board::graph::Kind;
    use serde_json::Value;

    fn goldens() -> Value {
        serde_json::from_str(include_str!("../../board/fixtures/goldens.json")).unwrap()
    }

    fn canvas(width: usize, height: usize, paper: u8, ink: u8, rects: &[[usize; 4]]) -> Vec<u8> {
        let mut data = vec![255_u8; width * height * 4];
        for p in data.chunks_exact_mut(4) {
            p[0] = paper;
            p[1] = paper;
            p[2] = paper;
        }
        for [x0, y0, w, h] in rects {
            for y in *y0..y0 + h {
                for x in *x0..x0 + w {
                    data[(y * width + x) * 4..(y * width + x) * 4 + 3].fill(ink);
                }
            }
        }
        data
    }

    #[test]
    fn blocks_are_found_as_the_typescript_engine_finds_them() {
        let g = goldens();
        let cases = g["segments"].as_array().unwrap();
        assert!(cases.len() >= 6);
        for case in cases {
            let (w, h) = (case["w"].as_u64().unwrap() as usize, case["h"].as_u64().unwrap() as usize);
            let rects: Vec<[usize; 4]> = serde_json::from_value(case["rects"].clone()).unwrap();
            let mut data = canvas(w, h, case["paper"].as_u64().unwrap() as u8, case["ink"].as_u64().unwrap() as u8, &rects);
            if case["clear"].as_bool().unwrap_or(false) {
                for p in data.chunks_exact_mut(4) {
                    if p[0] == 255 {
                        p[3] = 0;
                    }
                }
            }
            let got: Vec<[f64; 4]> = segment(w, h, &data).iter().map(|r| [r.x, r.y, r.w, r.h]).collect();
            let wanted: Vec<[f64; 4]> = serde_json::from_value(case["blocks"].clone()).unwrap();
            assert_eq!(got, wanted, "{}", case["name"]);
        }
    }

    #[test]
    fn what_the_model_says_is_read_as_the_typescript_engine_reads_it() {
        let g = goldens();
        for case in g["reads"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            assert_eq!(clean_read(input), case["clean"].as_str().unwrap(), "{input}");
            assert_eq!(meaningful(input), case["meaningful"].as_bool().unwrap(), "{input}");
        }
    }

    #[test]
    fn a_line_saying_nothing_is_dropped_from_what_was_read() {
        assert_eq!(clean_read("感谢信\n无"), "感谢信");
        assert_eq!(clean_read("无\n没有"), "");
        assert_eq!(clean_read("无论如何\n写库"), "无论如何 写库");
    }

    fn png_with_blocks(rects: &[[usize; 4]], width: u32, height: u32) -> Vec<u8> {
        let data = canvas(width as usize, height as usize, 255, 20, rects);
        let mut png = vec![];
        PngEncoder::new(&mut png).write_image(&data, width, height, image::ExtendedColorType::Rgba8).unwrap();
        png
    }

    fn word(x: usize, y: usize) -> Vec<[usize; 4]> {
        (0..6).map(|i| [x + i * 18, y, 14, 14]).collect()
    }

    #[test]
    fn each_block_is_read_and_only_text_is_kept() {
        let mut rects = word(20, 20);
        rects.extend(word(20, 200));
        let mut answers = vec!["订单服务".to_string(), "->".to_string()].into_iter();
        let mut asked = vec![];
        let imported = import_image(&png_with_blocks(&rects, 400, 300), false, &AtomicBool::new(false), &mut |png| {
            asked.push(png.to_vec());
            Ok(answers.next().unwrap())
        }, &mut |_, _| {})
        .unwrap();
        assert_eq!((imported.found, imported.read, imported.capped), (2, 1, false));
        let block = &imported.page.graph.nodes[0];
        assert_eq!((block.label.as_str(), block.kind), ("订单服务", Kind::Region));
        // Board coordinates: this picture is shown at 900 wide.
        let at = block.at().unwrap();
        assert!((at[0] - 20.0 * 2.25).abs() < 8.0 && (at[1] - 20.0 * 2.25).abs() < 8.0, "{at:?}");
        assert_eq!(imported.page.graph.underlay(), Some((900.0, 675.0)));
        // Small text is enlarged for the model.
        let piece = image::load_from_memory(&asked[0]).unwrap();
        assert!(piece.height() > 24, "{}", piece.height());
    }

    #[test]
    fn too_many_blocks_stop_at_the_limit_and_say_so() {
        let rects: Vec<[usize; 4]> = (0..MAX_AREAS + 5).flat_map(|i| word(20 + (i % 3) * 200, 20 + (i / 3) * 40)).collect();
        let mut asked = 0;
        let imported = import_image(&png_with_blocks(&rects, 800, 1000), true, &AtomicBool::new(false), &mut |_| {
            asked += 1;
            Ok("text".to_string())
        }, &mut |_, _| {})
        .unwrap();
        assert!(imported.capped);
        assert_eq!(asked, MAX_AREAS);
    }

    #[test]
    fn a_large_image_is_scaled_and_a_damaged_one_says_so_and_a_stop_stops() {
        let big = import_image(&png_with_blocks(&[], 3600, 200), true, &AtomicBool::new(false), &mut |_| Ok(String::new()), &mut |_, _| {}).unwrap();
        assert_eq!(image::load_from_memory(&big.page.jpeg).unwrap().width(), WORK);
        assert!(big.page.graph.nodes.is_empty(), "a picture with no text is still a page");
        let Err(error) = import_image(b"not an image", true, &AtomicBool::new(false), &mut |_| Ok(String::new()), &mut |_, _| {}) else { panic!("should fail") };
        assert!(format!("{error}").starts_with("Can't open the image"));
        let stop = AtomicBool::new(true);
        let mut asked = 0;
        assert!(import_image(&png_with_blocks(&word(20, 20), 400, 300), true, &stop, &mut |_| { asked += 1; Ok(String::new()) }, &mut |_, _| {}).is_err());
        assert_eq!(asked, 0);
    }
}
