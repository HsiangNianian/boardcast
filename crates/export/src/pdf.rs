//! A whole meeting as a PDF: one page per board in vector ink, then the summary and transcript as
//! real text. System fonts are embedded, cut down to the characters used.

use crate::ink::{Drawn, Page};
use crate::{Doc, Output};
use anyhow::{anyhow, Result};
use boardcast_board::compose::PAPER;
use fontdb::{Database, Family, Query, Weight};
use krilla::blend::BlendMode;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Size, Transform};
use krilla::image::Image;
use krilla::metadata::Metadata;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule};
use krilla::surface::Surface;
use krilla::text::{Font, TextDirection};
use krilla::Document;
use std::collections::HashMap;
use std::sync::Arc;

/// A4 landscape, in points.
const WIDTH: f32 = 841.89;
const HEIGHT: f32 = 595.28;
const MARGIN: f32 = 38.6;
const BOARD_TOP: f32 = 38.6;
const BOARD_BOTTOM: f32 = 17.5;
const BOARD_SIDE: f32 = 19.3;
const BOARD_PADDING: f64 = 40.0;
const TEXT_TOP: f32 = 45.6;
const HEADER_SIZE: f32 = 9.8;
const HEADER_Y: f32 = 28.0;
const BODY: f32 = 9.8;
const LINE_HEIGHT: f32 = 1.55;
const INK: u32 = 0x1e_1e_1e;
const MUTED: u32 = 0x6b_6b_6b;

/// Tried in order; a character none of them has is looked for in every other installed font.
const FAMILIES: [&str; 8] = ["PingFang SC", "Noto Sans CJK SC", "Source Han Sans SC", "Microsoft YaHei", "Noto Sans SC", "Source Han Sans CN", "WenQuanYi Micro Hei", "Hiragino Sans GB"];

struct Face {
    font: Font,
    data: Arc<Vec<u8>>,
    index: u32,
}

impl Face {
    /// How wide a character is for one unit of size, if the font has it.
    fn advance(&self, c: char) -> Option<f32> {
        let face = ttf_parser::Face::parse(&self.data, self.index).ok()?;
        let glyph = face.glyph_index(c).filter(|g| g.0 != 0)?;
        Some(f32::from(face.glyph_hor_advance(glyph)?) / f32::from(face.units_per_em()))
    }
}

struct Fonts {
    db: Database,
    faces: Vec<Face>,
    regular: Vec<usize>,
    bold: Vec<usize>,
    /// Character and weight, to which face has it and its width.
    known: HashMap<(char, bool), Option<(usize, f32)>>,
}

impl Fonts {
    fn system() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        let mut fonts = Self { db, faces: vec![], regular: vec![], bold: vec![], known: HashMap::new() };
        let names: Vec<Family> = FAMILIES.iter().map(|n| Family::Name(n)).chain([Family::SansSerif]).collect();
        for family in names {
            for (weight, bold) in [(Weight::NORMAL, false), (Weight::BOLD, true)] {
                let found = fonts.db.query(&Query { families: &[family], weight, ..Query::default() });
                if let Some(index) = found.and_then(|id| fonts.load(id)) {
                    let list = if bold { &mut fonts.bold } else { &mut fonts.regular };
                    if !list.contains(&index) {
                        list.push(index);
                    }
                }
            }
        }
        fonts
    }

    fn load(&mut self, id: fontdb::ID) -> Option<usize> {
        let (data, index) = self.db.with_face_data(id, |data, index| (Arc::new(data.to_vec()), index))?;
        if let Some(known) = self.faces.iter().position(|f| f.index == index && f.data == data) {
            return Some(known);
        }
        let font = Font::new(data.clone().into(), index)?;
        self.faces.push(Face { font, data, index });
        Some(self.faces.len() - 1)
    }

    fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }

    fn pick(&mut self, c: char, bold: bool) -> Option<(usize, f32)> {
        if let Some(known) = self.known.get(&(c, bold)) {
            return *known;
        }
        let order = if bold { self.bold.iter().chain(&self.regular) } else { self.regular.iter().chain(&self.bold) };
        let mut found = order.copied().find_map(|i| self.faces[i].advance(c).map(|a| (i, a)));
        if found.is_none() && !c.is_whitespace() {
            found = self.search_everything(c);
        }
        self.known.insert((c, bold), found);
        found
    }

    /// The slow way, once per unusual character. Colour emoji fonts are skipped, as they have no outlines.
    fn search_everything(&mut self, c: char) -> Option<(usize, f32)> {
        let ids: Vec<fontdb::ID> = self.db.faces().filter(|f| !f.post_script_name.contains("Color")).map(|f| f.id).collect();
        for id in ids {
            let has = self.db.with_face_data(id, |data, index| ttf_parser::Face::parse(data, index).ok().and_then(|f| f.glyph_index(c)).is_some_and(|g| g.0 != 0));
            if has == Some(true) {
                let index = self.load(id)?;
                return self.faces[index].advance(c).map(|a| (index, a));
            }
        }
        None
    }

    fn width(&mut self, text: &str, size: f32, bold: bool) -> f32 {
        text.chars().map(|c| self.pick(c, bold).map_or(0.5, |(_, a)| a) * size).sum()
    }
}

