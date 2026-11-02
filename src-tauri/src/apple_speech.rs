//! Apple owns the assets; this process bridge never opens a microphone or falls back to cloud ASR.
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex, Weak,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Clone, Copy, Default, Deserialize, Serialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Sensevoice,
    Apple,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Selection {
    pub provider: Provider,
    #[serde(default)]
    pub locale: String,
}

impl Selection {
    pub fn load(app: &AppHandle) -> Self {
        app.path()
            .app_config_dir()
            .ok()
            .and_then(|p| std::fs::read(p.join("speech.json")).ok())
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, app: &AppHandle) -> Result<(), String> {
        let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let data = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("speech.json"), data).map_err(|e| format!("Save failed: {e}"))
    }
}

#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct Language {
    pub id: String,
    pub name: String,
    pub installed: bool,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Availability {
    pub is_apple: bool,
    pub available: bool,
    pub reason: Option<String>,
    #[serde(default)]
    pub default_locale: String,
    pub locales: Vec<Language>,
}

impl Availability {
    pub fn ready(&self, locale: &str) -> bool {
        self.available && self.locales.iter().any(|l| l.id == locale && l.installed)
    }
}

#[derive(Default)]
pub struct Jobs {
    children: Mutex<Vec<Weak<Mutex<Child>>>>,
    closed: AtomicBool,
    pub preparing: AtomicBool,
}

