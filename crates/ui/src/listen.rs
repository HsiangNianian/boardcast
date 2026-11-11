//! One capture at a time; each source keeps its own detector, language and meeting stamp.
use crate::pipeline::{Event, Speech};
use anyhow::{bail, ensure, Result};
use boardcast_audio::{capture, play_wav, system::SystemSpeech, Ear, Recognizers, Sentence};
use serde::Serialize;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const SOURCES: [&str; 2] = ["microphone", "system"];

#[derive(Default, Serialize)]
pub struct Track {
    pub engine: Option<String>,
    pub locale: Option<String>,
    pub samples: u64,
    pub last_received_ms: Option<u128>,
    pub peak: f32,
    pub queued_samples: usize,
    pub max_queued_samples: usize,
    pub finals: usize,
    pub queue_ms: VecDeque<u128>,
    pub asr_ms: VecDeque<u128>,
    pub ui_receive_ms: VecDeque<u128>,
}

pub fn keep(values: &mut VecDeque<u128>, value: u128) {
    if values.len() == 128 { values.pop_front(); }
    values.push_back(value);
}

#[derive(Serialize)]
pub struct Report {
    pub id: u64,
    pub source: String,
    pub requested_apple: bool,
    pub requested_languages: [String; 2],
    pub phase: &'static str,
    pub failure_stage: Option<&'static str>,
    pub tracks: [Track; 2],
    pub elapsed_ms: u128,
    #[serde(skip)]
    pub started: Instant,
}

impl Report {
    pub fn new(id: u64, source: &str, speech: &Speech) -> Self {
        Self { id, source: source.into(), requested_apple: speech.system,
            requested_languages: [speech.language.clone(), speech.system_language.clone()],
            phase: "preparing", failure_stage: None, tracks: Default::default(), elapsed_ms: 0, started: Instant::now() }
    }
}

pub struct Job {
    pub id: u64,
    pub meeting: u64,
    pub input: Option<String>,
    pub source: String,
    pub speech: Speech,
    pub stop: Arc<AtomicBool>,
    pub report: Arc<Mutex<Report>>,
    pub wav: Option<PathBuf>,
    pub done: Option<Sender<()>>,
}

/// The result carries the capture's meeting, never the meeting selected when recognition finishes.
#[derive(Clone)]
pub struct Heard {
    pub capture: u64,
    pub source: usize,
    pub ready: Instant,
    pub report: Arc<Mutex<Report>>,
}

fn ear_for(models: &Path, speech: &Speech, locale: &str, stop: Arc<AtomicBool>) -> Result<(Ear, String, Option<String>, Option<String>)> {
    let system = |helper: &Path| -> Result<(Ear, String, Option<String>, Option<String>)> {
        let recognizer = SystemSpeech::start_with_stop(helper, locale, stop.clone())?;
        let with = format!("Apple {}", if recognizer.engine == "analyzer" { "SpeechAnalyzer" } else { "SFSpeechRecognizer" });
        let locale = recognizer.locale.clone();
        Ok((Ear::with(models, Recognizers::system(Box::new(recognizer)))?, with, Some(locale), None))
    };
    let mut note = None;
    if let Some(helper) = speech.helper.as_deref().filter(|_| speech.system) {
        match system(helper) {
            Ok(ear) => return Ok(ear),
            Err(e) if !stop.load(Ordering::Relaxed) => note = Some(format!("{e:#}")),
            Err(e) => return Err(e),
        }
    }
    let recognizers = Recognizers::load(models);
    if recognizers.is_empty() {
        if let Some(helper) = speech.helper.as_deref().filter(|_| !speech.system) { return system(helper); }
    }
    let with = recognizers.names().join(" + ");
    Ok((Ear::with(models, recognizers)?, with, None, note))
}

struct Frame { source: usize, samples: Vec<f32>, arrived: Instant }

enum Status { Ready(usize), Failed(anyhow::Error), Finished }

