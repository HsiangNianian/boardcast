//! The recording's sound as a live scope either side of the window: the wave of this very moment, the last
//! few dozen milliseconds of it, drawn again every frame, so it wriggles while someone speaks and lies flat in
//! a pause. The left channel is on the left and the right on the right, each with now at its end beside the
//! window; a mono recording draws the same sound in both.

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioBuffer, CanvasRenderingContext2d, HtmlCanvasElement, OfflineAudioContext, Response};

/// The rate the recording is decoded at: enough for the wave's shape, and small to keep.
const RATE: f32 = 8000.0;
/// How much sound a strip shows at once, in seconds, ending now: a few cycles of a voice.
const SPAN: f64 = 0.06;
/// How far in from each end the wave fades out, as a share of the strip's width.
const FADE: f64 = 0.08;

/// One recording's samples per channel, scaled so its loud moments reach the strip's edges.
struct Shape {
    src: String,
    channels: Vec<Vec<f32>>,
}

#[derive(Clone, Default)]
pub struct Waves {
    shape: Rc<RefCell<Option<Shape>>>,
    canvases: Vec<HtmlCanvasElement>,
}

impl Waves {
    /// The two strips, if the page has them: left of the window, then right.
    pub fn find(doc: &web_sys::Document) -> Self {
        let canvases = ["wave-left", "wave-right"].iter().filter_map(|id| doc.get_element_by_id(id)?.dyn_into::<HtmlCanvasElement>().ok()).collect();
        Self { shape: Rc::default(), canvases }
    }

    /// Read a recording, unless it is the one already read. Until it is, the strips show a flat line.
    pub fn load(&self, src: &str) {
        if self.canvases.is_empty() || self.shape.borrow().as_ref().is_some_and(|s| s.src == src) {
            return;
        }
        *self.shape.borrow_mut() = None;
        let (shape, src) = (self.shape.clone(), src.to_string());
        wasm_bindgen_futures::spawn_local(async move {
            match decode(&src).await {
                Ok(channels) => *shape.borrow_mut() = Some(Shape { src, channels }),
                Err(e) => web_sys::console::warn_2(&"the recording could not be read for its wave".into(), &e),
            }
        });
    }

    /// Draw both strips as the sound is at second `t` of the recording.
    pub fn draw(&self, t: f64) {
        let shape = self.shape.borrow();
        for (side, canvas) in self.canvases.iter().enumerate() {
            let samples = shape.as_ref().and_then(|s| s.channels.get(side).or_else(|| s.channels.first()));
            draw_strip(canvas, samples.map(Vec::as_slice), t, side == 1);
        }
    }
}

async fn decode(src: &str) -> Result<Vec<Vec<f32>>, JsValue> {
    let response: Response = JsFuture::from(web_sys::window().ok_or("no window")?.fetch_with_str(src)).await?.dyn_into()?;
    let bytes = JsFuture::from(response.array_buffer()?).await?;
    let context = OfflineAudioContext::new_with_number_of_channels_and_length_and_sample_rate(2, 1, RATE)?;
    let buffer: AudioBuffer = JsFuture::from(context.decode_audio_data(&bytes.dyn_into()?)?).await?.dyn_into()?;
    let mut channels = vec![];
    for c in 0..buffer.number_of_channels().min(2) {
        channels.push(buffer.get_channel_data(c)?);
    }
    // Loud moments reach the edges: the 99.5th percentile of the samples' size, from every 16th sample, so a
    // single click does not leave the rest of the meeting flat.
    let mut sizes: Vec<f32> = channels.iter().flat_map(|c| c.iter().step_by(16)).map(|v| v.abs()).collect();
    sizes.sort_by(f32::total_cmp);
    let top = sizes.get(sizes.len() * 995 / 1000).copied().unwrap_or(1.0).max(1e-4);
    for channel in &mut channels {
        channel.iter_mut().for_each(|v| *v = (*v / top).clamp(-1.0, 1.0));
    }
    Ok(channels)
}

