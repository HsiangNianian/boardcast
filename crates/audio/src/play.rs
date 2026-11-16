//! A meeting's kept audio played back, for its replay: 16 kHz mono through the computer's output, at the
//! device's own rate and channels. Without an output it keeps time all the same, so the replay runs on.

use crate::SAMPLE_RATE;
use anyhow::{anyhow, Context, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Where playback is, in seconds, shared with the output's thread as the bits of an f64.
#[derive(Clone, Default)]
struct Shared {
    at: Arc<AtomicU64>,
    playing: Arc<AtomicBool>,
}

impl Shared {
    fn get(&self) -> f64 {
        f64::from_bits(self.at.load(Ordering::Relaxed))
    }

    fn set(&self, seconds: f64) {
        self.at.store(seconds.to_bits(), Ordering::Relaxed);
    }
}

pub struct Player {
    shared: Shared,
    seconds: f64,
    /// The output, while there is one; it stops when this goes.
    _stream: Option<cpal::Stream>,
    /// Without an output, the time playing went on from, and where it was then.
    clock: Option<(Instant, f64)>,
}

impl Player {
    /// Ready to play `samples` from the start, paused.
    pub fn new(samples: Vec<f32>) -> Self {
        let seconds = samples.len() as f64 / f64::from(SAMPLE_RATE);
        let shared = Shared::default();
        let stream = match open(Arc::new(samples), shared.clone()) {
            Ok(stream) => Some(stream),
            Err(e) => {
                eprintln!("no sound for the replay, which keeps time without it: {e:#}");
                None
            }
        };
        Self { shared, seconds, _stream: stream, clock: None }
    }

    /// Whether the sound is heard, or only its time kept.
    pub fn audible(&self) -> bool {
        self._stream.is_some()
    }

    pub fn seconds(&self) -> f64 {
        self.seconds
    }

    pub fn playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    /// Where playback is, in seconds; it stops at the end.
    pub fn at(&self) -> f64 {
        let at = match self.clock {
            Some((since, from)) if self._stream.is_none() => from + since.elapsed().as_secs_f64(),
            _ => self.shared.get(),
        };
        at.min(self.seconds)
    }

    pub fn play(&mut self) {
        if self.at() >= self.seconds {
            self.seek(0.0);
        }
        if self._stream.is_none() {
            self.clock = Some((Instant::now(), self.shared.get()));
        }
        self.shared.playing.store(true, Ordering::Relaxed);
    }

    pub fn pause(&mut self) {
        let at = self.at();
        self.shared.playing.store(false, Ordering::Relaxed);
        self.shared.set(at);
        self.clock = None;
    }

    pub fn seek(&mut self, seconds: f64) {
        let seconds = seconds.clamp(0.0, self.seconds);
        self.shared.set(seconds);
        if self.clock.is_some() {
            self.clock = Some((Instant::now(), seconds));
        }
    }
}

/// The output stream, reading the samples at `shared`'s place while it says to play.
fn open(samples: Arc<Vec<f32>>, shared: Shared) -> Result<cpal::Stream> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = cpal::default_host();
    let device = host.default_output_device().context("no sound output")?;
    let supported = device.default_output_config()?;
    let channels = usize::from(supported.channels());
    let rate = f64::from(supported.sample_rate());
    let step = f64::from(SAMPLE_RATE) / rate;
    let config = supported.config();
    // The next value for the device, the samples read in between, and the place moved on.
    let next = move || -> f32 {
        if !shared.playing.load(Ordering::Relaxed) {
            return 0.0;
        }
        let at = shared.get() * f64::from(SAMPLE_RATE);
        let i = at.floor() as usize;
        let k = (at - at.floor()) as f32;
        let value = match (samples.get(i), samples.get(i + 1)) {
            (Some(a), Some(b)) => a + (b - a) * k,
            (Some(a), None) => *a,
            _ => {
                shared.playing.store(false, Ordering::Relaxed);
                0.0
            }
        };
        shared.set((at + step) / f64::from(SAMPLE_RATE));
        value
    };
    let on_error = |e: cpal::Error| eprintln!("replay sound: {e}");
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            device.build_output_stream::<f32, _, _>(
                config,
                move |data: &mut [f32], _| {
                    for frame in data.chunks_mut(channels) {
                        frame.fill(next());
                    }
                },
                on_error,
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            device.build_output_stream::<i16, _, _>(
                config,
                move |data: &mut [i16], _| {
                    for frame in data.chunks_mut(channels) {
                        frame.fill((next().clamp(-1.0, 1.0) * 32767.0) as i16);
                    }
                },
                on_error,
                None,
            )?
        }
        other => return Err(anyhow!("unsupported sample format {other:?}")),
    };
    stream.play()?;
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_sound_the_player_keeps_time() {
        let mut player = Player { shared: Shared::default(), seconds: 10.0, _stream: None, clock: None };
        assert_eq!(player.at(), 0.0);
        player.seek(4.0);
        assert_eq!(player.at(), 4.0);
        player.play();
        std::thread::sleep(std::time::Duration::from_millis(60));
        let at = player.at();
        assert!(at > 4.04 && at < 4.5, "{at}");
        player.pause();
        let held = player.at();
        std::thread::sleep(std::time::Duration::from_millis(30));
        assert_eq!(player.at(), held, "paused, it holds");
        player.seek(99.0);
        assert_eq!(player.at(), 10.0, "never past the end");
    }
}