fn send_frame(tx: &SyncSender<Frame>, report: &Arc<Mutex<Report>>, source: usize, samples: Vec<f32>) -> Result<()> {
    ensure!(!samples.is_empty() && samples.len() <= 16_000 && samples.iter().all(|s| s.is_finite()), "invalid captured audio");
    let n = samples.len();
    let mut report = report.lock().unwrap();
    let elapsed = report.started.elapsed().as_millis();
    let track = &mut report.tracks[source];
    track.samples += n as u64;
    track.last_received_ms = Some(elapsed);
    track.peak = samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
    track.queued_samples += n;
    if tx.try_send(Frame { source, samples, arrived: Instant::now() }).is_err() {
        track.queued_samples -= n;
        bail!("Speech recognition cannot keep up; capture stopped. Try a smaller speech model or one source.");
    }
    track.max_queued_samples = track.max_queued_samples.max(track.queued_samples);
    Ok(())
}

pub fn run_jobs(jobs: Receiver<Job>, models: PathBuf, events: Sender<Event>) {
    // Returning from run joins every capture thread and drops both helpers before the next job starts.
    for job in jobs {
        let result = run(&job, &models, &events);
        {
            let mut report = job.report.lock().unwrap();
            report.elapsed_ms = report.started.elapsed().as_millis();
            if result.is_err() && !job.stop.load(Ordering::Relaxed) { report.failure_stage = Some(report.phase); }
            report.phase = if report.failure_stage.is_some() { "failed" } else { "stopped" };
        }
        if let Err(error) = result {
            if job.report.lock().unwrap().failure_stage.is_some() {
                let message = if error.to_string().contains("SYSTEM_AUDIO_PERMISSION") {
                    crate::i18n::tr("Allow boardcast in System Settings → Privacy & Security → Screen & System Audio Recording, then start again.", "请在系统设置 → 隐私与安全 → 屏幕与系统音频录制中允许 boardcast，然后重新开始。").to_string()
                } else { format!("{error:#}") };
                let _ = events.send(Event::Failed(message));
            }
        }
        job.stop.store(true, Ordering::Relaxed);
        let _ = events.send(Event::CaptureStopped(job.id));
        if let Some(done) = job.done { let _ = done.send(()); }
    }
}

