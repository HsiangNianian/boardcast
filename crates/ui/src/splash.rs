//! What shows from the moment the window opens: the name, written by the same pen as the board.
//! Nothing in the toolkit writes by hand, so this is drawn here. The caller swaps it for the app.

use crate::i18n::tr;
use crate::theme::pal;
use boardcast_board::glyphs::{Glyphs, Pt};
use boardcast_board::pen::{ink, line, outline, place, Inked, Rng};
use gpui_kit::{canvas, div, point, prelude::*, px, rgb, Background, Bounds, Hsla, IntoElement, PathBuilder, Pixels, Window};
use std::rc::Rc;
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
        for stroke in place(&glyph.strokes, x, y, SIZE, &mut r, 0.0) {
            strokes.push(ink(&stroke, SIZE * 0.09, SIZE * 0.012, &mut r));
        }
        x += glyph.advance * SIZE;
    }
    let under: Vec<Pt> = line([-width / 2.0 - 6.0, SIZE * 0.72], [width / 2.0 + 6.0, SIZE * 0.72], &mut r, 0.008);
    strokes.push(ink(&under, SIZE * 0.07, SIZE * 0.012, &mut r));
    strokes
}

/// The name as the pen wrote it, small and still, for the top of the sidebar: its outlines from (0, 0), and
/// how much room they take.
pub struct Wordmark {
    shapes: Vec<Vec<Pt>>,
    width: f32,
    height: f32,
}

/// None when the pen has no letters to write it with; the caller sets the name in type instead.
pub fn wordmark(glyphs: &mut Glyphs) -> Option<Rc<Wordmark>> {
    let mut strokes = word_strokes(glyphs);
    // The line under the name belongs to the splash.
    strokes.pop();
    let mut shapes: Vec<Vec<Pt>> = strokes.iter().map(|s| outline(s, 1.0)).filter(|shape| shape.len() > 2).collect();
    let points = || shapes.iter().flatten();
    let (x0, y0) = (points().map(|p| p[0]).reduce(f32::min)?, points().map(|p| p[1]).reduce(f32::min)?);
    let (x1, y1) = (points().map(|p| p[0]).reduce(f32::max)?, points().map(|p| p[1]).reduce(f32::max)?);
    shapes.iter_mut().flatten().for_each(|p| *p = [p[0] - x0, p[1] - y0]);
    Some(Rc::new(Wordmark { shapes, width: x1 - x0, height: (y1 - y0).max(1.0) }))
}

/// The wordmark, `height` pixels tall.
pub fn wordmark_element(mark: &Rc<Wordmark>, height: f32) -> impl IntoElement {
    let k = height / mark.height;
    let width = mark.width * k;
    let mark = mark.clone();
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, (), window: &mut Window, _| {
            let color: Hsla = rgb(pal().ink(INK)).into();
            for shape in &mark.shapes {
                let at = |p: &Pt| bounds.origin + point(px(p[0] * k), px(p[1] * k));
                let mut pb = PathBuilder::fill();
                pb.move_to(at(&shape[0]));
                shape[1..].iter().for_each(|p| pb.line_to(at(p)));
                pb.close();
                if let Ok(path) = pb.build() {
                    window.paint_path(path, Background::from(color));
                }
            }
        },
    )
    .w(px(width))
    .h(px(height))
    .flex_none()
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
            d.child(div().absolute().bottom(px(48.)).w_full().flex().justify_center().text_sm().text_color(rgb(pal().faint)).child(tr("Getting things ready…", "正在做准备…")))
        })
}