/// One strip: the last `SPAN` of sound about a level line, ending at now on the right, or on the left when
/// `mirrored`, so now is always beside the window.
fn draw_strip(canvas: &HtmlCanvasElement, samples: Option<&[f32]>, t: f64, mirrored: bool) {
    let (w, h) = (f64::from(canvas.client_width()), f64::from(canvas.client_height()));
    if w < 8.0 || h < 8.0 {
        return;
    }
    let dpr = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio());
    let (pw, ph) = ((w * dpr).round() as u32, (h * dpr).round() as u32);
    if canvas.width() != pw || canvas.height() != ph {
        canvas.set_width(pw);
        canvas.set_height(ph);
    }
    let Ok(Some(ctx)) = canvas.get_context("2d") else { return };
    let Ok(ctx) = ctx.dyn_into::<CanvasRenderingContext2d>() else { return };
    let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
    let _ = ctx.set_global_composite_operation("source-over");
    ctx.clear_rect(0.0, 0.0, w, h);
    // The page's text colour, set on the strip in CSS, so it follows light and dark.
    let ink = web_sys::window().and_then(|win| win.get_computed_style(canvas).ok().flatten()).and_then(|s| s.get_property_value("color").ok()).unwrap_or_else(|| "#888".into());
    ctx.set_stroke_style_str(&ink);
    ctx.set_line_width(1.1);
    ctx.set_line_join("round");
    let middle = h / 2.0;
    let amp = h / 2.0 - 4.0;
    let start = (t - SPAN) * f64::from(RATE);
    let count = SPAN * f64::from(RATE);
    let sample = |i: f64| match samples {
        Some(s) if i >= 0.0 && (i as usize) < s.len() => f64::from(s[i as usize]),
        _ => 0.0,
    };
    // Lightly smoothed, so the line has no single-sample teeth.
    let smooth = |i: f64| (sample(i - 1.0) + 2.0 * sample(i) + sample(i + 1.0)) / 4.0;
    let at = |x: f64| {
        let along = if mirrored { w - x } else { x };
        let f = start + along / w * count;
        let k = f - f.floor();
        let v = smooth(f.floor()) * (1.0 - k) + smooth(f.floor() + 1.0) * k;
        // Loud sound is pressed gently rather than cut off at the edges, and the wave tapers to nothing at both
        // ends, as a plucked string is still where it is held.
        let pressed = (1.6 * v).tanh() / 1.6_f64.tanh() * 0.9;
        let taper = (std::f64::consts::PI * x / w).sin().powf(1.4);
        middle - pressed * taper * amp
    };
    // A smooth curve through a point every few pixels, each corner rounded off at the midpoints.
    const STEP: f64 = 3.0;
    let points: Vec<(f64, f64)> = (0..=(w / STEP).ceil() as usize)
        .map(|i| {
            let x = (i as f64 * STEP).min(w);
            (x, at(x))
        })
        .collect();
    ctx.begin_path();
    ctx.move_to(points[0].0, points[0].1);
    for pair in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        ctx.quadratic_curve_to(x0, y0, (x0 + x1) / 2.0, (y0 + y1) / 2.0);
    }
    if let Some(&(x, y)) = points.last() {
        ctx.line_to(x, y);
    }
    ctx.stroke();
    // Both ends fade into the page, so the wave seems to pass through rather than stop at an edge.
    let _ = ctx.set_global_composite_operation("destination-out");
    for (from, to) in [(0.0, w * FADE), (w, w * (1.0 - FADE))] {
        let fade = ctx.create_linear_gradient(from, 0.0, to, 0.0);
        let _ = fade.add_color_stop(0.0, "rgba(0,0,0,1)");
        let _ = fade.add_color_stop(1.0, "rgba(0,0,0,0)");
        ctx.set_fill_style_canvas_gradient(&fade);
        ctx.fill_rect(from.min(to), 0.0, (to - from).abs(), h);
    }
    let _ = ctx.set_global_composite_operation("source-over");
}
