//! The recording's sound, drawn beside the window as it plays: the left channel on the left, the right on
//! the right, each running down past a line that marks now. What was said is above the line, bright; what
//! is coming is below it, faint. A mono recording draws the same sound on both sides.

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioBuffer, CanvasRenderingContext2d, HtmlCanvasElement, OfflineAudioContext, Response};

/// Stretches kept for each second of sound, each as its lowest and highest sample, so speech keeps its
/// spikes on both sides and silence its low hum.
const PER_SECOND: f64 = 200.0;
/// How many seconds the height of a strip shows, half of them before now.
const SPAN: f64 = 8.0;
/// The rate the recording is decoded at: plenty for its shape, and small to keep.
const DECODE_RATE: f32 = 8000.0;

/// One recording's shape: each channel's stretches as (lowest, highest), scaled so its loudest stretches
/// reach the strip's sides.
struct Shape {
    src: String,
    channels: Vec<Vec<(f32, f32)>>,
}

#[derive(Clone, Default)]
pub struct Waves {
    shape: Rc<RefCell<Option<Shape>>>,
    canvases: Vec<HtmlCanvasElement>,
}

impl Waves {
    /// The two strips, if the page has them.
    pub fn find(doc: &web_sys::Document) -> Self {
        let canvases = ["wave-left", "wave-right"].iter().filter_map(|id| doc.get_element_by_id(id)?.dyn_into::<HtmlCanvasElement>().ok()).collect();
        Self { shape: Rc::default(), canvases }
    }

    /// Read a recording's shape, unless it is the one already read. Until it is, the strips show only now.
    pub fn load(&self, src: &str) {
        if self.canvases.is_empty() || self.shape.borrow().as_ref().is_some_and(|s| s.src == src) {
            return;
        }
        *self.shape.borrow_mut() = None;
        let (shape, src) = (self.shape.clone(), src.to_string());
        wasm_bindgen_futures::spawn_local(async move {
            match decode(&src).await {
                Ok(channels) => *shape.borrow_mut() = Some(Shape { src, channels }),
                Err(e) => web_sys::console::warn_2(&"the recording's shape could not be read".into(), &e),
            }
        });
    }

    /// Draw both strips for second `t` of the recording.
    pub fn draw(&self, t: f64) {
        let shape = self.shape.borrow();
        for (side, canvas) in self.canvases.iter().enumerate() {
            let points = shape.as_ref().and_then(|s| s.channels.get(side).or_else(|| s.channels.first()));
            draw_strip(canvas, points.map(Vec::as_slice), t);
        }
    }
}

async fn decode(src: &str) -> Result<Vec<Vec<(f32, f32)>>, JsValue> {
    let response: Response = JsFuture::from(web_sys::window().ok_or("no window")?.fetch_with_str(src)).await?.dyn_into()?;
    let bytes = JsFuture::from(response.array_buffer()?).await?;
    let context = OfflineAudioContext::new_with_number_of_channels_and_length_and_sample_rate(2, 1, DECODE_RATE)?;
    let buffer: AudioBuffer = JsFuture::from(context.decode_audio_data(&bytes.dyn_into()?)?).await?.dyn_into()?;
    let step = (f64::from(buffer.sample_rate()) / PER_SECOND).max(1.0) as usize;
    let mut channels = vec![];
    for c in 0..buffer.number_of_channels().min(2) {
        let samples = buffer.get_channel_data(c)?;
        channels.push(samples.chunks(step).map(|chunk| chunk.iter().fold((0.0_f32, 0.0_f32), |(low, high), &v| (low.min(v), high.max(v)))).collect::<Vec<(f32, f32)>>());
    }
    // The loudest stretches reach the sides: the 99th percentile of the points, so a single click does not
    // make the rest of the meeting flat.
    let mut sizes: Vec<f32> = channels.iter().flatten().map(|(low, high)| high.max(-low)).collect();
    sizes.sort_by(f32::total_cmp);
    let top = sizes.get(sizes.len() * 99 / 100).copied().unwrap_or(1.0).max(1e-4);
    for channel in &mut channels {
        channel.iter_mut().for_each(|(low, high)| (*low, *high) = ((*low / top).clamp(-1.0, 1.0), (*high / top).clamp(-1.0, 1.0)));
    }
    Ok(channels)
}

/// One strip: time runs down it, now at the middle; the sound swings left and right of its centre.
fn draw_strip(canvas: &HtmlCanvasElement, points: Option<&[(f32, f32)]>, t: f64) {
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
    let Some(Ok(Some(ctx))) = Some(canvas.get_context("2d")) else { return };
    let Ok(ctx) = ctx.dyn_into::<CanvasRenderingContext2d>() else { return };
    let _ = ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0);
    ctx.clear_rect(0.0, 0.0, w, h);
    // The strip's colour is the page's text colour, set on it in CSS, so it follows light and dark.
    let ink = web_sys::window().and_then(|win| win.get_computed_style(canvas).ok().flatten()).and_then(|s| s.get_property_value("color").ok()).unwrap_or_else(|| "#888".into());
    ctx.set_stroke_style_str(&ink);
    let middle = h / 2.0;
    let per_pixel = SPAN / h;
    if let Some(points) = points {
        let amp = w / 2.0 - 6.0;
        // Every couple of pixels of the strip spans a few stretches. The line goes to their highest point, then
        // the next step to their lowest, and so on, so the sound shows as a thin trace spiking either way of a
        // quiet line, as on a scope, keeping how far each side swung.
        const STEP: f64 = 2.0;
        let reach = |y: f64, high: bool| {
            let first = ((t + (y - middle) * per_pixel) * PER_SECOND).floor();
            let last = ((t + (y + STEP - middle) * per_pixel) * PER_SECOND).ceil();
            let mut peak = 0.0_f32;
            for i in first.max(0.0) as usize..(last.max(0.0) as usize).min(points.len()) {
                peak = if high { peak.max(points[i].1) } else { peak.min(points[i].0) };
            }
            w / 2.0 + f64::from(peak) * amp
        };
        // What was said, then what is coming: one line each, the second faint.
        for (from, to, alpha) in [(0.0, middle, 0.9), (middle, h, 0.28)] {
            ctx.set_global_alpha(alpha);
            ctx.set_line_width(1.0);
            ctx.begin_path();
            // Steps are counted from the start of the recording, so a point keeps its side as it scrolls.
            let offset = (t / per_pixel).rem_euclid(2.0 * STEP);
            let mut y = (from / STEP).floor() * STEP - offset;
            let side = |y: f64| (((y + t / per_pixel) / STEP).round() as i64).rem_euclid(2) == 0;
            ctx.move_to(reach(y, side(y)), y);
            while y < to {
                y += STEP;
                ctx.line_to(reach(y, side(y)), y.min(to));
            }
            ctx.stroke();
        }
    }
    // Now.
    ctx.set_global_alpha(0.6);
    ctx.set_line_width(1.0);
    ctx.begin_path();
    ctx.move_to(0.0, middle.round() + 0.5);
    ctx.line_to(w, middle.round() + 0.5);
    ctx.stroke();
    ctx.set_global_alpha(1.0);
}
