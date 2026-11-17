//! What is listened to: a microphone, the sound the computer is playing (the other side of an online
//! meeting), or both at once, mixed into one stream of 16 kHz mono.
//!
//! The computer's sound is a PulseAudio or PipeWire output's monitor on Linux, a WASAPI loopback on
//! Windows, and on a Mac (14.6 and later) a CoreAudio tap: in each case cpal opens it as an input.

use crate::SAMPLE_RATE;
use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// The most of the computer's sound held back for the microphone to be mixed with, a quarter of a
/// second: past it the oldest goes, so two clocks drifting apart never pile up a delay.
const MAX_LAG: usize = SAMPLE_RATE as usize / 4;

/// Where what is heard comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Listen {
    /// The room, through a microphone.
    #[default]
    Microphone,
    /// What the computer plays: the other side of an online meeting.
    Computer,
    /// Both, mixed.
    Both,
}

impl Listen {
    /// From a saved setting: "microphone", "computer" or "both"; anything else is the microphone.
    pub fn from_setting(setting: &str) -> Self {
        match setting {
            "computer" => Self::Computer,
            "both" => Self::Both,
            _ => Self::Microphone,
        }
    }

    pub fn setting(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::Computer => "computer",
            Self::Both => "both",
        }
    }

    pub fn microphone(self) -> bool {
        self != Self::Computer
    }

    pub fn computer(self) -> bool {
        self != Self::Microphone
    }
}

/// What to listen to, and through which devices: each by its id, or None for the system's default.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sources {
    pub listen: Listen,
    pub microphone: Option<String>,
    pub computer: Option<String>,
}

/// Something that went less than to plan, while listening goes on.
#[derive(Clone, Debug, PartialEq)]
pub enum Warning {
    /// The chosen microphone is not connected, so the default one listens.
    MicrophoneGone,
    /// The chosen sound output is not connected, so the default one's sound is heard.
    ComputerGone,
    /// The computer's sound could not be heard, so only the microphone is.
    ComputerFailed(String),
}

/// A microphone, or a sound output whose sound can be heard.
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    /// Stable across runs, for saving the choice.
    pub id: String,
    pub name: String,
}

fn host() -> cpal::Host {
    #[cfg(target_os = "linux")]
    return cpal::host_from_id(cpal::HostId::PulseAudio).unwrap_or_else(|_| cpal::default_host());
    #[cfg(not(target_os = "linux"))]
    cpal::default_host()
}

fn id(device: &cpal::Device) -> Option<String> {
    Some(device.id().ok()?.to_string())
}

/// A PulseAudio source that is an output's monitor rather than a microphone.
fn is_monitor(id: &str) -> bool {
    id.ends_with(".monitor")
}

fn listed(devices: impl Iterator<Item = cpal::Device>, keep: impl Fn(&str) -> bool) -> Vec<Input> {
    devices.filter_map(|d| Some(Input { id: id(&d)?, name: d.to_string() })).filter(|i| keep(&i.id)).collect()
}

/// The microphones connected now.
pub fn microphones() -> Vec<Input> {
    let Ok(devices) = host().input_devices() else { return vec![] };
    listed(devices, |id| !is_monitor(id))
}

/// The sound outputs whose sound can be heard now.
pub fn computer_sounds() -> Vec<Input> {
    #[cfg(target_os = "linux")]
    let devices = host().input_devices().map(|d| listed(d, is_monitor));
    #[cfg(not(target_os = "linux"))]
    let devices = host().output_devices().map(|d| listed(d, |_| true));
    devices.unwrap_or_default()
}

/// The chosen device, or the default when it is not there; true when it fell back.
fn microphone(host: &cpal::Host, chosen: Option<&str>) -> Result<(cpal::Device, bool)> {
    let picked = chosen.and_then(|want| host.input_devices().ok()?.find(|d| id(d).as_deref() == Some(want)));
    let fell_back = chosen.is_some() && picked.is_none();
    let device = match picked {
        Some(device) => device,
        None => host.default_input_device().context("no microphone")?,
    };
    Ok((device, fell_back))
}

