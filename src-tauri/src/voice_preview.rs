//! Explicit, bounded input metering. No recognizer, transcript events, or audio storage.
use super::{
    audio_host, microphone, open_stream, LinearResampler, Mixer, Source, Voice, SAMPLE_RATE,
};
use cpal::traits::{DeviceTrait, StreamTrait};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

const LIMIT: Duration = Duration::from_secs(10);

struct Check {
    id: String,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

#[derive(Default)]
pub(super) struct Preview {
    active: Option<Check>,
    // A close can reach IPC before its start. Keep a bounded set of canceled IDs.
    canceled: VecDeque<String>,
}

impl Preview {
    fn cancel(&mut self, id: &str) -> Option<Check> {
        if !self.canceled.iter().any(|x| x == id) {
            self.canceled.push_back(id.into());
            if self.canceled.len() > 32 {
                self.canceled.pop_front();
            }
        }
        if self.active.as_ref().is_some_and(|check| check.id == id) {
            let check = self.active.take().unwrap();
            check.stop.store(true, Ordering::SeqCst);
            Some(check)
        } else {
            None
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Status<'a> {
    id: &'a str,
    state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    device: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    level: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

fn emit(
    app: &AppHandle,
    id: &str,
    state: &str,
    device: Option<&str>,
    level: Option<f32>,
    message: Option<&str>,
    reason: Option<&str>,
) {
    let _ = app.emit(
        "voice-input-check",
        Status {
            id,
            state,
            device,
            level,
            message,
            reason,
        },
    );
}

#[tauri::command(async)]
pub fn voice_input_check_start(
    app: AppHandle,
    id: String,
    device: Option<String>,
    source: Option<String>,
) -> Result<(), String> {
    if id.is_empty() || id.len() > 128 {
        return Err("Invalid input check request".into());
    }
    if source
        .as_deref()
        .is_some_and(|source| !matches!(source, "mic" | "system" | "both"))
    {
        return Err("Invalid input source".into());
    }
    let voice = app.state::<Voice>();
    // Share the recording start gate; the preview is published before releasing it.
    let running = voice.running.lock().unwrap();
    if voice.shutting_down.load(Ordering::SeqCst) {
        return Err("Audio input is shutting down".into());
    }
    if running
        .as_ref()
        .is_some_and(|r| !r.stop.load(Ordering::SeqCst))
    {
        return Err("Stop recording before checking the input".into());
    }
    let mut preview = voice.preview.lock().unwrap();
    if preview.canceled.iter().any(|x| x == &id) {
        return Ok(());
    }
    if let Some(active) = preview.active.as_ref() {
        if !active.stop.load(Ordering::SeqCst) {
            return if active.id == id {
                Ok(())
            } else {
                Err("An input check is already running".into())
            };
        }
    }
    if let Some(old) = preview.active.take() {
        let _ = old.thread.join();
    }
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let app_thread = app.clone();
    let request = id.clone();
    emit(&app, &id, "starting", None, None, None, None);
    let thread = thread::spawn(move || {
        let deadline = Instant::now() + LIMIT;
        let result = meter(
            &app_thread,
            &request,
            device,
            Source::parse(source.as_deref()),
            &flag,
            deadline,
        );
        let reason = if flag.load(Ordering::SeqCst) {
            "cancelled"
        } else {
            "timeout"
        };
        flag.store(true, Ordering::SeqCst);
        match result {
            Err(error) if reason != "cancelled" && Instant::now() < deadline => emit(
                &app_thread,
                &request,
                "error",
                None,
                None,
                Some(&error),
                None,
            ),
            _ => emit(
                &app_thread,
                &request,
                "stopped",
                None,
                None,
                None,
                Some(reason),
            ),
        }
    });
    preview.active = Some(Check { id, stop, thread });
    Ok(())
}

#[tauri::command(async)]
pub fn voice_input_check_stop(app: AppHandle, id: Option<String>) {
    let Some(id) = id else {
        stop_all(&app);
        return;
    };
    if id.is_empty() || id.len() > 128 {
        return;
    }
    let voice = app.state::<Voice>();
    let mut preview = voice.preview.lock().unwrap();
    let check = preview.cancel(&id);
    if let Some(check) = check {
        let _ = check.thread.join();
    }
}

pub(super) fn stop_all(app: &AppHandle) {
    let voice = app.state::<Voice>();
    let mut preview = voice.preview.lock().unwrap();
    let id = preview.active.as_ref().map(|check| check.id.clone());
    let check = id.and_then(|id| preview.cancel(&id));
    if let Some(check) = check {
        let _ = check.thread.join();
    }
}

fn meter(
    app: &AppHandle,
    id: &str,
    device: Option<String>,
    source: Source,
    stop: &AtomicBool,
    deadline: Instant,
) -> Result<(), String> {
    let host = audio_host();
    let (tx, rx) = mpsc::channel();
    let (errors, error_rx) = mpsc::channel();
    let on_error = move |error| {
        let _ = errors.send(error);
    };
    let mut streams = Vec::new();
    let mut resamplers = vec![None, None];
    let mut names = Vec::new();
    if source != Source::System {
        let dev = microphone(&host, device.as_deref())?;
        let (stream, rate) = open_stream(&dev, false, 0, tx.clone(), on_error.clone())?;
        if stop.load(Ordering::SeqCst) || Instant::now() >= deadline {
            return Ok(());
        }
        stream
            .play()
            .map_err(|error| format!("Couldn't start microphone: {error}"))?;
        streams.push(stream);
        resamplers[0] = (rate != SAMPLE_RATE)
            .then(|| LinearResampler::create(rate, SAMPLE_RATE))
            .flatten();
        names.push(
            dev.description()
                .map(|d| d.name().to_string())
                .unwrap_or_else(|_| "Microphone".into()),
        );
    }
    #[cfg(target_os = "macos")]
    let mut system_audio = None;
    if source != Source::Mic {
        if stop.load(Ordering::SeqCst) || Instant::now() >= deadline {
            return Ok(());
        }
        #[cfg(target_os = "macos")]
        {
            system_audio = Some(crate::apple_speech::SystemAudio::start_cancellable(
                app,
                tx.clone(),
                stop,
                deadline.saturating_duration_since(Instant::now()),
            )?);
        }
        #[cfg(not(target_os = "macos"))]
        {
            let (dev, loopback) = super::system_source(&host)?;
            let (stream, rate) = open_stream(&dev, loopback, 1, tx.clone(), on_error.clone())?;
            if stop.load(Ordering::SeqCst) || Instant::now() >= deadline {
                return Ok(());
            }
            stream
                .play()
                .map_err(|error| format!("Couldn't start system audio: {error}"))?;
            streams.push(stream);
            resamplers[1] = (rate != SAMPLE_RATE)
                .then(|| LinearResampler::create(rate, SAMPLE_RATE))
                .flatten();
        }
        names.push("System audio".into());
    }
    drop(tx);
    if stop.load(Ordering::SeqCst) || Instant::now() >= deadline {
        return Ok(());
    }
    let name = names.join(" + ");
    emit(app, id, "active", Some(&name), None, None, None);
    let mut mixer = Mixer::new(source.names().len());
    let mut level = Meter::default();
    let mut last = Instant::now();
    while !stop.load(Ordering::SeqCst) && Instant::now() < deadline {
        if let Ok(error) = error_rx.try_recv() {
            return Err(error);
        }
        #[cfg(target_os = "macos")]
        if let Some(capture) = &system_audio {
            capture.poll()?;
        }
        if let Ok((src, samples)) = rx.recv_timeout(Duration::from_millis(50)) {
            let samples = match &resamplers[src] {
                Some(r) => r.resample(&samples, false),
                None => samples,
            };
            mixer.push(if source == Source::System { 0 } else { src }, &samples);
            level.push(&super::mix(&mixer.pop_tracks()));
        }
        if last.elapsed() >= Duration::from_millis(100) {
            if let Some(rms) = level.take() {
                emit(app, id, "active", Some(&name), Some(rms), None, None);
            }
            last = Instant::now();
        }
    }
    // Dropping streams and the optional helper stops capture before the terminal event.
    drop(streams);
    #[cfg(target_os = "macos")]
    drop(system_audio);
    Ok(())
}

#[derive(Default)]
struct Meter {
    squares: f64,
    count: usize,
}
impl Meter {
    fn push(&mut self, samples: &[f32]) {
        for &sample in samples.iter().filter(|s| s.is_finite()) {
            self.squares += f64::from(sample).powi(2);
            self.count += 1;
        }
    }
    fn take(&mut self) -> Option<f32> {
        let result =
            (self.count > 0).then(|| (self.squares / self.count as f64).sqrt().min(1.0) as f32);
        *self = Self::default();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meter_distinguishes_unmeasured_from_measured_silence() {
        let mut meter = Meter::default();
        assert_eq!(meter.take(), None);
        meter.push(&[0., 0.]);
        assert_eq!(meter.take(), Some(0.));
        meter.push(&[0.5, -0.5, f32::NAN]);
        assert_eq!(meter.take(), Some(0.5));
        assert_eq!(meter.take(), None);
    }
    #[test]
    fn a_canceled_start_is_remembered_without_stopping_another_request() {
        let mut preview = Preview::default();
        let stop = Arc::new(AtomicBool::new(false));
        preview.active = Some(Check {
            id: "new".into(),
            stop: stop.clone(),
            thread: thread::spawn(|| {}),
        });
        assert!(preview.cancel("old").is_none());
        assert!(!stop.load(Ordering::SeqCst));
        assert!(preview.canceled.iter().any(|id| id == "old"));
        let check = preview.cancel("new").unwrap();
        assert!(check.stop.load(Ordering::SeqCst));
        check.thread.join().unwrap();
        for i in 0..40 {
            preview.cancel(&i.to_string());
        }
        assert_eq!(preview.canceled.len(), 32);
    }

    // Explicit opt-in: this can only open the isolated sink's monitor, never a host mic.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires an explicitly created BoardcastPreviewQA virtual sink and synthetic WAV"]
    fn virtual_input_meter_receives_samples_and_releases_capture() {
        use cpal::traits::HostTrait;
        let name = std::env::var("BOARDCAST_PREVIEW_TEST_DEVICE").unwrap();
        assert_eq!(name, "Monitor of BoardcastPreviewQA");
        let wav = std::env::var("BOARDCAST_PREVIEW_TEST_WAV").unwrap();
        let device = audio_host()
            .input_devices()
            .unwrap()
            .find(|device| {
                device
                    .description()
                    .is_ok_and(|description| description.name() == name)
            })
            .expect("isolated QA monitor exists");
        let (tx, rx) = mpsc::channel();
        let (stream, _) = open_stream(&device, false, 0, tx.clone(), |_| {}).unwrap();
        drop(tx);
        stream.play().unwrap();
        let mut playback = std::process::Command::new("paplay")
            .args(["--device=boardcast_preview_qa", &wav])
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut meter = Meter::default();
        let mut signal = false;
        while Instant::now() < deadline {
            if let Ok((_, samples)) = rx.recv_timeout(Duration::from_millis(100)) {
                meter.push(&samples);
            }
            if meter
                .take()
                .is_some_and(|level| level > 0.02 && level <= 1.0)
            {
                signal = true;
                break;
            }
        }
        drop(stream);
        // CPAL may deliver a final queued callback while its stream is dropping.
        let released_by = Instant::now() + Duration::from_secs(1);
        let mut disconnected = false;
        while Instant::now() < released_by {
            if matches!(
                rx.recv_timeout(Duration::from_millis(50)),
                Err(mpsc::RecvTimeoutError::Disconnected)
            ) {
                disconnected = true;
                break;
            }
        }
        // Reap playback before any assertion can unwind and remove its virtual sink.
        let playback_ok = playback.wait().unwrap().success();
        assert!(
            disconnected,
            "capture callback must disconnect after stream release"
        );
        assert!(playback_ok);
        assert!(
            signal,
            "synthetic signal produced a real nonzero meter reading"
        );
    }
}
