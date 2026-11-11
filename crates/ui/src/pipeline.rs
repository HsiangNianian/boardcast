//! Wires the audio chain to the board agent on background threads, and reports to the UI over a channel.

use boardcast_agent::{Backend, Guidance, Said, Session, Update};
use crate::listen::{self, Heard, Job, Report};
use boardcast_audio::Sentence;
use std::collections::VecDeque;
use boardcast_board::graph::Utterance;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub enum Event {
    Agent(Update),
    Level(f32),
    Heard { meeting: u64, speaker: Option<String>, text: String, asr_ms: Option<u128> },
    /// A sentence from the microphone or a recording. The window works out who said it, as it holds
    /// the meeting's voices, and passes it on to the model.
    Spoken { meeting: u64, sentence: Sentence, audio: Heard },
    CaptureReady(u64),
    CaptureStopped(u64),
    Failed(String),
    /// What recognizes speech now, and why it isn't what was chosen, if it isn't.
    Hearing { with: String, note: Option<String> },
    /// A scripted source has finished and every queued line has been drawn.
    Done,
}

/// Which recognizer hears speech.
#[derive(Clone, Debug, PartialEq)]
pub struct Speech {
    /// The helper for the system's recognizer, where this platform has one.
    pub helper: Option<PathBuf>,
    /// Use the system's recognizer rather than the bundled models.
    pub system: bool,
    /// The language the system's recognizer listens for, or "auto".
    pub language: String,
    pub system_language: String,
}

impl Speech {
    /// Apple's helper sits beside the app's executable in a Mac package.
    pub fn helper_path() -> Option<PathBuf> {
        let helper = std::env::current_exe().ok()?.parent()?.join("boardcast-speech");
        helper.is_file().then_some(helper)
    }
}

pub struct Script {
    pub turns: Vec<Utterance>,
    pub wav: Option<PathBuf>,
}

pub struct Pipeline {
    pub said: Sender<Said>,
    pub events: Receiver<Event>,
    session: Session,
    audio: Sender<Job>,
    active: Option<Active>,
    next_capture: u64,
    reports: VecDeque<Arc<Mutex<Report>>>,
    event_tx: Sender<Event>,
    script_start: Sender<()>,
    speech: Speech,
}

#[derive(Clone)]
struct Active { id: u64, stop: Arc<AtomicBool>, input: Option<String>, source: String }

impl Pipeline {
    pub fn start(backend: Backend, guidance: Option<Guidance>, voice_models: PathBuf, speech: Speech, script: Script) -> Self {
        let (event_tx, events) = channel();
        let (said, said_rx) = channel::<Said>();
        let (drained_tx, drained_rx) = channel::<()>();
        let (ready_tx, ready_rx) = channel::<()>();
        let (audio, jobs) = channel::<Job>();
        let (updates_tx, updates_rx) = channel::<Update>();
        let session = Session::start(backend, guidance, said_rx, ready_tx, drained_tx, updates_tx);
        let tx = event_tx.clone();
        thread::spawn(move || {
            while let Ok(update) = updates_rx.recv() {
                let _ = tx.send(Event::Agent(update));
            }
        });

        let tx = event_tx.clone();
        thread::spawn(move || listen::run_jobs(jobs, voice_models, tx));

        // Scripted sources run in real time, so timings resemble a live meeting.
        let tx = event_tx.clone();
        let to_model = said.clone();
        let feed = audio.clone();
        let scripted_speech = speech.clone();
        let stamp = session.stamp();
        let scripted = !script.turns.is_empty() || script.wav.is_some();
        let (script_start, script_go) = channel::<()>();
        thread::spawn(move || {
            if !scripted {
                return;
            }
            // A meeting starts once the window has opened its own, or the first line would belong to
            // none; and once the model is up, or lines queued during loading would all merge.
            let _ = script_go.recv();
            let _ = ready_rx.recv();
            if let Some(path) = &script.wav {
                let (done, finished) = channel();
                let report = Arc::new(Mutex::new(Report::new(0, "microphone", &scripted_speech)));
                let _ = feed.send(Job { id: 0, meeting: stamp.load(Ordering::SeqCst), input: None,
                    source: "microphone".into(), speech: scripted_speech, stop: Arc::new(AtomicBool::new(false)),
                    report, wav: Some(path.clone()), done: Some(done) });
                let _ = finished.recv();
            }
            for turn in script.turns {
                let meeting = stamp.load(Ordering::SeqCst);
                let _ = tx.send(Event::Heard { meeting, speaker: turn.speaker.clone(), text: turn.text.clone(), asr_ms: None });
                let _ = to_model.send(Said { line: turn, ended: Instant::now(), asr_ms: None, meeting, renames: 0 });
                thread::sleep(Duration::from_millis(2500));
            }
            // Lines can merge into one request, so this waits for silence rather than a count.
            let mut done = 0_usize;
            while drained_rx.recv_timeout(Duration::from_secs(if done == 0 { 240 } else { 12 })).is_ok() {
                done += 1;
            }
            let _ = tx.send(Event::Done);
        });

        Self { said, events, session, audio, active: None, next_capture: 0, reports: VecDeque::new(), event_tx, script_start, speech }
    }

