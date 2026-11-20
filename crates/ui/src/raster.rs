//! Finished ink as textures. GPUI's path cost grows with total vertices however they are grouped,
//! so strokes are rasterized once on the CPU and only touched tiles are uploaded again.

use boardcast_board::glyphs::Pt;
use gpui_kit::{point, px, size, Bounds, Corners, Pixels, Point, RenderImage, Window};
use image::{Frame, RgbaImage};
use std::collections::HashMap;
use std::sync::Arc;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

const TILE: i32 = 256;

struct Tile {
    pixels: Pixmap,
    image: Option<Arc<RenderImage>>,
    dirty: bool,
}

pub struct Layer {
    tiles: HashMap<(i32, i32), Tile>,
    pub uploads: usize,
    stale: Vec<Arc<RenderImage>>,
    /// Texture pixels per board unit. Painting at another zoom stretches until a new layer is ready.
    scale: f32,
}

impl Default for Layer {
    fn default() -> Self {
        Self::with_scale(1.0)
    }
}

impl Layer {
    pub fn with_scale(scale: f32) -> Self {
        Self { tiles: HashMap::new(), uploads: 0, stale: vec![], scale }
    }

    /// Give up this layer; the caller drops the returned textures during a paint.
    pub fn release(self) -> Vec<Arc<RenderImage>> {
        let mut images = self.stale;
        images.extend(self.tiles.into_values().filter_map(|tile| tile.image));
        images
    }

    pub fn fill(&mut self, shape: &[Pt], rgb: [u8; 3], alpha: u8) {
        let Some(first) = shape.first() else { return };
        let mut b = PathBuilder::new();
        b.move_to(first[0], first[1]);
        for p in &shape[1..] {
            b.line_to(p[0], p[1]);
        }
        b.close();
        let Some(path) = b.finish() else { return };
        let mut paint = Paint::default();
        paint.set_color_rgba8(rgb[0], rgb[1], rgb[2], alpha);
        paint.anti_alias = true;
        let area = path.bounds();
        let scale = self.scale;
        let tile_of = |v: f32| ((v * scale).floor() as i32).div_euclid(TILE);
        let pad = 1.0 / scale;
        for ty in tile_of(area.top() - pad)..=tile_of(area.bottom() + pad) {
            for tx in tile_of(area.left() - pad)..=tile_of(area.right() + pad) {
                // Only the tiles the shape reaches, not every tile in its bounding box: a long arrow across
                // a two-hour meeting's board spans most of it, and at twice the scale one such line made
                // 1,600 tiles, 1,514 of them empty, 400 MB before upload, all drawn every frame zoomed out.
                let rect = [(tx * TILE) as f32 / scale - pad, (ty * TILE) as f32 / scale - pad, ((tx + 1) * TILE) as f32 / scale + pad, ((ty + 1) * TILE) as f32 / scale + pad];
                if !reaches(shape, rect) {
                    continue;
                }
                // Made only for a tile nothing has touched yet: most strokes land on one that is there.
                let tile = self.tiles.entry((tx, ty)).or_insert_with(|| Tile { pixels: Pixmap::new(TILE as u32, TILE as u32).expect("a tile has a size"), image: None, dirty: false });
                let at = Transform::from_row(scale, 0.0, 0.0, scale, (-tx * TILE) as f32, (-ty * TILE) as f32);
                tile.pixels.fill_path(&path, &paint, FillRule::Winding, at, None);
                tile.dirty = true;
            }
        }
    }

    /// Forget everything; the old textures leave the GPU atlas on the next paint.
    pub fn clear(&mut self) {
        self.stale.extend(self.tiles.drain().filter_map(|(_, tile)| tile.image));
    }

