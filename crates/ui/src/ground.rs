//! The ground: the website's halftone (crates/site/src/ground.rs), shown where there is room and nothing to
//! read, which is a board before its first ink and the pages that cover the board. A soft noise field drifts
//! slowly and is screened through an 8x8 ordered dither into small dots on a 3 px lattice. GPUI has no shader
//! for this, so the dots are drawn into an image on a background thread a few times a second, and that image
//! is painted; with reduced motion the field holds still.

use crate::theme;
use gpui_kit::{px, size, App, Bounds, Context, Corners, Pixels, RenderImage, Window};
use image::{Frame, RgbaImage};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often the drifting field is drawn again. The drift is slow, so a few times a second reads as the
/// website's, where each dot comes and goes as the field passes.
const REDRAW: Duration = Duration::from_millis(250);

/// What the ground is asked for and what it has drawn, shared between the view and its paint.
#[derive(Default)]
pub struct Ground {
    /// Whether anything shows the ground in the frame just rendered.
    pub wanted: bool,
    /// The size in device pixels and the scale of the last place it was painted.
    at: Option<(u32, u32, f32)>,
    /// A clear oval in the middle, for what is written there, as fractions of the width and height.
    clear: Option<(f32, f32)>,
    image: Option<(Arc<RenderImage>, (u32, u32, f32), bool)>,
    stale: Vec<Arc<RenderImage>>,
    running: bool,
    started: Option<Instant>,
}

pub type Shared = Rc<RefCell<Ground>>;

/// Ask for the ground in this frame, with or without a clear middle, and keep it drawing while it is asked for.
pub fn want<V: 'static>(ground: &Shared, clear: Option<(f32, f32)>, cx: &mut Context<V>) {
    let mut g = ground.borrow_mut();
    g.wanted = true;
    g.clear = clear;
    if g.running {
        return;
    }
    g.running = true;
    g.started.get_or_insert_with(Instant::now);
    drop(g);
    let ground = ground.clone();
    cx.spawn(async move |view, cx| loop {
        // Asked for in the last frame, and painted somewhere, or there is nothing to draw yet.
        let job = {
            let mut g = ground.borrow_mut();
            if !g.wanted {
                g.running = false;
                return;
            }
            g.wanted = false;
            g.at.map(|at| (at, g.clear, g.started.map_or(0., |s| s.elapsed().as_secs_f32())))
        };
        if let Some(((w, h, scale), clear, seconds)) = job {
            let still = cx.update(|cx: &mut App| cx.reduce_motion());
            let dark = theme::is_dark();
            let t = if still { 0. } else { seconds };
            let image = cx.background_executor().spawn(async move { draw(w, h, scale, t, dark, clear) }).await;
            let mut g = ground.borrow_mut();
            if let Some((old, ..)) = g.image.replace((Arc::new(image), (w, h, scale), dark)) {
                g.stale.push(old);
            }
        }
        if view.update(cx, |_, cx| cx.notify()).is_err() {
            return;
        }
        cx.background_executor().timer(REDRAW).await;
    })
    .detach();
}

/// Paint the ground over `bounds` at `opacity`, noting the size it is wanted at for the next drawing.
pub fn paint(ground: &Shared, bounds: Bounds<Pixels>, opacity: f32, window: &mut Window) {
    let mut g = ground.borrow_mut();
    for old in g.stale.drain(..) {
        let _ = window.drop_image(old);
    }
    let scale = window.scale_factor();
    let at = ((f32::from(bounds.size.width) * scale).round() as u32, (f32::from(bounds.size.height) * scale).round() as u32, scale);
    g.at = Some(at);
    if opacity <= 0.001 {
        return;
    }
    // A drawing of another size or the other palette is not stretched or shown; the next one is a moment away.
    // How faint it is comes from the element it is painted in, which fades it in and out.
    if let Some((image, ..)) = g.image.clone().filter(|(_, drawn, dark)| *drawn == at && *dark == theme::is_dark()) {
        let area = Bounds::new(bounds.origin, size(px(at.0 as f32 / scale), px(at.1 as f32 / scale)));
        let _ = window.paint_image(area, area, Corners::default(), image, 0, false);
    }
}

fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn hash(x: f32, y: f32) -> f32 {
    fract((x * 127.1 + y * 311.7).sin() * 43_758.547)
}

fn noise(x: f32, y: f32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (mut fx, mut fy) = (x - ix, y - iy);
    fx = fx * fx * (3. - 2. * fx);
    fy = fy * fy * (3. - 2. * fy);
    let (a, b, c, d) = (hash(ix, iy), hash(ix + 1., iy), hash(ix, iy + 1.), hash(ix + 1., iy + 1.));
    let top = a + (b - a) * fx;
    top + ((c + (d - c) * fx) - top) * fy
}