fn fill(rgb_value: u32, alpha: f32) -> Fill {
    let [_, r, g, b] = rgb_value.to_be_bytes();
    Fill { paint: rgb::Color::new(r, g, b).into(), opacity: NormalizedF32::new(alpha.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE), rule: FillRule::NonZero }
}

/// Draw a line of text in runs: a font at a time, and Latin apart from Chinese, which keeps shaping simple.
fn draw(surface: &mut Surface, fonts: &mut Fonts, text: &str, x: f32, y: f32, size: f32, bold: bool, color: u32) {
    surface.set_fill(Some(fill(color, 1.0)));
    let mut runs: Vec<(Option<usize>, bool, String, f32)> = vec![];
    for c in text.chars() {
        let picked = fonts.pick(c, bold);
        let (face, advance) = picked.map_or((None, 0.5), |(i, a)| (Some(i), a));
        match runs.last_mut() {
            // A space joins the run it is in, whatever the font says about it.
            Some(run) if (run.0 == face && run.1 == c.is_ascii()) || (c == ' ' && run.0.is_some()) => {
                run.2.push(c);
                run.3 += advance;
            }
            _ => runs.push((face, c.is_ascii(), c.to_string(), advance)),
        }
    }
    let mut at = x;
    for (face, _, run, advance) in runs {
        if let Some(face) = face {
            surface.draw_text(Point::from_xy(at, y), fonts.faces[face].font.clone(), size, &run, false, TextDirection::Auto);
        }
        at += advance * size;
    }
}

/// Fit text into a width, breaking Latin at spaces and anything else anywhere.
fn wrap(fonts: &mut Fonts, text: &str, max: f32, size: f32, bold: bool) -> Vec<String> {
    let mut rows = vec![];
    let mut row = String::new();
    let mut width = 0.0;
    for c in text.chars() {
        let advance = fonts.pick(c, bold).map_or(0.5, |(_, a)| a) * size;
        if !row.is_empty() && width + advance > max {
            // Take a whole word down with it, unless it is the only thing here.
            let word_start = if c.is_ascii_alphanumeric() { row.rfind(' ').map(|i| i + 1).filter(|i| row[*i..].chars().all(|c| c.is_ascii_graphic())) } else { None };
            let carried = word_start.map_or(String::new(), |i| row.split_off(i));
            rows.push(row.trim_end().to_string());
            width = fonts.width(&carried, size, bold);
            row = carried;
            if c == ' ' {
                continue;
            }
        }
        row.push(c);
        width += advance;
    }
    if !row.is_empty() || rows.is_empty() {
        rows.push(row);
    }
    rows
}

#[derive(Clone, Copy, PartialEq)]
enum Lead {
    Nothing,
    Bullet,
    Open,
    Done,
}

struct Row {
    text: String,
    size: f32,
    bold: bool,
    color: u32,
    gap: f32,
    indent: f32,
    lead: Lead,
}

impl Row {
    fn new(text: impl Into<String>, size: f32) -> Self {
        Self { text: text.into(), size, bold: false, color: INK, gap: 0.0, indent: 0.0, lead: Lead::Nothing }
    }
}