    /// Paint at `k` screen pixels per board unit, with the board origin at `origin`. Only tiles
    /// inside `clip` are uploaded and drawn, so a large board costs what is on screen.
    pub fn paint(&mut self, window: &mut Window, origin: Point<Pixels>, k: f32, clip: Option<Bounds<Pixels>>) {
        for image in self.stale.drain(..) {
            let _ = window.drop_image(image);
        }
        let side = TILE as f32 / self.scale * k;
        for ((tx, ty), tile) in &mut self.tiles {
            // Both edges round the same way, so neighbouring tiles meet without a hairline.
            let left = (f32::from(origin.x) + *tx as f32 * side).round();
            let top = (f32::from(origin.y) + *ty as f32 * side).round();
            let right = (f32::from(origin.x) + (*tx + 1) as f32 * side).round();
            let bottom = (f32::from(origin.y) + (*ty + 1) as f32 * side).round();
            let bounds: Bounds<Pixels> = Bounds::new(point(px(left), px(top)), size(px(right - left), px(bottom - top)));
            if clip.is_some_and(|c| {
                right < f32::from(c.origin.x) || bottom < f32::from(c.origin.y) || left > f32::from(c.origin.x) + f32::from(c.size.width) || top > f32::from(c.origin.y) + f32::from(c.size.height)
            }) {
                continue;
            }
            if tile.dirty || tile.image.is_none() {
                tile.dirty = false;
                self.uploads += 1;
                if let Some(old) = tile.image.take() {
                    let _ = window.drop_image(old);
                }
                tile.image = Some(Arc::new(to_image(&tile.pixels)));
            }
            let Some(image) = tile.image.clone() else { continue };
            let _ = window.paint_image(bounds, bounds, Corners::default(), image, 0, false);
        }
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }
}

