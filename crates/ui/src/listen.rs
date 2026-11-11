//! One capture at a time; each source keeps its own detector, language and meeting stamp.
use crate::pipeline::{Event, Speech};
use anyhow::{bail, ensure, Result};
use boardcast_audio::{capture, play_wav, system::{AutoSpeech, SystemSpeech}, Captured, Ear, Recognizers, Sentence};
use serde::Serialize;
use std::collections::{hash_map::RandomState, VecDeque};
use std::hash::BuildHasher;
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
    pub failure_stage: Option<&'static str>,
    pub samples: u64,
    pub last_received_ms: Option<u128>,
    pub peak: f32,
    pub queued_samples: usize,
    pub max_queued_samples: usize,
    pub finals: usize,
    pub queue_ms: VecDeque<u128>,
    pub asr_ms: VecDeque<u128>,
    pub ui_receive_ms: VecDeque<u128>,
    pub audio_end_to_final_ms: VecDeque<u128>,
}

pub fn keep(values: &mut VecDeque<u128>, value: u128) {
    if values.len() == 128 { values.pop_front(); }
    values.push_back(value);
}

#[derive(Serialize)]
pub struct Report {
    pub id: u64,
    pub key: String,
    pub source: String,
    pub requested_apple: bool,
    pub requested_languages: [String; 2],
    pub phase: &'static str,
    pub failure_stage: Option<&'static str>,
    pub failure_source: Option<&'static str>,
    pub tracks: [Track; 2],
    pub elapsed_ms: u128,
    #[serde(skip)]
    pub started: Instant,
}

