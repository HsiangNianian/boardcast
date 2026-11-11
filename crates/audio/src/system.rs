//! Apple's speech recognition and system audio capture, through the packaged Swift helper.

use crate::{helper::Helper, language::Probe, timing, Captured, Recognizer, SAMPLE_RATE};
use anyhow::{bail, ensure, Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const UNFINISHED: &str = "Apple speech did not finish within five seconds after stopping. Some captured speech was not transcribed.";

pub struct SystemSpeech {
    helper: Helper,
    pub engine: String,
    pub locale: String,
    failure: Option<String>,
    stop: Arc<AtomicBool>,
    finish_by: Option<Instant>,
}

impl SystemSpeech {
    /// "auto" follows the system language; it does not detect speech's language.
    pub fn start(helper: &Path, locale: &str) -> Result<Self> {
        Self::start_with_stop(helper, locale, Arc::new(AtomicBool::new(false)))
    }

    pub fn start_with_stop(helper: &Path, locale: &str, stop: Arc<AtomicBool>) -> Result<Self> {
        Self::launch(helper, locale, stop, false)
    }

    fn launch(helper: &Path, locale: &str, stop: Arc<AtomicBool>, finish: bool) -> Result<Self> {
        ensure!(finish || !stop.load(Ordering::Relaxed), "audio cancelled");
        let mut command = Command::new(helper);
        command.args(["serve", locale]);
        let helper = Helper::spawn(command)?;
        let deadline = Instant::now() + Duration::from_secs(300);
        let mut finish_by = None;
        loop {
            if stop.load(Ordering::Relaxed) {
                ensure!(finish, "audio cancelled");
                finish_by.get_or_insert_with(|| Instant::now() + Duration::from_secs(5));
            }
            let remaining = finish_by.unwrap_or(deadline).min(deadline).saturating_duration_since(Instant::now());
            ensure!(!remaining.is_zero(), if finish_by.is_some() { UNFINISHED } else { "audio helper timed out" });
            let Some(event) = helper.poll(remaining.min(Duration::from_millis(50)))? else { continue };
            match event["type"].as_str() {
                Some("ready") => return Ok(Self {
                    helper,
                    engine: event["engine"].as_str().context("missing speech engine")?.into(),
                    locale: event["locale"].as_str().context("missing speech locale")?.into(),
                    failure: None,
                    stop,
                    finish_by,
                }),
                Some("error") => bail!("{}", event["message"].as_str().unwrap_or("Apple speech recognition did not start")),
                Some("fallback") => eprintln!("SpeechAnalyzer fallback: {}", event["message"].as_str().unwrap_or_default()),
                Some("preparing") => {}
                _ => bail!("unexpected speech helper startup message"),
            }
        }
    }
}

impl Recognizer for SystemSpeech {
    fn recognize(&mut self, samples: &[f32]) -> Result<String> {
        let result = (|| {
            ensure!(!samples.is_empty() && samples.len() <= 16_000 * 30 && samples.iter().all(|s| s.is_finite()), "invalid sentence samples");
            ensure!(self.finish_by.is_none_or(|end| Instant::now() < end), UNFINISHED);
            let mut bytes = Vec::with_capacity(4 + samples.len() * 4);
            bytes.extend_from_slice(&(samples.len() as u32).to_le_bytes());
            for sample in samples { bytes.extend_from_slice(&sample.to_le_bytes()); }
            self.helper.write(bytes)?;
            let request_deadline = Instant::now() + Duration::from_secs(60);
            let event = loop {
                // One finalization budget for the whole stopped stream, including queued sentences.
                if self.stop.load(Ordering::Relaxed) {
                    self.finish_by.get_or_insert_with(|| Instant::now() + Duration::from_secs(5));
                }
                let remaining = self.finish_by.unwrap_or(request_deadline).min(request_deadline).saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    ensure!(self.finish_by.is_none_or(|end| Instant::now() < end), UNFINISHED);
                    bail!("audio helper timed out");
                }
                if let Some(event) = self.helper.poll(remaining.min(Duration::from_millis(50)))? { break event; }
            };
            match event["type"].as_str() {
                Some("text") => Ok(event["text"].as_str().context("missing recognized text")?.trim().to_string()),
                Some("error") => bail!("{}", event["message"].as_str().unwrap_or("recognition failed")),
                _ => bail!("unexpected speech helper reply"),
            }
        })();
        if let Err(e) = &result { self.failure = Some(format!("{e:#}")); }
        result
    }

    fn error(&self) -> Option<&str> { self.failure.as_deref() }
    fn identity(&self) -> Option<(&str, &str)> { Some((&self.engine, &self.locale)) }
}

