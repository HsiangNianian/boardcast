//! Wires the audio chain to the board agent on background threads, and reports to the UI over a channel.

use boardcast_agent::{Backend, Guidance, Said, Session, Update};
use boardcast_audio::{capture, play_wav, Ear, Sentence};
use boardcast_board::graph::Utterance;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub enum Event {
    Agent(Update),
    Level(f32),
    Heard { meeting: u64, speaker: Option<String>, text: String, asr_ms: Option<u128> },
    /// A sentence from the microphone or a recording. The window works out who said it, as it holds
    /// the meeting's voices, and passes it on to the model.
    Spoken { meeting: u64, sentence: Sentence },
    Failed(String),
    /// A scripted source has finished and every queued line has been drawn.
    Done,
}

pub struct Script {
    pub turns: Vec<Utterance>,
    pub wav: Option<PathBuf>,
}

pub struct Pipeline {
    pub said: Sender<Said>,
    pub events: Receiver<Event>,
    session: Session,
    samples: Sender<Vec<f32>>,
    mic_stop: Option<Arc<AtomicBool>>,
    event_tx: Sender<Event>,
    script_start: Sender<()>,
}

impl Pipeline {
    pub fn start(backend: Backend, guidance: Option<Guidance>, voice_models: PathBuf, script: Script) -> Self {
        let (event_tx, events) = channel();
        let (said, said_rx) = channel::<Said>();
        let (drained_tx, drained_rx) = channel::<()>();
        let (ready_tx, ready_rx) = channel::<()>();
        let (samples, samples_rx) = channel::<Vec<f32>>();
        let (updates_tx, updates_rx) = channel::<Update>();
        let session = Session::start(backend, guidance, said_rx, ready_tx, drained_tx, updates_tx);
        let tx = event_tx.clone();
        thread::spawn(move || {
            while let Ok(update) = updates_rx.recv() {
                let _ = tx.send(Event::Agent(update));
            }
        });

        let tx = event_tx.clone();
        let stamp = session.stamp();
        let (finish_tx, finish_rx) = channel::<()>();
        thread::spawn(move || {
            let mut ear = match Ear::load(&voice_models) {
                Ok(ear) => ear,
                Err(e) => {
                    let _ = tx.send(Event::Failed(format!("{e:#}")));
                    return;
                }
            };
            let mut heard = |sentence: Sentence| {
                let _ = tx.send(Event::Spoken { meeting: stamp.load(Ordering::SeqCst), sentence });
            };
            loop {
                match samples_rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(chunk) => {
                        let level = chunk.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
                        let _ = tx.send(Event::Level(level));
                        ear.feed(&chunk, &mut heard);
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
                if finish_rx.try_recv().is_ok() {
                    ear.finish(&mut heard);
                }
            }
        });

        // Scripted sources run in real time, so timings resemble a live meeting.
        let tx = event_tx.clone();
        let to_model = said.clone();
        let feed = samples.clone();
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
                match play_wav(path, &feed) {
                    Ok(()) => {
                        let _ = finish_tx.send(());
                    }
                    Err(e) => {
                        let _ = tx.send(Event::Failed(format!("{e:#}")));
                    }
                }
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

        Self { said, events, session, samples, mic_stop: None, event_tx, script_start }
    }

    /// The window is ready for a scripted meeting to begin.
    pub fn start_script(&self) {
        let _ = self.script_start.send(());
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn listening(&self) -> bool {
        self.mic_stop.is_some()
    }

    pub fn toggle_mic(&mut self) {
        if let Some(stop) = self.mic_stop.take() {
            stop.store(true, Ordering::Relaxed);
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        self.mic_stop = Some(stop.clone());
        let samples = self.samples.clone();
        let tx = self.event_tx.clone();
        thread::spawn(move || {
            if let Err(e) = capture(&stop, &samples) {
                let _ = tx.send(Event::Failed(format!("{e:#}")));
            }
        });
    }
}