    /// The window is ready for a scripted meeting to begin.
    pub fn start_script(&self) {
        let _ = self.script_start.send(());
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// A change starts only after the old streams and helpers have been released.
    pub fn set_speech(&mut self, speech: Speech) {
        self.speech = speech;
        if let Some(active) = self.active.clone() {
            self.toggle_mic(None, "microphone");
            self.toggle_mic(active.input, &active.source);
        }
    }

    pub fn listening(&self) -> bool { self.active.is_some() }

    pub fn current_capture(&self, id: u64) -> bool { self.active.as_ref().is_some_and(|a| a.id == id) }

    pub fn capture_stopped(&mut self, id: u64) -> bool {
        if !self.current_capture(id) { return false; }
        self.active.take();
        true
    }

    pub fn toggle_mic(&mut self, input: Option<String>, source: &str) {
        if let Some(active) = self.active.take() {
            active.stop.store(true, Ordering::Relaxed);
            return;
        }
        self.next_capture += 1;
        let id = self.next_capture;
        let stop = Arc::new(AtomicBool::new(false));
        self.active = Some(Active { id, stop: stop.clone(), input: input.clone(), source: source.into() });
        let report = Arc::new(Mutex::new(Report::new(id, source, &self.speech)));
        if self.reports.len() == 20 { self.reports.pop_front(); }
        self.reports.push_back(report.clone());
        let job = Job { id, meeting: self.session.meeting(), input, source: source.into(), speech: self.speech.clone(), stop, report, wav: None, done: None };
        if self.audio.send(job).is_err() {
            self.active.take();
            let _ = self.event_tx.send(Event::Failed("Audio worker stopped".into()));
        }
    }

    /// Bounded measurements only: no meeting text, audio, device names, keys or error bodies.
    pub fn diagnostics(&self) -> String {
        fn stats(values: &VecDeque<u128>) -> serde_json::Value {
            if values.is_empty() { return serde_json::Value::Null; }
            let mut values: Vec<_> = values.iter().copied().collect(); values.sort_unstable();
            serde_json::json!({ "n": values.len(), "p50": values[(values.len() - 1) / 2],
                "p95": values[((values.len() - 1) as f64 * 0.95).ceil() as usize], "max": values.last() })
        }
        let captures: Vec<_> = self.reports.iter().map(|report| {
            let report = report.lock().unwrap();
            let mut value = serde_json::to_value(&*report).expect("report serializes");
            if self.current_capture(report.id) { value["elapsed_ms"] = serde_json::json!(report.started.elapsed().as_millis()); }
            for (i, track) in report.tracks.iter().enumerate() {
                value["tracks"][i]["source"] = serde_json::json!(listen::SOURCES[i]);
                for (name, values) in [("queue_ms", &track.queue_ms), ("asr_ms", &track.asr_ms), ("ui_receive_ms", &track.ui_receive_ms)] {
                    value["tracks"][i][name] = stats(values);
                }
            }
            value
        }).collect();
        serde_json::to_string_pretty(&serde_json::json!({ "version": 1, "captures": captures,
            "limits": { "captures": 20, "recent_timings_per_track": 128, "queued_packets_per_track": 300 },
            "speech_to_first_partial_ms": null, "sentence_end_to_final_ms": null, "screen_present_ms": null,
            "timing_basis": "monotonic arrival at Rust, recognition, receipt by UI; no acoustic onset or screen presentation measurement"
        })).expect("report serializes")
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        if let Some(active) = &self.active { active.stop.store(true, Ordering::Relaxed); }
    }
}
