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

/// A colour `t` of the way from `a` to `b`, as 0xRRGGBB.
pub fn mix_rgb(a: u32, b: u32, t: f32) -> u32 {
    let t = t.clamp(0., 1.);
    let channel = |shift: u32| {
        let (x, y) = (((a >> shift) & 0xff) as f32, ((b >> shift) & 0xff) as f32);
        ((x + (y - x) * t).round() as u32) << shift
    };
    channel(16) | channel(8) | channel(0)
}

/// A colour `t` of the way from `a` to `b`.
pub fn mix(a: u32, b: u32, t: f32) -> Hsla {
    rgb(mix_rgb(a, b, t)).into()
}

/// The fill of a row or quiet button, `t` of the way into hover: the hover shade, faded in over whatever
/// the row sits on.
pub fn hover_fill(t: f32) -> Hsla {
    let mut fill: Hsla = rgb(pal().hover).into();
    fill.a = t.clamp(0., 1.);
    fill
}

/// What a small system command prints, trimmed; nothing if it cannot be run or fails.
fn run(program: &str, args: &[&str]) -> Option<String> {
    let mut command = std::process::Command::new(program);
    command.args(args).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window flashing up for a moment.
        command.creation_flags(0x0800_0000);
    }
    let out = command.output().ok().filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Whether the computer is running on its battery: then what moves only for looks, the halftone ground, holds
/// still. Anything that cannot be read counts as on mains.
pub fn on_battery() -> bool {
    if cfg!(target_os = "macos") {
        run("pmset", &["-g", "batt"]).is_some_and(|v| v.contains("'Battery Power'"))
    } else if cfg!(windows) {
        // 1 is "discharging" in Win32_Battery's BatteryStatus.
        run("powershell", &["-NoProfile", "-NonInteractive", "-Command", "(Get-CimInstance Win32_Battery).BatteryStatus"]).is_some_and(|v| v.lines().any(|l| l.trim() == "1"))
    } else {
        std::fs::read_dir("/sys/class/power_supply").is_ok_and(|dir| {
            dir.flatten().any(|entry| std::fs::read_to_string(entry.path().join("status")).is_ok_and(|s| s.trim() == "Discharging"))
        })
    }
}

/// Whether the system asks for less motion: GNOME's or KDE's animation settings on Linux, Reduce motion on a
/// Mac, animations turned off on Windows. Read once at start, off the main thread; anything that cannot be
/// read counts as no.
pub fn system_reduces() -> bool {
    if cfg!(target_os = "macos") {
        run("defaults", &["read", "com.apple.universalaccess", "reduceMotion"]).is_some_and(|v| v == "1")
    } else if cfg!(windows) {
        run("reg", &["query", r"HKCU\Control Panel\Desktop\WindowMetrics", "/v", "MinAnimate"]).is_some_and(|v| v.split_whitespace().last() == Some("0"))
    } else {
        run("gsettings", &["get", "org.gnome.desktop.interface", "enable-animations"]).is_some_and(|v| v == "false")
            || ["kreadconfig6", "kreadconfig5"].iter().any(|k| run(k, &["--group", "KDE", "--key", "AnimationDurationFactor"]).is_some_and(|v| v.parse::<f32>().is_ok_and(|f| f == 0.)))
    }
}

static SYSTEM_REDUCES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Note what the system asks for, once it is known.
pub fn set_system(reduces: bool) {
    SYSTEM_REDUCES.store(reduces, std::sync::atomic::Ordering::Relaxed);
}

/// Whether to draw animations at rest: the setting, or the system asking for it.
pub fn reduced(setting: bool) -> bool {
    setting || SYSTEM_REDUCES.load(std::sync::atomic::Ordering::Relaxed)
}