/// Each source chooses one opening language; the normal ear retains and segments the original PCM.
pub struct AutoSpeech {
    probe: Option<Probe>,
    opening: Vec<f32>,
    speech: Option<SystemSpeech>,
    helper: PathBuf,
    stop: Arc<AtomicBool>,
    failure: Option<String>,
}

impl AutoSpeech {
    pub fn load(models: &Path, helper: &Path, stop: Arc<AtomicBool>) -> Result<Self> {
        Ok(Self { probe: Some(Probe::load(models)?), opening: vec![], speech: None, helper: helper.into(), stop, failure: None })
    }
}

impl Recognizer for AutoSpeech {
    fn prepare(&mut self, samples: &[f32], finished: bool) -> bool {
        if self.probe.is_none() { return true; }
        let needed = SAMPLE_RATE as usize * 5 - self.opening.len();
        self.opening.extend_from_slice(&samples[..samples.len().min(needed)]);
        finished || self.opening.len() == SAMPLE_RATE as usize * 5
    }

    fn recognize(&mut self, samples: &[f32]) -> Result<String> {
        let result = (|| {
            if let Some(error) = &self.failure { bail!("{error}"); }
            ensure!(!samples.is_empty() && samples.len() <= SAMPLE_RATE as usize * 30 && samples.iter().all(|s| s.is_finite()), "invalid sentence samples");
            if self.speech.is_none() {
                let probe = self.probe.take().context("language detection is unavailable")?;
                // A short first VAD segment can look English inside an otherwise Japanese utterance.
                let opening = if self.opening.is_empty() { &samples[..samples.len().min(SAMPLE_RATE as usize * 5)] } else { &self.opening };
                let locale = probe.detect(opening)?;
                self.opening = Vec::new();
                drop(probe);
                // A short first utterance may only reach VAD when stopped; startup shares its finalization budget.
                self.speech = Some(SystemSpeech::launch(&self.helper, locale, self.stop.clone(), true)?);
            }
            self.speech.as_mut().unwrap().recognize(samples)
        })();
        if let Err(e) = &result { self.failure = Some(format!("{e:#}")); }
        result
    }

    fn error(&self) -> Option<&str> { self.failure.as_deref() }
    fn identity(&self) -> Option<(&str, &str)> { self.speech.as_ref().and_then(Recognizer::identity) }
}

fn system_samples(event: &Value) -> Result<Captured> {
    let values = event["samples"].as_array().context("missing system audio samples")?;
    ensure!(!values.is_empty() && values.len() <= 1600, "invalid system audio packet length");
    let samples = values.iter().map(|v| {
        let sample = v.as_f64().context("invalid system audio sample")? as f32;
        ensure!(sample.is_finite(), "invalid system audio sample");
        Ok(sample)
    }).collect::<Result<_>>()?;
    let start = event.get("hostTimeNs").map(|v| v.as_u64().context("invalid system audio timestamp")).transpose()?.and_then(timing::host_time);
    Ok(Captured { samples, start })
}

/// Preserve this stream as a separate source; EOF releases ScreenCaptureKit before the next start.
pub fn capture(helper: &Path, stop: &AtomicBool, mut samples: impl FnMut(Captured) -> Result<()>, ready: impl FnOnce()) -> Result<()> {
    let mut command = Command::new(helper);
    command.arg("system-audio");
    capture_from(Helper::spawn(command)?, stop, &mut samples, ready)
}

