//! What shows from the moment the window opens: the name, written by the same pen as the board.
//! Nothing in the toolkit writes by hand, so this is drawn here. The caller swaps it for the app.

use crate::i18n::tr;
use crate::theme::FAINT;
use boardcast_board::glyphs::{Glyphs, Pt};
use boardcast_board::pen::{ink, line, outline, place, Inked, Rng};
use gpui_kit::{canvas, div, point, prelude::*, px, rgb, Background, Bounds, Hsla, IntoElement, PathBuilder, Pixels, Window};
use std::time::{Duration, Instant};

const WORD: &str = "boardcast";
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
        for stroke in place(&glyph.strokes, x, y, SIZE, &mut r) {
            strokes.push(ink(&stroke, SIZE * 0.09, SIZE * 0.012, &mut r));
        }
        x += glyph.advance * SIZE;
    }
    let under: Vec<Pt> = line([-width / 2.0 - 6.0, SIZE * 0.72], [width / 2.0 + 6.0, SIZE * 0.72], &mut r, 0.008);
    strokes.push(ink(&under, SIZE * 0.07, SIZE * 0.012, &mut r));
    strokes
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
                    let color: Hsla = rgb(INK).into();
                    let mut left = written;
                    for stroke in &strokes {
                        if left <= 0.0 {
                            break;
                        }
                        let shape = outline(stroke, (left / stroke.length.max(0.001)).min(1.0));
                        left -= stroke.length;
                        let mut pb = PathBuilder::fill();
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
            d.child(div().absolute().bottom(px(48.)).w_full().flex().justify_center().text_sm().text_color(rgb(FAINT)).child(tr("Getting things ready…", "正在做准备…")))
        })
}