fn fbm(mut x: f32, mut y: f32) -> f32 {
    let (mut v, mut a) = (0., 0.5);
    for _ in 0..4 {
        v += a * noise(x, y);
        x = x * 2.03 + 1.7;
        y = y * 2.03 + 9.2;
        a *= 0.5;
    }
    v
}

fn bayer2(x: f32, y: f32) -> f32 {
    let (x, y) = (x.floor(), y.floor());
    fract(x * 0.5 + y * y * 0.75)
}

fn bayer4(x: f32, y: f32) -> f32 {
    bayer2(0.5 * x, 0.5 * y) * 0.25 + bayer2(x, y)
}

fn bayer8(x: f32, y: f32) -> f32 {
    bayer4(0.5 * x, 0.5 * y) * 0.25 + bayer2(x, y)
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// The ground at `t` seconds, `w` by `h` device pixels, as GPUI takes it: straight-alpha BGRA.
pub fn draw(w: u32, h: u32, scale: f32, t: f32, dark: bool, clear: Option<(f32, f32)>) -> RenderImage {
    let mut buffer = RgbaImage::new(w.max(1), h.max(1));
    // Light ink on dark paper reads stronger than dark on light, so it is thinner, as on the website.
    let (ink, alpha) = if dark { (0xf2u8, 0.16) } else { (0x11u8, 0.13) };
    let a = (alpha * 255.0f32).round() as u8;
    let cell = (3. * scale).round().max(2.) as u32;
    let dot = ((cell as f32) * 0.67).round().max(1.) as u32;
    let (cols, rows) = (w.div_ceil(cell), h.div_ceil(cell));
    // The field is smooth, so it is worked out every few cells and blended between.
    const STEP: u32 = 4;
    let (gc, gr) = (cols / STEP + 2, rows / STEP + 2);
    let field: Vec<f32> = (0..gr)
        .flat_map(|j| (0..gc).map(move |i| (i, j)))
        .map(|(i, j)| {
            let ux = (i * STEP * cell) as f32 / h as f32;
            let uy = (j * STEP * cell) as f32 / h as f32;
            fbm(ux * 2.4 + t * 0.018, uy * 2.4 - t * 0.011)
        })
        .collect();
    let at = |i: u32, j: u32| field[(j.min(gr - 1) * gc + i.min(gc - 1)) as usize];
    for cy in 0..rows {
        // The shader counts rows from the bottom.
        let gy = rows - 1 - cy;
        for cx in 0..cols {
            let (i, j) = (cx / STEP, gy / STEP);
            let (fx, fy) = ((cx % STEP) as f32 / STEP as f32, (gy % STEP) as f32 / STEP as f32);
            let top = at(i, j) + (at(i + 1, j) - at(i, j)) * fx;
            let bottom = at(i, j + 1) + (at(i + 1, j + 1) - at(i, j + 1)) * fx;
            let mut n = smoothstep(0.38, 0.82, top + (bottom - top) * fy) * 0.7;
            if let Some((rx, ry)) = clear {
                let dx = (cx * cell) as f32 / w as f32 - 0.5;
                let dy = (cy * cell) as f32 / h as f32 - 0.5;
                let d = ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt();
                n *= smoothstep(1.0, 1.5, d);
            }
            // The faintest levels of a dither are lone dots in a lattice, which read as a screen, not as tone.
            if n < 0.09 || bayer8(cx as f32, gy as f32) > n {
                continue;
            }
            let (x0, y0) = (cx * cell, cy * cell + (cell - dot));
            for y in y0..(y0 + dot).min(h) {
                for x in x0..(x0 + dot).min(w) {
                    buffer.put_pixel(x, y, image::Rgba([ink, ink, ink, a]));
                }
            }
        }
    }
    RenderImage::new([Frame::new(buffer)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ground_is_mostly_empty_and_never_dense() {
        let image = draw(300, 200, 1., 3., false, None);
        let bytes = image.as_bytes(0).unwrap();
        let on = bytes.chunks_exact(4).filter(|p| p[3] > 0).count();
        let share = on as f32 / (300 * 200) as f32;
        assert!(share > 0.0 && share < 0.35, "{share}");
    }

    #[test]
    fn a_clear_middle_has_no_dots() {
        let image = draw(300, 200, 1., 3., true, Some((0.2, 0.2)));
        let bytes = image.as_bytes(0).unwrap();
        let middle = (100 * 300 + 150) * 4;
        assert_eq!(bytes[middle + 3], 0);
    }
}