fn run(job: &Job, models: &Path, events: &Sender<Event>) -> Result<()> {
    if job.stop.load(Ordering::Relaxed) { return Ok(()); }
    let active = match job.source.as_str() {
        "microphone" => [true, false], "system" => [false, true], "both" => [true, true],
        _ => bail!("unknown capture source"),
    };
    if active[1] { ensure!(job.speech.helper.is_some(), "system audio capture needs the Mac desktop helper"); }
    let mut ears: [Option<Ear>; 2] = [None, None];
    let mut names = vec![];
    let mut notes = vec![];
    for source in 0..2 {
        if !active[source] { continue; }
        if job.stop.load(Ordering::Relaxed) { return Ok(()); }
        let language = if source == 0 { &job.speech.language } else { &job.speech.system_language };
        let (ear, with, locale, note) = ear_for(models, &job.speech, language, job.stop.clone())?;
        names.push(format!("{}: {with}{}", SOURCES[source], locale.as_ref().map_or(String::new(), |l| format!(" ({l})"))));
        if let Some(note) = note { notes.push(format!("{}: {note}", SOURCES[source])); }
        let mut report = job.report.lock().unwrap();
        report.tracks[source].engine = Some(with);
        report.tracks[source].locale = locale;
        ears[source] = Some(ear);
    }
    let _ = events.send(Event::Hearing { with: names.join(" · "), note: (!notes.is_empty()).then(|| notes.join("\n")) });
    if job.stop.load(Ordering::Relaxed) { return Ok(()); }
    job.report.lock().unwrap().phase = "starting-capture";
    let (status, statuses) = channel::<Status>();
    let mic_ready = Arc::new(AtomicBool::new(!active[0]));
    let result = thread::scope(|scope| -> Result<()> {
        for source in 0..2 {
            if !active[source] { continue; }
            let (frames, receive) = sync_channel::<Frame>(300);
            let mut ear = ears[source].take().unwrap();
            let hearing_status = status.clone();
            // Each source can wait for its recognizer without holding up the other source.
            scope.spawn(move || {
                let result = (|| -> Result<()> {
                    for frame in receive {
                        ensure!(frame.source == source, "audio reached the wrong source's recognizer");
                        {
                            let mut report = job.report.lock().unwrap();
                            let track = &mut report.tracks[source];
                            track.queued_samples -= frame.samples.len();
                            keep(&mut track.queue_ms, frame.arrived.elapsed().as_millis());
                        }
                        let _ = events.send(Event::Level(frame.samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()))));
                        ear.feed(&frame.samples, &mut |sentence| heard(job, source, sentence, events));
                        if let Some(error) = ear.error() { bail!("{error}"); }
                    }
                    ear.finish(&mut |sentence| heard(job, source, sentence, events));
                    if let Some(error) = ear.error() { bail!("{error}"); }
                    Ok(())
                })();
                if let Err(error) = result { let _ = hearing_status.send(Status::Failed(error)); }
                let _ = hearing_status.send(Status::Finished);
            });
            let (status, report, stop, events) = (status.clone(), job.report.clone(), job.stop.clone(), events.clone());
            let input = job.input.clone();
            let helper = job.speech.helper.clone();
            let wav = job.wav.clone();
            let mic_ready = mic_ready.clone();
            scope.spawn(move || {
                // Activate the microphone before asking ScreenCaptureKit for permission.
                while source == 1 && !mic_ready.load(Ordering::Relaxed) && !stop.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(20));
                }
                if stop.load(Ordering::Relaxed) { return; }
                let sending_stop = stop.clone();
                let send = move |samples| {
                    if sending_stop.load(Ordering::Relaxed) { bail!("audio cancelled"); }
                    send_frame(&frames, &report, source, samples)
                };
                let ready = || {
                    if source == 0 { mic_ready.store(true, Ordering::Relaxed); }
                    let _ = status.send(Status::Ready(source));
                };
                let result = if let Some(wav) = wav {
                    ready();
                    play_wav(&wav, send)
                } else if source == 0 {
                    let warning = move |_: &str| {
                        let _ = events.send(Event::Failed(crate::i18n::tr("The chosen input isn't connected, so the default one is listening.", "选择的输入设备没有连接，正在用默认设备收音。").into()));
                    };
                    capture(&stop, send, input.as_deref(), warning, ready)
                } else {
                    boardcast_audio::system::capture(helper.as_deref().unwrap(), &stop, send, ready)
                };
                if let Err(error) = result { if !stop.load(Ordering::Relaxed) { let _ = status.send(Status::Failed(error)); } }
            });
        }
        drop(status);
        let mut started = [false; 2];
        let mut finished = 0;
        let mut failure = None;
        while finished < active.iter().filter(|on| **on).count() {
            match statuses.recv() {
                Ok(Status::Ready(source)) => {
                    started[source] = true;
                    if failure.is_none() && (0..2).all(|i| !active[i] || started[i]) {
                        job.report.lock().unwrap().phase = "listening";
                        let _ = events.send(Event::CaptureReady(job.id));
                    }
                }
                Ok(Status::Finished) => finished += 1,
                Ok(Status::Failed(error)) => {
                    failure.get_or_insert(error);
                    job.stop.store(true, Ordering::Relaxed);
                }
                Err(error) => {
                    failure.get_or_insert(error.into());
                    break;
                }
            }
        }
        if failure.is_some() { job.report.lock().unwrap().failure_stage = Some("capture-or-recognition"); }
        // Every exit, including errors, releases the other source before joining it.
        job.stop.store(true, Ordering::Relaxed);
        failure.map_or(Ok(()), Err)
    });
    result
}