impl Jobs {
    pub fn shutdown(&self) {
        self.closed.store(true, Ordering::SeqCst);
        for child in self
            .children
            .lock()
            .unwrap()
            .drain(..)
            .filter_map(|c| c.upgrade())
        {
            let mut child = child.lock().unwrap();
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Message {
    Status(Availability),
    Ready,
    Partial { text: String },
    Final { text: String, start: f64, end: f64 },
    Done,
    Error { message: String },
}

fn command(action: &str, locale: &str) -> Result<Command, String> {
    if !cfg!(target_os = "macos") {
        return Err("Apple transcription requires the macOS desktop app".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = Command::new(exe.with_file_name("boardcast-speech"));
    command.arg(action);
    if !locale.is_empty() {
        command.arg(locale);
    }
    Ok(command)
}

// Every exit path reaps the child and joins both pipe workers, including startup and download errors.
struct Helper {
    child: Arc<Mutex<Child>>,
    input: Option<mpsc::SyncSender<Vec<u8>>>,
    output: mpsc::Receiver<Result<Message, String>>,
    threads: Vec<JoinHandle<()>>,
}

impl Helper {
    fn spawn(mut command: Command, jobs: &Jobs) -> Result<Self, String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Cannot start Apple speech helper: {e}"))?;
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let child = Arc::new(Mutex::new(child));
        let mut children = jobs.children.lock().unwrap();
        if jobs.closed.load(Ordering::SeqCst) {
            let mut child = child.lock().unwrap();
            let _ = child.kill();
            let _ = child.wait();
            return Err("Speech is shutting down".into());
        }
        children.retain(|c| c.strong_count() > 0);
        children.push(Arc::downgrade(&child));
        drop(children);
        let (send, output) = mpsc::channel();
        let (input, receive) = mpsc::sync_channel::<Vec<u8>>(50);
        let errors = send.clone();
        let writer = thread::spawn(move || {
            for bytes in receive {
                if let Err(e) = stdin.write_all(&bytes) {
                    let _ = errors.send(Err(format!("Apple audio input closed: {e}")));
                    break;
                }
            }
        });
        let reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                // Cap protocol frames; a malformed helper cannot fill the parent's memory.
                let mut bytes = Vec::new();
                let n = std::io::Read::take(&mut reader, 1_048_576).read_until(b'\n', &mut bytes);
                match n {
                    Ok(0) => {
                        let _ = send.send(Err("Apple speech helper exited unexpectedly".into()));
                        break;
                    }
                    Ok(_) if bytes.last() == Some(&b'\n') => {
                        let result = serde_json::from_slice::<Message>(&bytes)
                            .map_err(|e| format!("Invalid Apple speech response: {e}"));
                        let done = matches!(
                            &result,
                            Ok(Message::Done | Message::Status(_) | Message::Error { .. })
                        );
                        if send.send(result).is_err() || done {
                            break;
                        }
                    }
                    _ => {
                        let _ = send.send(Err("Invalid or oversized Apple speech response".into()));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            input: Some(input),
            output,
            threads: vec![writer, reader],
        })
    }

    fn receive(&self, timeout: Duration) -> Result<Message, String> {
        match self
            .output
            .recv_timeout(timeout)
            .map_err(|e| format!("Waiting for Apple speech: {e}"))??
        {
            Message::Error { message } => Err(message),
            m => Ok(m),
        }
    }

    fn feed(&self, samples: &[f32]) -> Result<(), String> {
        for samples in samples.chunks(1600) {
            let bytes = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
            self.input.as_ref().ok_or("Apple transcription has stopped")?.try_send(bytes)
                .map_err(|_| "Apple transcription could not keep up with audio; recording stopped and the last phrase may be incomplete".to_string())?;
        }
        Ok(())
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        self.input.take();
        let mut child = self.child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
        drop(child);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

pub fn status(app: &AppHandle) -> Result<Availability, String> {
    if !cfg!(target_os = "macos") {
        return Ok(Availability {
            is_apple: false,
            available: false,
            reason: Some("platform".into()),
            default_locale: String::new(),
            locales: vec![],
        });
    }
    let helper = Helper::spawn(command("status", "")?, &app.state::<Jobs>())?;
    match helper.receive(Duration::from_secs(15))? {
        Message::Status(s) => Ok(s),
        _ => Err("Apple returned no capability information".into()),
    }
}

pub fn prepare(app: &AppHandle, locale: &str) -> Result<(), String> {
    let jobs = app.state::<Jobs>();
    if jobs.preparing.swap(true, Ordering::SeqCst) {
        return Err("An Apple language download is already running".into());
    }
    let result = (|| {
        let supported = status(app)?;
        if !supported.available || !supported.locales.iter().any(|l| l.id == locale) {
            return Err("This Apple transcription language is not supported on this device".into());
        }
        let helper = Helper::spawn(command("prepare", locale)?, &jobs)?;
        match helper.receive(Duration::from_secs(1200))? {
            Message::Done => Ok(()),
            _ => Err("Apple did not finish installing the language model".into()),
        }
    })();
    jobs.preparing.store(false, Ordering::SeqCst);
    result
}

pub struct Session {
    helper: Helper,
    capture: String,
    sequence: u64,
    until: f64,
    fed: usize,
}

impl Session {
    pub fn start(app: &AppHandle, locale: &str, capture: String) -> Result<Self, String> {
        let helper = Helper::spawn(command("transcribe", locale)?, &app.state::<Jobs>())?;
        match helper.receive(Duration::from_secs(25))? {
            Message::Ready => Ok(Self {
                helper,
                capture,
                sequence: 0,
                until: 0.0,
                fed: 0,
            }),
            _ => Err("Apple transcription did not become ready".into()),
        }
    }

    pub fn feed(&mut self, samples: &[f32]) -> Result<(), String> {
        self.helper.feed(samples)?;
        self.fed += samples.len();
        Ok(())
    }

    fn event(&mut self, app: &AppHandle, event: Message) -> Result<bool, String> {
        match event {
            Message::Partial { text } => {
                let _ = app.emit("voice-partial", text);
            }
            Message::Final { text, start, end } => {
                validate_range(start, end, self.until, self.fed)?;
                self.until = end;
                if !text.trim().is_empty() {
                    self.sequence += 1;
                    // Apple doesn't expose isolated ASR time or CAM++ embeddings; don't fabricate them.
                    let _ = app.emit(
                        "voice-final",
                        serde_json::json!({
                            "id": format!("{}:{}", self.capture, self.sequence), "revision": 0,
                            "text": text, "start": start, "end": end, "sources": ["mic"],
                        }),
                    );
                }
                let _ = app.emit("voice-partial", "");
            }
            Message::Done => return Ok(true),
            Message::Error { message } => return Err(message),
            _ => return Err("Unexpected Apple transcription response".into()),
        }
        Ok(false)
    }

    pub fn poll(&mut self, app: &AppHandle) -> Result<(), String> {
        while let Ok(event) = self.helper.output.try_recv() {
            if self.event(app, event?)? {
                return Err("Apple transcription ended while recording".into());
            }
        }
        Ok(())
    }

    pub fn finish(&mut self, app: &AppHandle) -> Result<(), String> {
        self.helper.input.take();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let event = self
                .helper
                .receive(deadline.saturating_duration_since(Instant::now()))?;
            if self.event(app, event)? {
                return Ok(());
            }
        }
    }
}

fn validate_range(start: f64, end: f64, until: f64, fed: usize) -> Result<(), String> {
    if !start.is_finite()
        || !end.is_finite()
        || start < 0.0
        || end < start
        || start + 0.001 < until
        || end > fed as f64 / 16_000.0 + 0.1
    {
        Err("Invalid or repeated Apple transcript timestamps".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_installed_supported_language_is_ready() {
        let s = Availability {
            is_apple: true,
            available: true,
            reason: None,
            default_locale: "ja-JP".into(),
            locales: vec![
                Language {
                    id: "ja-JP".into(),
                    name: "日本語".into(),
                    installed: true,
                },
                Language {
                    id: "en-US".into(),
                    name: "English".into(),
                    installed: false,
                },
            ],
        };
        assert!(s.ready("ja-JP"));
        assert!(!s.ready("en-US"));
        assert!(!s.ready("zh-CN"));
        assert!(!Availability {
            available: false,
            ..s
        }
        .ready("ja-JP"));
        assert_eq!(Selection::default().provider, Provider::Sensevoice);
        assert!(
            serde_json::from_str::<Selection>(r#"{"provider":"cloud","locale":"en-US"}"#).is_err()
        );
    }

    #[test]
    fn apple_ranges_cannot_duplicate_text_or_exceed_received_audio() {
        assert!(validate_range(0.25, 1.0, 0.25, 16_000).is_ok());
        for (start, end) in [(-1.0, 1.0), (2.0, 1.0), (0.0, 20.0), (f64::NAN, 1.0)] {
            assert!(validate_range(start, end, 0.0, 16_000).is_err());
        }
        assert!(validate_range(0.0, 1.0, 1.0, 16_000).is_err());
        assert!(serde_json::from_str::<Message>(r#"{"type":"final","text":"x"}"#).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn helper_eof_timeout_and_shutdown_are_bounded() {
        let jobs = Jobs::default();
        let mut command = Command::new("sh");
        command.args(["-c", "printf '%s\\n' '{\"type\":\"ready\"}'; read line; printf '%s\\n' '{\"type\":\"done\"}'"]);
        let mut helper = Helper::spawn(command, &jobs).unwrap();
        assert!(matches!(
            helper.receive(Duration::from_secs(2)).unwrap(),
            Message::Ready
        ));
        assert!(helper.receive(Duration::from_millis(10)).is_err());
        helper.input.take();
        assert!(matches!(
            helper.receive(Duration::from_secs(2)).unwrap(),
            Message::Done
        ));
        drop(helper);
        let mut command = Command::new("sh");
        command.args(["-c", "read line"]);
        let helper = Helper::spawn(command, &jobs).unwrap();
        jobs.shutdown();
        assert!(helper.receive(Duration::from_secs(2)).is_err());
        assert!(helper.child.lock().unwrap().try_wait().unwrap().is_some());
    }
}