fn clean(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// The summary and transcript as rows of text.
fn rows_of(doc: &Doc) -> Vec<Row> {
    let (summary, transcript, note_prefix, colon) = if doc.english { ("Summary", "Transcript", "Edited: ", ": ") } else { ("摘要", "会议记录", "手动修改：", "：") };
    let board_name = |n: u64| if doc.english { format!("Board {n}") } else { format!("第 {n} 张板") };
    let mut out = vec![Row { bold: true, ..Row::new(clean(&doc.title), 18.2) }, Row { color: MUTED, gap: 2.8, ..Row::new(format!("{} · boardcast", doc.began), 9.1) }];
    if !doc.summary.trim().is_empty() {
        out.push(Row { bold: true, gap: 17.5, ..Row::new(summary, 13.3) });
        for raw in doc.summary.lines() {
            let line = raw.trim().replace("**", "");
            if line.is_empty() {
                continue;
            }
            if line.starts_with('#') {
                out.push(Row { bold: true, color: MUTED, gap: 7.7, ..Row::new(clean(line.trim_start_matches('#').trim()), 10.5) });
                continue;
            }
            let (lead, rest) = match line.get(..2) {
                Some("- " | "* " | "+ ") => {
                    let item = line[2..].trim_start();
                    match item.get(..3) {
                        Some("[ ]") => (Lead::Open, item[3..].trim_start()),
                        Some("[x]" | "[X]") => (Lead::Done, item[3..].trim_start()),
                        _ => (Lead::Bullet, item),
                    }
                }
                _ => (Lead::Nothing, line.as_str()),
            };
            out.push(Row { indent: 7.0, lead, ..Row::new(clean(rest), BODY) });
        }
    }
    if !doc.lines.is_empty() {
        out.push(Row { bold: true, gap: 17.5, ..Row::new(transcript, 13.3) });
        let many = doc.boards.len() > 1;
        let mut page = None;
        for l in &doc.lines {
            if many && page != Some(l.page) {
                page = Some(l.page);
                out.push(Row { color: MUTED, gap: 6.3, ..Row::new(board_name(l.page + 1), 9.1) });
            }
            if l.manual {
                out.push(Row { color: MUTED, indent: 7.0, ..Row::new(clean(&format!("{note_prefix}{}", l.text)), 9.1) });
            } else {
                let said = match &l.speaker {
                    Some(speaker) => format!("{speaker}{colon}{}", l.text),
                    None => l.text.clone(),
                };
                out.push(Row { indent: 7.0, ..Row::new(clean(&said), BODY) });
            }
        }
    }
    out
}

fn header(surface: &mut Surface, fonts: &mut Fonts, title: &str, right: &str) {
    if fonts.is_empty() {
        return;
    }
    let right_width = fonts.width(right, HEADER_SIZE, false);
    let room = WIDTH - 2.0 * MARGIN - 140.0;
    let mut shown = String::new();
    let mut width = 0.0;
    for c in clean(title).chars() {
        width += fonts.pick(c, false).map_or(0.5, |(_, a)| a) * HEADER_SIZE;
        if width > room {
            shown.push('…');
            break;
        }
        shown.push(c);
    }
    draw(surface, fonts, &shown, MARGIN, HEADER_Y, HEADER_SIZE, false, MUTED);
    draw(surface, fonts, right, WIDTH - MARGIN - right_width, HEADER_Y, HEADER_SIZE, false, MUTED);
}

fn paper(surface: &mut Surface) {
    let mut path = PathBuilder::new();
    path.move_to(0.0, 0.0);
    path.line_to(WIDTH, 0.0);
    path.line_to(WIDTH, HEIGHT);
    path.line_to(0.0, HEIGHT);
    path.close();
    if let Some(path) = path.finish() {
        surface.set_fill(Some(fill(PAPER, 1.0)));
        surface.draw_path(&path);
    }
}

/// JPEG and PNG go in as they are; anything else is converted first.
fn draw_page(surface: &mut Surface, page: &Page) {
    let data = |bytes: Vec<u8>| krilla::Data::from(Arc::new(bytes));
    let image = match page.image.mime.as_str() {
        "image/jpeg" => Image::from_jpeg(data(page.image.bytes.clone()), true),
        "image/png" => Image::from_png(data(page.image.bytes.clone()), true),
        _ => image::load_from_memory(&page.image.bytes).map_err(|e| e.to_string()).and_then(|picture| {
            let mut png = std::io::Cursor::new(vec![]);
            picture.write_to(&mut png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
            Image::from_png(data(png.into_inner()), true)
        }),
    };
    // A page that cannot be read costs the file its picture, not the marks on it.
    let Ok(image) = image else { return };
    if let Some(size) = Size::from_wh(page.w as f32, page.h as f32) {
        surface.draw_image(image, size);
    }
}

/// The board, as large as fits under the header and centred.
fn draw_board(surface: &mut Surface, board: &Drawn) {
    let (x, y, w, h) = board.frame(BOARD_PADDING);
    let (w_room, h_room) = (WIDTH - 2.0 * BOARD_SIDE, HEIGHT - BOARD_TOP - BOARD_BOTTOM);
    let k = (w_room / w as f32).min(h_room / h as f32);
    let left = (WIDTH - w as f32 * k) / 2.0;
    let top = BOARD_TOP + (h_room - h as f32 * k) / 2.0;
    surface.push_transform(&Transform::from_row(k, 0.0, 0.0, k, left - x as f32 * k, top - y as f32 * k));
    let paint = |surface: &mut Surface, under: bool| {
        for stroke in board.strokes.iter().filter(|s| s.style.under == under) {
            let Some((first, rest)) = stroke.outline.split_first() else { continue };
            let mut path = PathBuilder::new();
            path.move_to(first[0], first[1]);
            for p in rest {
                path.line_to(p[0], p[1]);
            }
            path.close();
            if let Some(path) = path.finish() {
                surface.set_fill(Some(fill(stroke.style.rgb, stroke.style.alpha)));
                surface.draw_path(&path);
            }
        }
    };
    if let Some(page) = &board.page {
        draw_page(surface, page);
    }
    surface.push_blend_mode(BlendMode::Multiply);
    paint(surface, true);
    surface.pop();
    paint(surface, false);
    surface.pop();
}

fn draw_lead(surface: &mut Surface, fonts: &mut Fonts, lead: Lead, x: f32, baseline: f32, size: f32) {
    match lead {
        Lead::Nothing => {}
        Lead::Bullet => draw(surface, fonts, "•", x, baseline, size, false, INK),
        Lead::Open | Lead::Done => {
            let side = size * 0.8;
            let top = baseline - side + size * 0.04;
            let mut outer = PathBuilder::new();
            outer.move_to(x, top);
            outer.line_to(x + side, top);
            outer.line_to(x + side, top + side);
            outer.line_to(x, top + side);
            outer.close();
            let line = size * 0.09;
            if lead == Lead::Open {
                // A square with a square hole.
                outer.move_to(x + line, top + line);
                outer.line_to(x + line, top + side - line);
                outer.line_to(x + side - line, top + side - line);
                outer.line_to(x + side - line, top + line);
                outer.close();
                if let Some(path) = outer.finish() {
                    surface.set_fill(Some(fill(MUTED, 1.0)));
                    surface.draw_path(&path);
                }
            } else {
                if let Some(path) = outer.finish() {
                    surface.set_fill(Some(fill(0x28_61_43, 1.0)));
                    surface.draw_path(&path);
                }
                let mut tick = PathBuilder::new();
                tick.move_to(x + side * 0.2, top + side * 0.5);
                tick.line_to(x + side * 0.42, top + side * 0.72);
                tick.line_to(x + side * 0.82, top + side * 0.26);
                tick.line_to(x + side * 0.72, top + side * 0.18);
                tick.line_to(x + side * 0.42, top + side * 0.52);
                tick.line_to(x + side * 0.3, top + side * 0.4);
                tick.close();
                if let Some(path) = tick.finish() {
                    surface.set_fill(Some(fill(0xff_ff_ff, 1.0)));
                    surface.draw_path(&path);
                }
            }
        }
    }
}

pub fn pdf(doc: &Doc, boards: &[Drawn]) -> Result<Output> {
    let mut fonts = Fonts::system();
    let mut document = Document::new();
    document.set_metadata(Metadata::new().title(doc.title.clone()).creator("boardcast".to_string()));
    let settings = || PageSettings::from_wh(WIDTH, HEIGHT).ok_or_else(|| anyhow!("bad page size"));
    let shown: Vec<(usize, &Drawn)> = boards.iter().enumerate().filter(|(_, b)| !b.strokes.is_empty() || b.page.is_some()).collect();
    for (index, board) in &shown {
        let mut page = document.start_page_with(settings()?);
        let mut surface = page.surface();
        paper(&mut surface);
        let label = match (boards.len() > 1, doc.english) {
            (true, true) => format!("Board {}", index + 1),
            (true, false) => format!("第 {} 张板", index + 1),
            (false, true) => "Board".to_string(),
            (false, false) => "板书".to_string(),
        };
        header(&mut surface, &mut fonts, &doc.title, &label);
        draw_board(&mut surface, board);
        surface.finish();
        page.finish();
    }

    let has_text = !doc.summary.trim().is_empty() || !doc.lines.is_empty();
    let text_left_out = has_text && fonts.is_empty();
    if has_text && !fonts.is_empty() {
        let rows = rows_of(doc);
        let continued = |n: u32| if doc.english { format!("Transcript {n}") } else { format!("会议记录 {n}") };
        let mut number = 1;
        let mut page = document.start_page_with(settings()?);
        let mut surface = page.surface();
        paper(&mut surface);
        let mut y = MARGIN;
        for row in rows {
            let room = WIDTH - 2.0 * MARGIN - row.indent - if row.lead == Lead::Nothing { 0.0 } else { row.size * 1.2 };
            let wrapped = wrap(&mut fonts, &row.text, room, row.size, row.bold);
            y += row.gap;
            for (i, text) in wrapped.iter().enumerate() {
                let height = row.size * LINE_HEIGHT;
                if y + height > HEIGHT - MARGIN {
                    surface.finish();
                    page.finish();
                    number += 1;
                    page = document.start_page_with(settings()?);
                    surface = page.surface();
                    paper(&mut surface);
                    header(&mut surface, &mut fonts, &doc.title, &continued(number));
                    y = TEXT_TOP;
                }
                let baseline = y + row.size * 1.15;
                let left = MARGIN + row.indent;
                if i == 0 {
                    draw_lead(&mut surface, &mut fonts, row.lead, left, baseline, row.size);
                }
                let text_x = left + if row.lead == Lead::Nothing { 0.0 } else { row.size * 1.2 };
                draw(&mut surface, &mut fonts, text, text_x, baseline, row.size, row.bold, row.color);
                y += height;
            }
        }
        surface.finish();
        page.finish();
    }
    let bytes = document.finish().map_err(|e| anyhow!("could not write the PDF: {e:?}"))?;
    Ok(Output { bytes, text_left_out })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ink::tests::board;
    use crate::Said;
    use boardcast_board::graph::Graph;

    fn doc(lines: usize) -> Doc {
        Doc {
            title: "评审".into(),
            exported: String::new(),
            began: "2026-01-02 03:04".into(),
            summary: "### 待办\n- [ ] 写接口\n- [x] 定方案\n- 普通".into(),
            boards: vec![Graph::default()],
            lines: (0..lines).map(|i| Said { speaker: Some("A".into()), text: format!("第 {i} 句 mixed with English"), page: 0, manual: i == 1 }).collect(),
            english: false,
            omits_visuals: false,
        }
    }

    #[test]
    fn a_meeting_becomes_a_pdf() {
        let out = pdf(&doc(3), &[board("E 「用户」 -> 「网关」")]).unwrap();
        assert!(out.bytes.starts_with(b"%PDF"));
    }

    #[test]
    fn nothing_to_print_is_still_a_valid_file() {
        let mut empty = doc(0);
        empty.summary.clear();
        let out = pdf(&empty, &[]).unwrap();
        assert!(out.bytes.starts_with(b"%PDF"));
        assert!(!out.text_left_out);
    }

    #[test]
    fn the_summary_is_read_as_headings_todos_and_bullets() {
        let rows = rows_of(&doc(0));
        let leads: Vec<Lead> = rows.iter().map(|r| r.lead).collect();
        assert!(leads.ends_with(&[Lead::Nothing, Lead::Open, Lead::Done, Lead::Bullet]));
        assert_eq!(rows[rows.len() - 4].text, "待办");
        assert_eq!(rows[rows.len() - 3].text, "写接口");
    }

    #[test]
    fn wrapping_never_loses_a_character_and_keeps_words_whole() {
        let mut fonts = Fonts::system();
        if fonts.is_empty() {
            return;
        }
        let rows = wrap(&mut fonts, "alpha beta gamma delta", 90.0, 10.0, false);
        assert!(rows.len() > 1, "{rows:?}");
        assert_eq!(rows.join(" "), "alpha beta gamma delta");
        let chinese = "这是一段很长的中文句子需要折行才能放得下";
        let rows = wrap(&mut fonts, chinese, 60.0, 10.0, false);
        assert_eq!(rows.concat(), chinese);
        assert_eq!(wrap(&mut fonts, "", 60.0, 10.0, false), [""]);
    }
}
