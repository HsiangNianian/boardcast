//! How things move: three durations and one curve, used only where something changes state. Nothing
//! bounces, and nothing moves to look alive except what listening shows, which carries information.
//! With reduced motion on (Settings, General) GPUI draws every animation at rest.

use crate::theme::pal;
use gpui_kit::{ease_out_quint, rgb, Animation, Hsla, SpringConfig};
use std::time::Duration;

/// Panels, a selection moving, a line arriving, a status changing.
pub const BASE: Duration = Duration::from_millis(180);
/// The beat of what pulses while listening.
pub const PULSE: Duration = Duration::from_millis(1600);

/// A one-off change over `duration`, easing out.
pub fn ease(duration: Duration) -> Animation {
    Animation::new(duration).with_easing(ease_out_quint())
}

/// A slow pulse between dim and full, for a dot that says something is live. It needs no more than a
/// few frames a second, so it does not keep the whole window drawing at full rate.
pub fn pulse() -> Animation {
    // Full at the start, so with reduced motion, which draws a repeating animation at its start, the dot is lit.
    Animation::new(PULSE).repeat().with_easing(|t: f32| 0.35 + 0.65 * (0.5 + 0.5 * (t * std::f32::consts::TAU).cos())).with_max_fps(20.)
}

/// A spring that settles in about 120 ms without overshoot, for hover and press, which can change
/// again before they finish.
pub const SNAP: SpringConfig = SpringConfig::new(900., 60., 1.);
/// The same, settling in about [`BASE`], for a selection moving.
pub const GLIDE: SpringConfig = SpringConfig::new(576., 48., 1.);

/// The same, settling in about 240 ms, for starting and stopping listening.
pub const SETTLE: SpringConfig = SpringConfig::new(400., 40., 1.);

/// A colour `t` of the way from `a` to `b`.
pub fn mix(a: u32, b: u32, t: f32) -> Hsla {
    let t = t.clamp(0., 1.);
    let channel = |shift: u32| {
        let (x, y) = (((a >> shift) & 0xff) as f32, ((b >> shift) & 0xff) as f32);
        ((x + (y - x) * t).round() as u32) << shift
    };
    rgb(channel(16) | channel(8) | channel(0)).into()
}

/// The fill of a row or quiet button, `t` of the way into hover: the hover shade, faded in over whatever
/// the row sits on.
pub fn hover_fill(t: f32) -> Hsla {
    let mut fill: Hsla = rgb(pal().hover).into();
    fill.a = t.clamp(0., 1.);
    fill
}
