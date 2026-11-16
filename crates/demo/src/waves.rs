//! The meeting's sound as a timeline under the window: the whole recording's loudness as thin bars, each
//! line's stretch in its speaker's colour as the transcript shows them, the part already played solid and
//! the rest faint, and a line at now.

use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioBuffer, CanvasRenderingContext2d, HtmlCanvasElement, OfflineAudioContext, Response};

/// The rate the recording is decoded at: plenty for its loudness, and small to keep.
const RATE: f32 = 8000.0;
/// A bar and the gap after it, in pixels.
const BAR: f64 = 2.0;
const PITCH: f64 = 3.0;

/// One recording: its samples, and its lines as (start, end, speaker's colour number).
struct Track {
    src: String,
    samples: Vec<f32>,
    spans: Vec<(f64, f64, usize)>,
    /// Each bar's loudness for the width they were last worked out for.
    bars: RefCell<(u32, Vec<f64>)>,
}

impl Track {
    fn seconds(&self) -> f64 {
        self.samples.len() as f64 / f64::from(RATE)
    }

    /// Each bar's loudness, 0 to 1, for a timeline `w` pixels wide: the loudest sample in its stretch,
    /// pressed so quiet talk still shows against the loud.
    fn bars(&self, w: f64) -> Vec<f64> {
        let key = w.round() as u32;
        if self.bars.borrow().0 != key {
            let count = (w / PITCH).floor().max(1.0) as usize;
            let per = (self.samples.len() / count).max(1);
            let mut raw: Vec<f64> = (0..count)
                .map(|i| {
                    let stretch = self.samples.get(i * per..((i + 1) * per).min(self.samples.len())).unwrap_or_default();
                    f64::from(stretch.iter().fold(0.0_f32, |a, v| a.max(v.abs())))
                })
                .collect();
            let mut sorted = raw.clone();
            sorted.sort_by(f64::total_cmp);
            let top = sorted.get(sorted.len() * 98 / 100).copied().unwrap_or(1.0).max(1e-4);
            raw.iter_mut().for_each(|v| *v = (*v / top).min(1.0).powf(0.7));
            *self.bars.borrow_mut() = (key, raw);
        }
        self.bars.borrow().1.clone()
    }
}

#[derive(Clone, Default)]
pub struct Waves {
    track: Rc<RefCell<Option<Track>>>,
    canvas: Option<HtmlCanvasElement>,
    /// Where the reader is holding the timeline, in seconds, while they choose a moment.
    preview: Rc<std::cell::Cell<Option<f64>>>,
}

impl Waves {
    /// The timeline, if the page has it.
    pub fn find(doc: &web_sys::Document) -> Self {
        Self { track: Rc::default(), canvas: doc.get_element_by_id("timeline").and_then(|c| c.dyn_into::<HtmlCanvasElement>().ok()), preview: Rc::default() }
    }

    /// How long the recording is, once it is read.
    pub fn seconds(&self) -> Option<f64> {
        self.track.borrow().as_ref().map(Track::seconds)
    }

    /// Show where the reader is holding the timeline, or stop showing it.
    pub fn hold(&self, at: Option<f64>) {
        self.preview.set(at);
    }

    /// Read a recording and the lines said over it, unless it is the one already read. Until it is, the
    /// timeline shows a quiet line.
    pub fn load(&self, src: &str, spans: Vec<(f64, f64, usize)>) {
        if self.canvas.is_none() || self.track.borrow().as_ref().is_some_and(|s| s.src == src) {
            return;
        }
        *self.track.borrow_mut() = None;
        let (track, src) = (self.track.clone(), src.to_string());
        wasm_bindgen_futures::spawn_local(async move {
            match decode(&src).await {
                Ok(samples) => *track.borrow_mut() = Some(Track { src, samples, spans, bars: RefCell::new((0, vec![])) }),
                Err(e) => web_sys::console::warn_2(&"the recording could not be read for its timeline".into(), &e),
            }
        });
    }

    /// Draw the timeline at second `t` of the recording.
    pub fn draw(&self, t: f64) {
        if let Some(canvas) = &self.canvas {
            draw(canvas, self.track.borrow().as_ref(), t, self.preview.get());
        }
    }
}

/// The recording's samples, its channels mixed into one.
async fn decode(src: &str) -> Result<Vec<f32>, JsValue> {
    let response: Response = JsFuture::from(web_sys::window().ok_or("no window")?.fetch_with_str(src)).await?.dyn_into()?;
    let bytes = JsFuture::from(response.array_buffer()?).await?;
    let context = OfflineAudioContext::new_with_number_of_channels_and_length_and_sample_rate(1, 1, RATE)?;
    let buffer: AudioBuffer = JsFuture::from(context.decode_audio_data(&bytes.dyn_into()?)?).await?.dyn_into()?;
    let mut mixed = buffer.get_channel_data(0)?;
    for c in 1..buffer.number_of_channels() {
        for (m, v) in mixed.iter_mut().zip(buffer.get_channel_data(c)?) {
            *m = (*m + v) / 2.0;
        }
    }
    Ok(mixed)
}

fn draw(canvas: &HtmlCanvasElement, track: Option<&Track>, t: f64, held: Option<f64>) {
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
    ctx.clear_rect(0.0, 0.0, w, h);
    // The page's own colours, from the strip's style: its text, and each speaker's as the transcript has it.
    let style = web_sys::window().and_then(|win| win.get_computed_style(canvas).ok().flatten());
    let read = |name: &str, or: &str| style.as_ref().and_then(|s| s.get_property_value(name).ok()).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).unwrap_or_else(|| or.to_string());
    let ink = read("color", "#888");
    let speakers: Vec<String> = (0..6).map(|n| read(&format!("--p{n}i"), &ink)).collect();
    let middle = h / 2.0;
    let Some(track) = track else {
        // Not read yet: a quiet line.
        ctx.set_fill_style_str(&ink);
        ctx.set_global_alpha(0.25);
        ctx.fill_rect(0.0, middle - 0.5, w, 1.0);
        ctx.set_global_alpha(1.0);
        return;
    };
    let seconds = track.seconds().max(0.001);
    let bars = track.bars(w);
    let now_x = (t / seconds * w).clamp(0.0, w);
    for (i, &level) in bars.iter().enumerate() {
        let x = i as f64 * PITCH;
        let at = (x + BAR / 2.0) / w * seconds;
        let colour = track.spans.iter().find(|(start, end, _)| at >= *start && at <= *end).map_or(&ink, |&(_, _, n)| &speakers[n % 6]);
        ctx.set_fill_style_str(colour);
        ctx.set_global_alpha(if x + BAR <= now_x { 1.0 } else { 0.28 });
        let half = (level * (middle - 2.0)).max(0.5);
        ctx.fill_rect(x, middle - half, BAR, 2.0 * half);
    }
    // Now.
    ctx.set_global_alpha(1.0);
    ctx.set_fill_style_str(&ink);
    ctx.fill_rect(now_x.round() - 0.5, 0.0, 1.0, h);
    // Where the reader is holding it: a firmer line with a knob, which the meeting jumps to when let go.
    if let Some(at) = held {
        let x = (at / seconds * w).clamp(0.0, w).round();
        ctx.fill_rect(x - 1.0, 0.0, 2.0, h);
        ctx.begin_path();
        let _ = ctx.arc(x, 4.0, 4.0, 0.0, std::f64::consts::TAU);
        ctx.fill();
    }
}