#[cfg(target_os = "linux")]
fn computer(host: &cpal::Host, chosen: Option<&str>) -> Result<(cpal::Device, bool)> {
    let monitors = || host.input_devices().into_iter().flatten().filter(|d| id(d).is_some_and(|i| is_monitor(&i)));
    let picked = chosen.and_then(|want| monitors().find(|d| id(d).as_deref() == Some(want)));
    let fell_back = chosen.is_some() && picked.is_none();
    if let Some(device) = picked {
        return Ok((device, fell_back));
    }
    // The default output's monitor is named after it; failing that, any output's.
    let default = host.default_output_device().and_then(|d| id(&d)).map(|sink| format!("{sink}.monitor"));
    let device = monitors().find(|d| id(d) == default).or_else(|| monitors().next()).context("no sound output to listen to")?;
    Ok((device, fell_back))
}

#[cfg(not(target_os = "linux"))]
fn computer(host: &cpal::Host, chosen: Option<&str>) -> Result<(cpal::Device, bool)> {
    let picked = chosen.and_then(|want| host.output_devices().ok()?.find(|d| id(d).as_deref() == Some(want)));
    let fell_back = chosen.is_some() && picked.is_none();
    let device = match picked {
        Some(device) => device,
        None => host.default_output_device().context("no sound output to listen to")?,
    };
    Ok((device, fell_back))
}

/// Start hearing `device`, handing each piece to `push` as 16 kHz mono. An output is heard through its
/// loopback, which cpal opens as an input in the output's own format.
fn open(device: &cpal::Device, loopback: bool, push: impl FnMut(Vec<f32>) + Send + 'static) -> Result<cpal::Stream> {
    let supported = if loopback && cfg!(not(target_os = "linux")) { device.default_output_config()? } else { device.default_input_config()? };
    let channels = supported.channels() as usize;
    let ratio = f64::from(supported.sample_rate()) / f64::from(SAMPLE_RATE);
    let mut config = supported.config();
    if let cpal::SupportedBufferSize::Range { min, max } = *supported.buffer_size() {
        config.buffer_size = cpal::BufferSize::Fixed((supported.sample_rate() / 50).clamp(min, max));
    }
    let resample = Resample { ratio, position: 0.0, push };
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => build::<f32>(device, config, channels, resample)?,
        cpal::SampleFormat::I16 => build::<i16>(device, config, channels, resample)?,
        cpal::SampleFormat::I32 => build::<i32>(device, config, channels, resample)?,
        cpal::SampleFormat::U16 => build::<u16>(device, config, channels, resample)?,
        cpal::SampleFormat::U8 => build::<u8>(device, config, channels, resample)?,
        other => return Err(anyhow!("unsupported sample format {other:?}")),
    };
    stream.play()?;
    Ok(stream)
}

struct Resample<P> {
    ratio: f64,
    position: f64,
    push: P,
}

impl<P: FnMut(Vec<f32>)> Resample<P> {
    fn send(&mut self, mono: Vec<f32>) {
        let mut out = Vec::with_capacity((mono.len() as f64 / self.ratio) as usize + 1);
        while (self.position as usize) < mono.len() {
            out.push(mono[self.position as usize]);
            self.position += self.ratio;
        }
        self.position -= mono.len() as f64;
        (self.push)(out);
    }
}

fn build<T>(device: &cpal::Device, config: cpal::StreamConfig, channels: usize, mut resample: Resample<impl FnMut(Vec<f32>) + Send + 'static>) -> Result<cpal::Stream>
where
    T: cpal::SizedSample,
    f32: cpal::FromSample<T>,
{
    let stream = device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _| resample.send(data.chunks(channels).map(|f| f.iter().map(|&s| s.to_sample::<f32>()).sum::<f32>() / channels as f32).collect()),
        |e: cpal::Error| eprintln!("listening: {e}"),
        None,
    )?;
    Ok(stream)
}

/// Mix what the computer played into a piece from the microphone, taking as much of it as the piece is
/// long; when the computer has played less, the rest is the microphone alone.
fn mix(piece: &mut [f32], computer: &mut VecDeque<f32>) {
    let n = piece.len().min(computer.len());
    for (s, c) in piece.iter_mut().zip(computer.drain(..n)) {
        *s = (*s + c).clamp(-1.0, 1.0);
    }
}