fn capture_from(helper: Helper, stop: &AtomicBool, samples: &mut impl FnMut(Captured) -> Result<()>, ready: impl FnOnce()) -> Result<()> {
    let event = helper.next(Duration::from_secs(120), stop)?;
    match event["type"].as_str() {
        Some("ready") => ready(),
        Some("error") => bail!("{}", event["message"].as_str().unwrap_or("system audio did not start")),
        _ => bail!("system audio arrived before ready"),
    }
    while !stop.load(Ordering::Relaxed) {
        let Some(event) = helper.poll(Duration::from_millis(50))? else { continue };
        match event["type"].as_str() {
            Some("audio") => samples(system_samples(&event)?)?,
            Some("error") => bail!("{}", event["message"].as_str().unwrap_or("system audio stopped")),
            _ => bail!("unexpected system audio message"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in helper that speaks the same protocol and says how many samples it got.
    const FAKE: &str = r#"
import json, struct, sys
print(json.dumps({"type": "fallback", "message": "no analyzer here"}), flush=True)
print(json.dumps({"type": "ready", "engine": "fake", "locale": sys.argv[2]}), flush=True)
while True:
    head = sys.stdin.buffer.read(4)
    if len(head) < 4: break
    (n,) = struct.unpack("<I", head)
    if n == 0: break
    body = sys.stdin.buffer.read(n * 4)
    total = sum(struct.unpack("<%df" % n, body))
    print(json.dumps({"type": "text", "text": " %d samples, sum %.1f " % (n, total)}), flush=True)
"#;

    fn fake_helper(source: &str) -> Option<std::path::PathBuf> {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        Command::new("python3").arg("--version").output().ok()?;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
        // Coarse wall clocks can give parallel tests the same timestamp and overwrite their scripts.
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("boardcast-speech-test-{}-{stamp}-{sequence}", std::process::id()));
        std::fs::create_dir(&dir).expect("unique speech test directory");
        let script = dir.join("fake.py");
        std::fs::write(&script, source).ok()?;
        let helper = dir.join("boardcast-speech");
        std::fs::write(&helper, format!("#!/bin/sh\nexec python3 -I {} \"$@\"\n", script.display())).ok()?;
        #[cfg(not(unix))]
        return None;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).ok()?;
        }
        Some(helper)
    }

    #[test]
    fn sentences_go_to_the_helper_and_text_comes_back() {
        let Some(helper) = fake_helper(FAKE) else { return };
        let mut speech = SystemSpeech::start(&helper, "ja-JP").unwrap();
        assert_eq!(speech.engine, "fake");
        assert_eq!(speech.locale, "ja-JP");
        assert_eq!(speech.recognize(&[0.5; 1600]).unwrap(), "1600 samples, sum 800.0");
        assert_eq!(speech.recognize(&[0.25; 8]).unwrap(), "8 samples, sum 2.0");
    }

    #[test]
    fn stopping_keeps_a_prompt_final_reply() {
        let Some(helper) = fake_helper(FAKE) else { return };
        let stop = Arc::new(AtomicBool::new(false));
        let mut speech = SystemSpeech::start_with_stop(&helper, "en-US", stop.clone()).unwrap();
        stop.store(true, Ordering::Relaxed);
        assert_eq!(speech.recognize(&[0.5; 1600]).unwrap(), "1600 samples, sum 800.0");
        assert_eq!(speech.recognize(&[0.25; 8]).unwrap(), "8 samples, sum 2.0");
    }

    #[test]
    fn stopping_bounds_an_inflight_request_and_later_queued_sentences() {
        let Some(helper) = fake_helper(r#"
import json, struct, sys, time
print(json.dumps({'type':'ready','engine':'fake','locale':sys.argv[2]}), flush=True)
n, = struct.unpack('<I', sys.stdin.buffer.read(4))
sys.stdin.buffer.read(n * 4)
time.sleep(12)
"#) else { return };
        let stop = Arc::new(AtomicBool::new(false));
        let mut speech = SystemSpeech::start_with_stop(&helper, "en-US", stop.clone()).unwrap();
        let stopping = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            stop.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let error = speech.recognize(&[0.5; 1600]).unwrap_err();
        stopping.join().unwrap();
        assert!(error.to_string().contains("after stopping"), "{error:#}");
        assert!(started.elapsed() < Duration::from_secs(8));
        let again = Instant::now();
        assert!(speech.recognize(&[0.5; 1600]).is_err());
        assert!(again.elapsed() < Duration::from_millis(100), "the drain deadline was reset");
    }

    #[test]
    fn stopped_auto_startup_shares_the_transcription_deadline() {
        let Some(helper) = fake_helper(r#"
import json, struct, sys, time
time.sleep(2)
print(json.dumps({'type':'ready','engine':'fake','locale':sys.argv[2]}), flush=True)
n, = struct.unpack('<I', sys.stdin.buffer.read(4))
sys.stdin.buffer.read(n * 4)
time.sleep(4)
print(json.dumps({'type':'text','text':'too late'}), flush=True)
"#) else { return };
        let stop = Arc::new(AtomicBool::new(true));
        assert!(SystemSpeech::start_with_stop(&helper, "en-US", stop.clone()).is_err());
        let started = Instant::now();
        let mut speech = SystemSpeech::launch(&helper, "en-US", stop, true).unwrap();
        assert!(speech.recognize(&[0.5; 1600]).unwrap_err().to_string().contains("after stopping"));
        assert!(started.elapsed() < Duration::from_secs(8));
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "needs BOARDCAST_VOICE_DIR, BOARDCAST_AUTO_AUDIO and Python 3; uses no audio device"]
    fn auto_routes_real_opening_speech_and_preserves_the_pcm() {
        let models = PathBuf::from(std::env::var_os("BOARDCAST_VOICE_DIR").expect("voice models"));
        let audio = PathBuf::from(std::env::var_os("BOARDCAST_AUTO_AUDIO").expect("verified ASR samples"));
        let helper = fake_helper(r#"
import json, pathlib, struct, sys
locale = sys.argv[2]
output = pathlib.Path(__file__).parent / (locale + '.pcm')
print(json.dumps({'type':'ready','engine':'fake','locale':locale}), flush=True)
while True:
    header = sys.stdin.buffer.read(4)
    if len(header) < 4: break
    n, = struct.unpack('<I', header)
    body = sys.stdin.buffer.read(n * 4)
    if len(body) != n * 4: break
    with output.open('ab') as f: f.write(body)
    print(json.dumps({'type':'text','text':locale}), flush=True)
"#).expect("Python 3 helper");
        std::thread::scope(|scope| {
            for (file, locale) in [("zh-alimeeting-00.wav", "zh-CN"), ("ja-fleurs-00.wav", "ja-JP"), ("en-ami-00.wav", "en-US")] {
                let (models, audio, helper) = (&models, &audio, &helper);
                scope.spawn(move || {
                    let wave = sherpa_onnx::Wave::read(&audio.join(file).to_string_lossy()).unwrap();
                    assert_eq!(wave.sample_rate(), SAMPLE_RATE);
                    let samples = &wave.samples()[..wave.samples().len().min(SAMPLE_RATE as usize * 8)];
                    let stop = Arc::new(AtomicBool::new(locale == "ja-JP"));
                    let mut speech = AutoSpeech::load(models, helper, stop.clone()).unwrap();
                    assert!(speech.identity().is_none());
                    for (i, chunk) in samples.chunks(crate::VAD_WINDOW).enumerate() {
                        let collected = ((i + 1) * crate::VAD_WINDOW).min(samples.len());
                        assert_eq!(speech.prepare(chunk, false), collected >= SAMPLE_RATE as usize * 5);
                        assert_eq!(speech.opening.len(), collected.min(SAMPLE_RATE as usize * 5));
                    }
                    assert_eq!(speech.recognize(samples).unwrap(), locale, "{file}");
                    assert!(speech.probe.is_none(), "release SenseVoice after choosing Apple's language");
                    assert!(speech.opening.is_empty());
                    assert_eq!(speech.identity(), Some(("fake", locale)));
                    stop.store(true, Ordering::Relaxed);
                    assert_eq!(speech.recognize(&[0.25; 32]).unwrap(), locale);
                    drop(speech);
                    let mut expected: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
                    expected.extend([0.25_f32; 32].iter().flat_map(|s| s.to_le_bytes()));
                    assert_eq!(std::fs::read(helper.parent().unwrap().join(format!("{locale}.pcm"))).unwrap(), expected,
                        "forward all opening PCM once, including samples outside the probe, then the next sentence");
                });
            }
        });
        let auto = AutoSpeech::load(&models, &helper, Arc::new(AtomicBool::new(false))).unwrap();
        let mut ear = crate::Ear::with(&models, crate::Recognizers::system(Box::new(auto))).unwrap();
        let mut unheard = |_: crate::Sentence| panic!("silence must not start automatic recognition");
        for _ in 0..400 {
            ear.feed(&[0.0; 1600], &mut unheard);
            assert!(ear.audio.len() <= crate::RETAIN, "quiet Auto capture must keep bounded audio");
            assert!(ear.waiting.is_empty(), "silence must not queue sentences for language detection");
        }
        ear.finish(&mut unheard);
        assert!(ear.identity().is_none() && ear.error().is_none());
        drop(ear);
        std::fs::remove_dir_all(helper.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_missing_helper_is_an_error() {
        assert!(SystemSpeech::start(Path::new("/nowhere/boardcast-speech"), "auto").is_err());
    }

    #[test]
    fn invalid_audio_is_not_forwarded() {
        for event in [serde_json::json!({"samples":[]}), serde_json::json!({"samples":[1e100]}), serde_json::json!({"samples":["0"]}), serde_json::json!({"samples":vec![0; 1601]})] {
            assert!(system_samples(&event).is_err());
        }
        let untimed = system_samples(&serde_json::json!({"samples":[0.25,-0.5,0]})).unwrap();
        assert_eq!(untimed.samples, vec![0.25,-0.5,0.0]);
        assert!(untimed.start.is_none());
        for stamp in [serde_json::json!(-1), serde_json::json!(1.5), serde_json::json!("12"), Value::Null] {
            assert!(system_samples(&serde_json::json!({"samples":[0.25], "hostTimeNs":stamp})).is_err());
        }
        assert!(system_samples(&serde_json::json!({"samples":[0.25], "hostTimeNs":1_000_000_000})).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn system_capture_requires_ready_routes_audio_and_surfaces_errors() {
        let run = |script: &str, expected: bool| {
            let mut command = Command::new("sh");
            command.args(["-c", script]);
            let stop = AtomicBool::new(false);
            let mut heard = vec![];
            let result = capture_from(Helper::spawn(command).unwrap(), &stop, &mut |samples| {
                heard.extend(samples.samples); stop.store(true, Ordering::Relaxed); Ok(())
            }, || {});
            assert_eq!(result.is_ok(), expected, "{result:?}");
            heard
        };
        assert_eq!(run("printf '%s\\n' '{\"type\":\"ready\"}' '{\"type\":\"audio\",\"samples\":[0.25,-0.5]}'; read line", true), vec![0.25,-0.5]);
        assert!(run("printf '%s\\n' '{\"type\":\"audio\",\"samples\":[0.25]}'", false).is_empty());
        assert!(run("printf '%s\\n' '{\"type\":\"error\",\"message\":\"SYSTEM_AUDIO_PERMISSION\"}'", false).is_empty());
        assert!(run("printf '%s\\n' '{\"type\":\"ready\"}' '{\"type\":\"error\",\"message\":\"device disconnected\"}'", false).is_empty());
    }
}
