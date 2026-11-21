//! What shows from the moment the window opens: the name, written by the same pen as the board.
//! Nothing in the toolkit writes by hand, so this is drawn here. The caller swaps it for the app.

use crate::i18n::t;
use crate::theme::pal;
use alcyo_board::glyphs::{Glyphs, Pt};
use alcyo_board::pen::{ink, line, outline, place, Inked, Rng};
use gpui_kit::{canvas, div, point, prelude::*, px, rgb, Background, Bounds, FillOptions, Hsla, IntoElement, PathBuilder, PathStyle, Pixels, Window};
use std::time::{Duration, Instant};

const WORD: &str = "Alcyo";
const SIZE: f32 = 56.0;
const INK: u32 = 0x1e_1e_1e;
/// As long as the name takes to write, so it never flashes past half written.
pub const WRITING: Duration = Duration::from_millis(900);
const SAY_MORE_AFTER: Duration = Duration::from_secs(2);

/// The pen's strokes for the name, around (0, 0).
pub fn word_strokes(glyphs: &mut Glyphs) -> Vec<Inked> {
    let mut r = Rng::new(WORD);
    let found: Vec<_> = WORD.chars().filter_map(|c| glyphs.get(c)).collect();
    let width = found.iter().map(|g| g.advance).sum::<f32>() * SIZE;
    let (mut x, y) = (-width / 2.0, -SIZE / 2.0);
    let mut strokes = vec![];
    for glyph in found {
        for stroke in place(&glyph.strokes, x, y, SIZE, &mut r, 0.0) {
            strokes.push(ink(&stroke, SIZE * 0.09, SIZE * 0.012, &mut r));
        }
        x += glyph.advance * SIZE;
    }
    let under: Vec<Pt> = line([-width / 2.0 - 6.0, SIZE * 0.72], [width / 2.0 + 6.0, SIZE * 0.72], &mut r, 0.008);
    strokes.push(ink(&under, SIZE * 0.07, SIZE * 0.012, &mut r));
    strokes
}

/// A line of text in the board's pen, `size` tall, from (0, 0) at its top left: its strokes, and how wide and
/// tall it is. A character the pen has no letter for leaves a space.
pub fn written_line(glyphs: &mut Glyphs, text: &str, size: f32) -> (Vec<Inked>, f32, f32) {
    let mut r = Rng::new(text);
    let mut x = 0.0;
    let mut strokes = vec![];
    for c in text.chars() {
        match glyphs.get(c) {
            Some(glyph) => {
                for stroke in place(&glyph.strokes, x, 0.0, size, &mut r, 0.0) {
                    strokes.push(ink(&stroke, size * 0.085, size * 0.012, &mut r));
                }
                x += glyph.advance * size;
            }
            None => x += size * 0.32,
        }
    }
    (strokes, x, size * 1.25)
}

/// Written strokes as an element `w` by `h`, the pen as far along them as `started` and `writing` say,
/// scaled down to fit a narrower place than they were written for.
pub fn written(strokes: Vec<Inked>, w: f32, h: f32, started: Instant, writing: Duration, window: &mut Window) -> impl IntoElement {
    let elapsed = started.elapsed();
    if elapsed < writing {
        window.request_animation_frame();
    }
    let total: f32 = strokes.iter().map(|s| s.length).sum();
    let done = total * (elapsed.as_secs_f32() / writing.as_secs_f32().max(0.001)).min(1.0);
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, (), window: &mut Window, _| {
            let k = (f32::from(bounds.size.width) / w.max(1.0)).min(1.0);
            // Centred in its place, at whatever scale fits.
            let left = bounds.origin.x + px((f32::from(bounds.size.width) - w * k) / 2.0);
            let top = bounds.origin.y + px((f32::from(bounds.size.height) - h * k) / 2.0);
            let at = |p: &Pt| point(left + px(p[0] * k), top + px(p[1] * k));
            let color: Hsla = rgb(pal().ink(INK)).into();
            let mut left_to_write = done;
            for stroke in &strokes {
                if left_to_write <= 0.0 {
                    break;
                }
                let shape = outline(stroke, (left_to_write / stroke.length.max(0.001)).min(1.0));
                left_to_write -= stroke.length;
                let mut pb = solid();
                let Some(first) = shape.first() else { continue };
                pb.move_to(at(first));
                shape[1..].iter().for_each(|p| pb.line_to(at(p)));
                pb.close();
                if let Ok(path) = pb.build() {
                    window.paint_path(path, Background::from(color));
                }
            }
        },
    )
    .w_full()
    .h(px(h))
}

/// A stroke that crosses itself (the bowl of в coming back to its stem) stays solid rather than punching a
/// hole, as on the board.
fn solid() -> PathBuilder {
    PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::non_zero()))
}

pub fn splash(strokes: &[Inked], started: Instant, window: &mut Window) -> impl IntoElement {
    let elapsed = started.elapsed();
    if elapsed < WRITING {
        window.request_animation_frame();
    }
    let strokes = strokes.to_vec();
    let total: f32 = strokes.iter().map(|s| s.length).sum();
    let written = total * (elapsed.as_secs_f32() / WRITING.as_secs_f32()).min(1.0);
    div()
        .size_full()
        .relative()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds: Bounds<Pixels>, (), window: &mut Window, _| {
                    let center = bounds.center();
                    let at = |p: &Pt| center + point(px(p[0]), px(p[1]));
                    let color: Hsla = rgb(pal().ink(INK)).into();
                    let mut left = written;
                    for stroke in &strokes {
                        if left <= 0.0 {
                            break;
                        }
                        let shape = outline(stroke, (left / stroke.length.max(0.001)).min(1.0));
                        left -= stroke.length;
                        let mut pb = solid();
                        let Some(first) = shape.first() else { continue };
                        pb.move_to(at(first));
                        shape[1..].iter().for_each(|p| pb.line_to(at(p)));
                        pb.close();
                        if let Ok(path) = pb.build() {
                            window.paint_path(path, Background::from(color));
                        }
                    }
                },
            )
            .size_full(),
        )
        // If this runs long, say it is still going.
        .when(elapsed > SAY_MORE_AFTER, |d| {
            d.child(div().absolute().bottom(px(48.)).w_full().flex().justify_center().text_sm().text_color(rgb(pal().faint)).child(t("getting-things-ready")))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alcyo_board::glyphs::HanziSource;

    /// The pen skips a letter it has no strokes for, which would quietly misspell the name.
    #[test]
    fn the_pen_writes_every_letter_of_the_name() {
        let mut glyphs = Glyphs::load(HanziSource::Directory(std::env::temp_dir().join("boardcast-no-hanzi"))).unwrap();
        for c in WORD.chars() {
            assert!(glyphs.get(c).is_some(), "the pen has no strokes for {c:?}");
        }
    }
}