fn heard(job: &Job, source: usize, sentence: Sentence, events: &Sender<Event>) {
    {
        let mut report = job.report.lock().unwrap();
        report.tracks[source].finals += 1;
        keep(&mut report.tracks[source].asr_ms, sentence.asr_ms);
    }
    let audio = Heard { capture: job.id, source, ready: Instant::now(), report: job.report.clone() };
    let _ = events.send(Event::Spoken { meeting: job.meeting, sentence, audio });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    #[ignore = "needs BOARDCAST_VOICE_DIR, BOARDCAST_AUDIO_TEST_WAV and Python 3; uses no audio device"]
    fn both_sources_can_recognize_while_the_other_is_waiting() {
        use std::os::unix::fs::PermissionsExt;
        let models = PathBuf::from(std::env::var_os("BOARDCAST_VOICE_DIR").expect("voice models"));
        let wav = PathBuf::from(std::env::var_os("BOARDCAST_AUDIO_TEST_WAV").expect("16 kHz speech WAV"));
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("boardcast-dual-test-{}-{stamp}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let helper = dir.join("speech");
        std::fs::write(&helper, r#"#!/usr/bin/env python3
import json, pathlib, struct, sys, time
root = pathlib.Path(__file__).parent
locale = sys.argv[2]
print(json.dumps({'type':'ready','engine':'fake','locale':locale}), flush=True)
while True:
    header = sys.stdin.buffer.read(4)
    if len(header) < 4: break
    n, = struct.unpack('<I', header)
    if not n: break
    if len(sys.stdin.buffer.read(n * 4)) != n * 4: break
    (root / locale).touch()
    other = root / ('ja-JP' if locale == 'en-US' else 'en-US')
    until = time.monotonic() + 3
    while not other.exists() and time.monotonic() < until: time.sleep(0.01)
    event = {'type':'text','text':'The two sources both reached recognition.'} if other.exists() else {'type':'error','message':'The other source was blocked by this recognition request.'}
    if locale == 'ja-JP' and (root / 'fail').exists(): event = {'type':'error','message':'injected recognition failure'}
    print(json.dumps(event), flush=True)
(root / (locale + '.closed')).touch()
"#).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        let speech = Speech { helper: Some(helper), system: true, language: "en-US".into(), system_language: "ja-JP".into() };
        for fail in [false, true] {
            if fail { std::fs::write(dir.join("fail"), "").unwrap(); }
            let report = Arc::new(Mutex::new(Report::new(9, "both", &speech)));
            let job = Job { id: 9, meeting: 17, input: None, source: "both".into(), speech: speech.clone(),
                stop: Arc::new(AtomicBool::new(false)), report, wav: Some(wav.clone()), done: None };
            let (events, receive) = channel();
            let result = run(&job, &models, &events);
            let mut spoken = [0; 2];
            for event in receive.try_iter() {
                if let Event::Spoken { meeting, audio, .. } = event {
                    assert_eq!(meeting, 17);
                    assert_eq!(audio.capture, 9);
                    spoken[audio.source] += 1;
                }
            }
            if fail {
                assert!(result.unwrap_err().to_string().contains("injected recognition failure"));
                assert_eq!(job.report.lock().unwrap().failure_stage, Some("capture-or-recognition"));
            } else {
                assert!(result.is_ok(), "{result:?}");
                assert!(spoken.iter().all(|n| *n > 0), "{spoken:?}");
            }
            assert!(job.stop.load(Ordering::Relaxed));
            for locale in ["en-US", "ja-JP"] {
                assert!(dir.join(format!("{locale}.closed")).exists(), "helper did not shut down through EOF");
                std::fs::remove_file(dir.join(format!("{locale}.closed"))).unwrap();
                std::fs::remove_file(dir.join(locale)).unwrap();
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tracks_stay_separate_and_overflow_is_an_error() {
        let speech = Speech { helper: None, system: false, language: "zh-CN".into(), system_language: "ja-JP".into() };
        let report = Arc::new(Mutex::new(Report::new(1, "both", &speech)));
        let (tx, rx) = sync_channel(2);
        send_frame(&tx, &report, 0, vec![0.25; 320]).unwrap();
        send_frame(&tx, &report, 1, vec![-0.5; 1600]).unwrap();
        assert!(send_frame(&tx, &report, 0, vec![0.0; 320]).is_err());
        assert!(send_frame(&tx, &report, 0, vec![f32::NAN]).is_err());
        assert!(send_frame(&tx, &report, 0, vec![]).is_err());
        let first = rx.recv().unwrap(); let second = rx.recv().unwrap();
        assert_eq!((first.source, first.samples[0]), (0, 0.25));
        assert_eq!((second.source, second.samples[0]), (1, -0.5));
        let report = report.lock().unwrap();
        assert_eq!(report.tracks[0].queued_samples, 320);
        assert_eq!(report.tracks[1].queued_samples, 1600);
        assert_eq!(report.requested_languages, ["zh-CN", "ja-JP"]);
    }
    #[test]
    fn metrics_are_bounded() {
        let mut values = VecDeque::new();
        for i in 0..300 { keep(&mut values, i); }
        assert_eq!(values.len(), 128);
        assert_eq!(values.back(), Some(&299));
    }
}