impl Report {
    pub fn new(id: u64, source: &str, speech: &Speech) -> Self {
        Self { id, key: format!("{:016x}", RandomState::new().hash_one(id)), source: source.into(), requested_apple: speech.system,
            requested_languages: [speech.language.clone(), speech.system_language.clone()],
            phase: "preparing", failure_stage: None, failure_source: None, tracks: Default::default(), elapsed_ms: 0, started: Instant::now() }
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
    pub wavs: [Option<PathBuf>; 2],
    pub done: Option<Sender<()>>,
}

/// The result carries the capture's meeting, never the meeting selected when recognition finishes.
#[derive(Clone)]
pub struct Heard {
    pub capture: u64,
    pub key: String,
    pub source: usize,
    pub ready: Instant,
    /// Seconds from this capture's common origin, shared by both sources.
    pub captured: [Option<f64>; 2],
    pub report: Arc<Mutex<Report>>,
}

fn ear_for(models: &Path, speech: &Speech, locale: &str, stop: Arc<AtomicBool>) -> Result<(Ear, String, Option<String>, Option<String>)> {
    let system = |helper: &Path| -> Result<(Ear, String, Option<String>, Option<String>)> {
        if locale == "detect" {
            let recognizer = AutoSpeech::load(models, helper, stop.clone())?;
            return Ok((Ear::with(models, Recognizers::system(Box::new(recognizer)))?,
                crate::i18n::tr("Apple (detecting language)", "Apple（正在探测语言）").into(), None, None));
        }
        let recognizer = SystemSpeech::start_with_stop(helper, locale, stop.clone())?;
        let with = format!("Apple {}", if recognizer.engine == "analyzer" { "SpeechAnalyzer" } else { "SFSpeechRecognizer" });
        let locale = recognizer.locale.clone();
        Ok((Ear::with(models, Recognizers::system(Box::new(recognizer)))?, with, Some(locale), None))
    };
    let mut note = None;
    if let Some(helper) = speech.helper.as_deref().filter(|_| speech.system) {
        match system(helper) {
            Ok(ear) => return Ok(ear),
            Err(e) if locale == "detect" => return Err(e),
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

struct Frame { source: usize, samples: Vec<f32>, start: Option<Instant>, arrived: Instant }

enum Status { Ready(usize), Failed { source: usize, stage: &'static str, error: anyhow::Error } }

fn send_frame(tx: &SyncSender<Frame>, report: &Arc<Mutex<Report>>, source: usize, captured: Captured) -> Result<()> {
    let Captured { samples, start } = captured;
    ensure!(!samples.is_empty() && samples.len() <= 16_000 && samples.iter().all(|s| s.is_finite()), "invalid captured audio");
    let n = samples.len();
    let mut report = report.lock().unwrap();
    let elapsed = report.started.elapsed().as_millis();
    let track = &mut report.tracks[source];
    track.samples += n as u64;
    track.last_received_ms = Some(elapsed);
    track.peak = samples.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
    track.queued_samples += n;
    if tx.try_send(Frame { source, samples, start, arrived: Instant::now() }).is_err() {
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
    if active[1] && job.speech.helper.is_none() {
        let mut report = job.report.lock().unwrap();
        report.failure_source = Some(SOURCES[1]);
        report.tracks[1].failure_stage = Some("capture");
        bail!("system audio capture needs the Mac desktop helper");
    }
    let mut ears: [Option<Ear>; 2] = [None, None];
    let mut names = vec![];
    let mut notes = vec![];
    for source in 0..2 {
        if !active[source] { continue; }
        if job.stop.load(Ordering::Relaxed) { return Ok(()); }
        let language = if source == 0 { &job.speech.language } else { &job.speech.system_language };
        let (ear, with, locale, note) = ear_for(models, &job.speech, language, job.stop.clone()).map_err(|error| {
            if !job.stop.load(Ordering::Relaxed) {
                let mut report = job.report.lock().unwrap();
                report.failure_source = Some(SOURCES[source]);
                report.tracks[source].failure_stage = Some("preparing");
            }
            error
        })?;
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
                        ear.feed_at(&frame.samples, frame.start, &mut |sentence| heard(job, source, sentence, events));
                        refresh_identity(job, source, &ear, events);
                        if let Some(error) = ear.error() { bail!("{error}"); }
                    }
                    ear.finish(&mut |sentence| heard(job, source, sentence, events));
                    refresh_identity(job, source, &ear, events);
                    if let Some(error) = ear.error() { bail!("{error}"); }
                    Ok(())
                })();
                if let Err(error) = result { let _ = hearing_status.send(Status::Failed { source, stage: "recognition", error }); }
            });
            let (status, report, stop, events) = (status.clone(), job.report.clone(), job.stop.clone(), events.clone());
            let input = job.input.clone();
            let helper = job.speech.helper.clone();
            let wav = job.wavs[source].clone();
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
                if let Err(error) = result { if !stop.load(Ordering::Relaxed) { let _ = status.send(Status::Failed { source, stage: "capture", error }); } }
            });
        }
        drop(status);
        let mut started = [false; 2];
        let mut failure = None;
        // A capture error can arrive after the recognizer has drained; wait for both senders to close.
        while let Ok(status) = statuses.recv() {
            match status {
                Status::Ready(source) => {
                    started[source] = true;
                    if failure.is_none() && (0..2).all(|i| !active[i] || started[i]) {
                        job.report.lock().unwrap().phase = "listening";
                        let _ = events.send(Event::CaptureReady(job.id));
                    }
                }
                Status::Failed { source, stage, error } => {
                    let mut report = job.report.lock().unwrap();
                    report.failure_source.get_or_insert(SOURCES[source]);
                    report.tracks[source].failure_stage.get_or_insert(stage);
                    failure.get_or_insert(error);
                    job.stop.store(true, Ordering::Relaxed);
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

fn refresh_identity(job: &Job, source: usize, ear: &Ear, events: &Sender<Event>) {
    let Some((engine, locale)) = ear.identity() else { return };
    let name = format!("Apple {}", if engine == "analyzer" { "SpeechAnalyzer" } else { "SFSpeechRecognizer" });
    let mut report = job.report.lock().unwrap();
    let track = &mut report.tracks[source];
    if track.engine.as_deref() == Some(&name) && track.locale.as_deref() == Some(locale) { return; }
    track.engine = Some(name);
    track.locale = Some(locale.into());
    let names = report.tracks.iter().enumerate().filter_map(|(i, track)| {
        track.engine.as_ref().map(|engine| format!("{}: {engine}{}", SOURCES[i], track.locale.as_ref().map_or(String::new(), |l| format!(" ({l})"))))
    }).collect::<Vec<_>>();
    drop(report);
    let _ = events.send(Event::Hearing { with: names.join(" · "), note: None });
}

fn heard(job: &Job, source: usize, sentence: Sentence, events: &Sender<Event>) {
    let ready = Instant::now();
    let (key, captured) = {
        let mut report = job.report.lock().unwrap();
        report.tracks[source].finals += 1;
        keep(&mut report.tracks[source].asr_ms, sentence.asr_ms);
        if let Some(elapsed) = sentence.captured[1].and_then(|end| ready.checked_duration_since(end)) {
            keep(&mut report.tracks[source].audio_end_to_final_ms, elapsed.as_millis());
        }
        let captured = sentence.captured.map(|time| time.map(|time| {
            if time >= report.started { (time - report.started).as_secs_f64() }
            else { -(report.started - time).as_secs_f64() }
        }));
        (report.key.clone(), captured)
    };
    let audio = Heard { capture: job.id, key, source, ready, captured, report: job.report.clone() };
    let _ = events.send(Event::Spoken { meeting: job.meeting, sentence, audio });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_setup_keeps_the_source_without_copying_the_error_or_reporting_twice() {
        for (helper, source, stage) in [(None, "system", "capture"), (Some(PathBuf::from("private-helper-path")), "microphone", "preparing")] {
            let speech = Speech { helper, system: true, language: "detect".into(), system_language: "detect".into() };
            let report = Arc::new(Mutex::new(Report::new(1, "both", &speech)));
            let (jobs, receive_jobs) = channel();
            let (events, receive_events) = channel();
            jobs.send(Job { id: 1, meeting: 0, input: None, source: "both".into(), speech,
                stop: Arc::new(AtomicBool::new(false)), report: report.clone(), wavs: [None, None], done: None }).unwrap();
            drop(jobs);
            run_jobs(receive_jobs, PathBuf::from("private-missing-model-directory"), events);
            let events: Vec<_> = receive_events.try_iter().collect();
            assert_eq!(events.iter().filter(|e| matches!(e, Event::Failed(_))).count(), 1);
            assert_eq!(events.iter().filter(|e| matches!(e, Event::CaptureStopped(1))).count(), 1);
            let value = serde_json::to_value(&*report.lock().unwrap()).unwrap();
            assert_eq!(value["phase"], "failed");
            assert_eq!(value["failure_source"], source);
            for (i, name) in SOURCES.iter().enumerate() {
                assert_eq!(value["tracks"][i]["failure_stage"].as_str(), (*name == source).then_some(stage));
            }
            assert!(!value.to_string().contains("private-"));
        }
    }

    #[test]
    fn a_cancelled_queued_capture_has_no_failure_source() {
        let speech = Speech { helper: None, system: true, language: "detect".into(), system_language: "detect".into() };
        let report = Arc::new(Mutex::new(Report::new(1, "both", &speech)));
        let (jobs, receive_jobs) = channel();
        let (events, receive_events) = channel();
        jobs.send(Job { id: 1, meeting: 0, input: None, source: "both".into(), speech,
            stop: Arc::new(AtomicBool::new(true)), report: report.clone(), wavs: [None, None], done: None }).unwrap();
        drop(jobs);
        run_jobs(receive_jobs, PathBuf::from("private-missing-model-directory"), events);
        assert_eq!(receive_events.try_iter().filter(|e| matches!(e, Event::Failed(_))).count(), 0);
        let report = report.lock().unwrap();
        assert_eq!(report.phase, "stopped");
        assert!(report.failure_source.is_none() && report.failure_stage.is_none());
        assert!(report.tracks.iter().all(|t| t.failure_stage.is_none()));
    }

    #[test]
    fn missing_auto_detector_asks_for_a_manual_language() {
        let models = std::env::temp_dir().join(format!("boardcast-missing-models-{}", std::process::id()));
        assert!(!models.exists());
        let speech = Speech { helper: Some("unused-helper".into()), system: true, language: "detect".into(), system_language: "auto".into() };
        let error = ear_for(&models, &speech, "detect", Arc::new(AtomicBool::new(false))).err().unwrap();
        assert!(error.to_string().contains("multilingual speech package"), "{error:#}");
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "needs BOARDCAST_VOICE_DIR, BOARDCAST_AUTO_AUDIO and Python 3; uses no audio device"]
    fn both_sources_can_recognize_while_the_other_is_waiting() {
        use std::os::unix::fs::PermissionsExt;
        let models = PathBuf::from(std::env::var_os("BOARDCAST_VOICE_DIR").expect("voice models"));
        let audio = PathBuf::from(std::env::var_os("BOARDCAST_AUTO_AUDIO").expect("verified three-language WAVs"));
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
    event = {'type':'text','text':'Recognized by ' + locale}
    if (root / 'barrier').exists():
        other = root / ('ja-JP' if locale == 'en-US' else 'en-US')
        # Auto gathers opening speech while a manual-language peer can already be waiting here.
        until = time.monotonic() + 15
        while not other.exists() and time.monotonic() < until: time.sleep(0.01)
        if not other.exists(): event = {'type':'error','message':'The other source was blocked by this recognition request.'}
    if locale == 'ja-JP' and (root / 'fail').exists(): event = {'type':'error','message':'injected recognition failure'}
    print(json.dumps(event), flush=True)
(root / (locale + '.closed')).touch()
"#).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
        for (languages, locales, files, fail) in [
            (["en-US", "ja-JP"], ["en-US", "ja-JP"], ["en-ami-00", "en-ami-00"], ""),
            (["detect", "ja-JP"], ["en-US", "ja-JP"], ["en-ami-00", "en-ami-00"], ""),
            (["detect", "detect"], ["zh-CN", "ja-JP"], ["zh-alimeeting-00", "ja-fleurs-00"], ""),
            (["detect", "detect"], ["ja-JP", "zh-CN"], ["ja-fleurs-00", "zh-alimeeting-00"], ""),
            (["detect", "ja-JP"], ["en-US", "ja-JP"], ["en-ami-00", "en-ami-00"], "recognition"),
            (["en-US", "ja-JP"], ["en-US", "ja-JP"], ["en-ami-00", "private-missing-audio"], "capture"),
        ] {
            // Only equal-duration inputs use the barrier; separate utterances can end at different times.
            if files[0] == files[1] { std::fs::write(dir.join("barrier"), "").unwrap(); }
            else { let _ = std::fs::remove_file(dir.join("barrier")); }
            let speech = Speech { helper: Some(helper.clone()), system: true, language: languages[0].into(), system_language: languages[1].into() };
            if fail == "recognition" { std::fs::write(dir.join("fail"), "").unwrap(); }
            else { let _ = std::fs::remove_file(dir.join("fail")); }
            let report = Arc::new(Mutex::new(Report::new(9, "both", &speech)));
            let stop = Arc::new(AtomicBool::new(false));
            let job = Job { id: 9, meeting: 17, input: None, source: "both".into(), speech: speech.clone(),
                stop: stop.clone(), report: report.clone(), wavs: files.map(|file| Some(audio.join(format!("{file}.wav")))), done: None };
            let (events, receive) = channel();
            let (jobs, receive_jobs) = channel();
            jobs.send(job).unwrap();
            drop(jobs);
            run_jobs(receive_jobs, models.clone(), events);
            let mut spoken = [0; 2];
            let mut errors = vec![];
            let mut stopped = 0;
            for event in receive.try_iter() {
                match event {
                    Event::Spoken { meeting, audio, sentence } => {
                        assert_eq!(meeting, 17);
                        assert_eq!(audio.capture, 9);
                        assert_eq!(sentence.text, format!("Recognized by {}", locales[audio.source]));
                        assert_eq!(audio.captured, [None, None], "file input has no native capture clock");
                        spoken[audio.source] += 1;
                    }
                    Event::Failed(error) => errors.push(error),
                    Event::CaptureStopped(9) => stopped += 1,
                    _ => {}
                }
            }
            assert_eq!(stopped, 1);
            if !fail.is_empty() {
                assert_eq!(errors.len(), 1, "{errors:?}");
                if fail == "recognition" { assert!(errors[0].contains("injected recognition failure")); }
                let report = report.lock().unwrap();
                assert_eq!(report.phase, "failed");
                assert_eq!(report.failure_stage, Some("capture-or-recognition"));
                assert_eq!(report.failure_source, Some("system"));
                assert_eq!(report.tracks[1].failure_stage, Some(fail));
                let text = serde_json::to_string(&*report).unwrap();
                assert!(!text.contains("private-missing-audio") && !text.contains("injected recognition failure"));
            } else {
                assert!(errors.is_empty(), "{languages:?} / {files:?}: {errors:?}");
                assert!(spoken.iter().all(|n| *n > 0), "{spoken:?}");
                let report = report.lock().unwrap();
                assert_eq!(report.phase, "stopped");
                assert_eq!(report.requested_languages, languages);
                assert!(report.failure_source.is_none());
                for (track, locale) in report.tracks.iter().zip(locales) {
                    assert!(track.failure_stage.is_none());
                    assert_eq!(track.locale.as_deref(), Some(locale));
                    assert_eq!(track.queued_samples, 0);
                    assert!(track.samples > 0 && track.finals > 0);
                    assert!(track.audio_end_to_final_ms.is_empty());
                }
            }
            assert!(stop.load(Ordering::Relaxed));
            for locale in locales {
                assert!(dir.join(format!("{locale}.closed")).exists(), "helper did not shut down through EOF");
                std::fs::remove_file(dir.join(format!("{locale}.closed"))).unwrap();
                let _ = std::fs::remove_file(dir.join(locale));
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn tracks_stay_separate_and_overflow_is_an_error() {
        let speech = Speech { helper: None, system: false, language: "zh-CN".into(), system_language: "ja-JP".into() };
        let report = Arc::new(Mutex::new(Report::new(1, "both", &speech)));
        let (tx, rx) = sync_channel(2);
        let start = Instant::now();
        send_frame(&tx, &report, 0, Captured { samples: vec![0.25; 320], start: Some(start) }).unwrap();
        send_frame(&tx, &report, 1, Captured { samples: vec![-0.5; 1600], start: Some(start + Duration::from_secs(2)) }).unwrap();
        assert!(send_frame(&tx, &report, 0, Captured { samples: vec![0.0; 320], start: None }).is_err());
        assert!(send_frame(&tx, &report, 0, Captured { samples: vec![f32::NAN], start: None }).is_err());
        assert!(send_frame(&tx, &report, 0, Captured { samples: vec![], start: None }).is_err());
        let first = rx.recv().unwrap(); let second = rx.recv().unwrap();
        assert_eq!((first.source, first.samples[0]), (0, 0.25));
        assert_eq!((second.source, second.samples[0]), (1, -0.5));
        assert_eq!(second.start.unwrap() - first.start.unwrap(), Duration::from_secs(2));
        let report = report.lock().unwrap();
        assert_eq!(report.tracks[0].queued_samples, 320);
        assert_eq!(report.tracks[1].queued_samples, 1600);
        assert_eq!(report.requested_languages, ["zh-CN", "ja-JP"]);
    }

    #[test]
    fn final_times_share_the_capture_origin_and_keep_untimed_input_unknown() {
        let speech = Speech { helper: None, system: false, language: "detect".into(), system_language: "auto".into() };
        let mut report = Report::new(3, "both", &speech);
        assert_ne!(report.key, Report::new(3, "both", &speech).key, "capture IDs can repeat after restarting the app");
        report.started = Instant::now() - Duration::from_secs(10);
        let origin = report.started;
        let job = Job { id: 3, meeting: 7, input: None, source: "both".into(), speech,
            stop: Arc::new(AtomicBool::new(false)), report: Arc::new(Mutex::new(report)), wavs: [None, None], done: None };
        let (events, receive) = channel();
        for (source, times, expected) in [
            (0, [Some(origin - Duration::from_secs(1)), Some(origin)], [Some(-1.0), Some(0.0)]),
            (1, [Some(origin + Duration::from_secs(2)), Some(origin + Duration::from_secs(3))], [Some(2.0), Some(3.0)]),
            (0, [None, None], [None, None]),
        ] {
            heard(&job, source, Sentence { text: "test".into(), voiceprint: None, asr_ms: 5, ended: Instant::now(),
                start: 0.0, seconds: 1.0, captured: times, engine: boardcast_audio::Engine::SenseVoice }, &events);
            let Event::Spoken { meeting, sentence, audio } = receive.recv().unwrap() else { panic!("missing sentence") };
            assert_eq!((meeting, audio.capture, audio.source), (7, 3, source));
            assert_eq!(audio.key, job.report.lock().unwrap().key);
            assert_eq!(audio.captured, expected);
            assert_eq!(sentence.start, 0.0);
            if let Some(end) = times[1] {
                assert_eq!(job.report.lock().unwrap().tracks[source].audio_end_to_final_ms.back(), Some(&(audio.ready - end).as_millis()));
            }
        }
        let report = job.report.lock().unwrap();
        assert_eq!(report.tracks[0].audio_end_to_final_ms.len(), 1);
        assert_eq!(report.tracks[1].audio_end_to_final_ms.len(), 1);
    }

    #[test]
    fn metrics_are_bounded() {
        let mut values = VecDeque::new();
        for i in 0..300 { keep(&mut values, i); }
        assert_eq!(values.len(), 128);
        assert_eq!(values.back(), Some(&299));
    }
}