/// Hold the computer's sound for the microphone, keeping no more than [`MAX_LAG`] of it.
fn hold(computer: &mut VecDeque<f32>, piece: Vec<f32>) {
    computer.extend(piece);
    let over = computer.len().saturating_sub(MAX_LAG);
    computer.drain(..over);
}

/// Live capture, mono at 16 kHz, until `stop` is set. A chosen device that is gone is replaced by the
/// default, and `warn` hears about it; so does the computer's sound failing while the microphone goes on.
pub fn capture(stop: &AtomicBool, samples: &Sender<Vec<f32>>, sources: &Sources, warn: impl Fn(Warning)) -> Result<()> {
    let host = host();
    let tx = samples.clone();
    let mut streams = vec![];
    match sources.listen {
        Listen::Microphone => {
            let (device, fell_back) = microphone(&host, sources.microphone.as_deref())?;
            if fell_back {
                warn(Warning::MicrophoneGone);
            }
            streams.push(open(&device, false, move |piece| drop(tx.send(piece)))?);
        }
        Listen::Computer => {
            let (device, fell_back) = computer(&host, sources.computer.as_deref())?;
            if fell_back {
                warn(Warning::ComputerGone);
            }
            streams.push(open(&device, true, move |piece| drop(tx.send(piece))).context("hearing the computer's sound")?);
        }
        Listen::Both => {
            // The microphone keeps time: each of its pieces takes with it what the computer played meanwhile.
            // The computer is silent between sounds on some systems, and the room never is.
            let held = Arc::new(Mutex::new(VecDeque::<f32>::with_capacity(MAX_LAG)));
            let opened = computer(&host, sources.computer.as_deref()).and_then(|(device, fell_back)| {
                let held = held.clone();
                let stream = open(&device, true, move |piece| hold(&mut held.lock().unwrap_or_else(|e| e.into_inner()), piece))?;
                Ok((stream, fell_back))
            });
            match opened {
                Ok((stream, fell_back)) => {
                    if fell_back {
                        warn(Warning::ComputerGone);
                    }
                    streams.push(stream);
                }
                Err(e) => warn(Warning::ComputerFailed(format!("{e:#}"))),
            }
            let (device, fell_back) = microphone(&host, sources.microphone.as_deref())?;
            if fell_back {
                warn(Warning::MicrophoneGone);
            }
            streams.push(open(&device, false, move |mut piece| {
                mix(&mut piece, &mut held.lock().unwrap_or_else(|e| e.into_inner()));
                let _ = tx.send(piece);
            })?);
        }
    }
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_millis(50));
    }
    drop(streams);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_setting_names_where_to_listen() {
        for listen in [Listen::Microphone, Listen::Computer, Listen::Both] {
            assert_eq!(Listen::from_setting(listen.setting()), listen);
        }
        assert_eq!(Listen::from_setting(""), Listen::Microphone);
        assert!(Listen::Both.microphone() && Listen::Both.computer());
        assert!(!Listen::Computer.microphone() && !Listen::Microphone.computer());
    }

    #[test]
    fn the_computer_is_mixed_in_and_never_falls_far_behind() {
        let mut held = VecDeque::new();
        hold(&mut held, vec![0.25; 3]);
        let mut piece = vec![0.5; 5];
        mix(&mut piece, &mut held);
        // What the computer played is added where there is some, and the rest is the microphone alone.
        assert_eq!(piece, [0.75, 0.75, 0.75, 0.5, 0.5]);
        assert!(held.is_empty());
        // Loud on both is kept within range.
        hold(&mut held, vec![0.8; 2]);
        let mut loud = vec![0.6, -0.9];
        mix(&mut loud, &mut held);
        assert_eq!(loud[0], 1.0);
        assert!((loud[1] + 0.1).abs() < 1e-6);
        // A computer that runs ahead loses its oldest sound, not its newest.
        hold(&mut held, vec![0.1; MAX_LAG]);
        hold(&mut held, vec![0.2; 10]);
        assert_eq!(held.len(), MAX_LAG);
        assert_eq!(held.back(), Some(&0.2));
    }

    #[test]
    fn a_monitor_is_not_a_microphone() {
        assert!(is_monitor("alsa_output.pci-0000_00_1f.3.analog-stereo.monitor"));
        assert!(!is_monitor("alsa_input.pci-0000_00_1f.3.analog-stereo"));
    }
}
