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