/// Whether a closed shape reaches into a rectangle `[left, top, right, bottom]`: an edge crosses it, or the
/// shape surrounds it. A shape wholly inside it has edges inside it, so that is the first case.
fn reaches(shape: &[Pt], rect: [f32; 4]) -> bool {
    let [left, top, right, bottom] = rect;
    let n = shape.len();
    let crosses = (0..n).any(|i| {
        let (a, b) = (shape[i], shape[(i + 1) % n]);
        // Liang–Barsky: clip the segment to the rectangle and see whether anything is left.
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
        for (p, q) in [(-dx, a[0] - left), (dx, right - a[0]), (-dy, a[1] - top), (dy, bottom - a[1])] {
            if p == 0.0 {
                if q < 0.0 {
                    return false;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
                if t0 > t1 {
                    return false;
                }
            }
        }
        true
    });
    crosses || winds_around(shape, [(left + right) / 2.0, (top + bottom) / 2.0])
}

/// Whether a closed shape winds around a point, as tiny-skia's winding fill would paint it.
fn winds_around(shape: &[Pt], at: Pt) -> bool {
    let n = shape.len();
    let mut winding = 0;
    for i in 0..n {
        let (a, b) = (shape[i], shape[(i + 1) % n]);
        let side = (b[0] - a[0]) * (at[1] - a[1]) - (at[0] - a[0]) * (b[1] - a[1]);
        if a[1] <= at[1] {
            if b[1] > at[1] && side > 0.0 {
                winding += 1;
            }
        } else if b[1] <= at[1] && side < 0.0 {
            winding -= 1;
        }
    }
    winding != 0
}

/// GPUI takes straight-alpha BGRA, while tiny-skia's pixels are premultiplied. Passing them through
/// unchanged applies alpha twice and turns a translucent highlight dark.
fn to_image(pixels: &Pixmap) -> RenderImage {
    let mut raw = pixels.data().to_vec();
    for px in raw.chunks_exact_mut(4) {
        let alpha = u32::from(px[3]);
        if alpha > 0 && alpha < 255 {
            for channel in &mut px[..3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
        px.swap(0, 2);
    }
    let buffer = RgbaImage::from_raw(pixels.width(), pixels.height(), raw).expect("tile buffer size");
    RenderImage::new([Frame::new(buffer)])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tile in the bounding box, as the layer filled before: the reference for what must be drawn.
    fn by_bounding_box(shapes: &[Vec<Pt>], scale: f32) -> HashMap<(i32, i32), Pixmap> {
        let mut tiles: HashMap<(i32, i32), Pixmap> = HashMap::new();
        for shape in shapes {
            let mut b = PathBuilder::new();
            b.move_to(shape[0][0], shape[0][1]);
            for p in &shape[1..] {
                b.line_to(p[0], p[1]);
            }
            b.close();
            let path = b.finish().unwrap();
            let mut paint = Paint::default();
            paint.set_color_rgba8(30, 30, 30, 255);
            paint.anti_alias = true;
            let area = path.bounds();
            let tile_of = |v: f32| ((v * scale).floor() as i32).div_euclid(TILE);
            let pad = 1.0 / scale;
            for ty in tile_of(area.top() - pad)..=tile_of(area.bottom() + pad) {
                for tx in tile_of(area.left() - pad)..=tile_of(area.right() + pad) {
                    let tile = tiles.entry((tx, ty)).or_insert_with(|| Pixmap::new(TILE as u32, TILE as u32).unwrap());
                    tile.fill_path(&path, &paint, FillRule::Winding, Transform::from_row(scale, 0.0, 0.0, scale, (-tx * TILE) as f32, (-ty * TILE) as f32), None);
                }
            }
        }
        tiles
    }

    fn inked(p: &Pixmap) -> bool {
        p.data().chunks_exact(4).any(|px| px[3] != 0)
    }

    #[test]
    fn a_long_thin_stroke_makes_only_the_tiles_it_crosses() {
        let mut layer = Layer::with_scale(2.0);
        layer.fill(&[[0.0, 0.0], [3000.0, 8000.0], [3003.0, 8000.0], [3.0, 0.0]], [0, 0, 0], 255);
        let empty = layer.tiles.values().filter(|t| !inked(&t.pixels)).count();
        assert!(layer.tile_count() < 200, "{} tiles", layer.tile_count());
        // A tile the line passes within a pixel of is kept for its anti-aliased edge, which may come out blank.
        assert!(empty * 5 <= layer.tile_count(), "{empty} of {} tiles empty", layer.tile_count());
    }

    #[test]
    fn every_inked_pixel_is_drawn_as_before() {
        // Strokes in every direction, a curve, a large filled highlight and a ring with a hole.
        let ring = |cx: f32, cy: f32, r: f32| -> Vec<Pt> {
            let outer: Vec<Pt> = (0..64).map(|i| { let a = i as f32 / 64.0 * std::f32::consts::TAU; [cx + r * a.cos(), cy + r * a.sin()] }).collect();
            outer
        };
        let shapes: Vec<Vec<Pt>> = vec![
            vec![[10.0, 10.0], [1900.0, 1500.0], [1903.0, 1504.0], [12.0, 14.0]],
            vec![[2000.0, 40.0], [60.0, 1700.0], [64.0, 1703.0], [2003.0, 44.0]],
            vec![[300.0, 900.0], [1600.0, 900.0], [1600.0, 1400.0], [300.0, 1400.0]],
            (0..80).map(|i| { let t = i as f32 / 79.0; [100.0 + 1700.0 * t, 200.0 + 600.0 * (t * 6.0).sin()] }).chain((0..80).rev().map(|i| { let t = i as f32 / 79.0; [100.0 + 1700.0 * t, 204.0 + 600.0 * (t * 6.0).sin()] })).collect(),
            ring(1000.0, 600.0, 420.0),
            vec![[127.5, 127.5], [128.5, 127.5], [128.5, 128.5], [127.5, 128.5]],
        ];
        for scale in [0.5, 1.0, 2.0, 3.0] {
            let reference = by_bounding_box(&shapes, scale);
            let mut layer = Layer::with_scale(scale);
            for shape in &shapes {
                layer.fill(shape, [30, 30, 30], 255);
            }
            for (key, pixels) in reference.iter().filter(|(_, p)| inked(p)) {
                let tile = layer.tiles.get(key).unwrap_or_else(|| panic!("tile {key:?} at {scale}x is missing"));
                assert!(tile.pixels.data() == pixels.data(), "tile {key:?} at {scale}x differs");
            }
            assert!(layer.tile_count() < reference.len(), "{} tiles against {} at {scale}x", layer.tile_count(), reference.len());
        }
    }
}
