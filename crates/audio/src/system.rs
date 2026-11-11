//! Apple's speech recognition and system audio capture, through the packaged Swift helper.

use crate::{helper::Helper, Recognizer};
use anyhow::{bail, ensure, Context, Result};
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct SystemSpeech {
    helper: Helper,
    pub engine: String,
    pub locale: String,
    failure: Option<String>,
}

impl SystemSpeech {
    /// "auto" follows the system language; it does not detect speech's language.
    pub fn start(helper: &Path, locale: &str) -> Result<Self> {
        Self::start_with_stop(helper, locale, Arc::new(AtomicBool::new(false)))
    }

    pub fn start_with_stop(helper: &Path, locale: &str, stop: Arc<AtomicBool>) -> Result<Self> {
        let mut command = Command::new(helper);
        command.args(["serve", locale]);
        let helper = Helper::spawn(command)?;
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            let event = helper.next(deadline.saturating_duration_since(Instant::now()), &stop)?;
            match event["type"].as_str() {
                Some("ready") => return Ok(Self {
                    helper,
                    engine: event["engine"].as_str().context("missing speech engine")?.into(),
                    locale: event["locale"].as_str().context("missing speech locale")?.into(),
                    failure: None,
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
            let mut bytes = Vec::with_capacity(4 + samples.len() * 4);
            bytes.extend_from_slice(&(samples.len() as u32).to_le_bytes());
            for sample in samples { bytes.extend_from_slice(&sample.to_le_bytes()); }
            self.helper.write(bytes)?;
            let event = self.helper.next(Duration::from_secs(60), &AtomicBool::new(false))?;
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
}

fn system_samples(event: &Value) -> Result<Vec<f32>> {
    let values = event["samples"].as_array().context("missing system audio samples")?;
    ensure!(!values.is_empty() && values.len() <= 1600, "invalid system audio packet length");
    values.iter().map(|v| {
        let sample = v.as_f64().context("invalid system audio sample")? as f32;
        ensure!(sample.is_finite(), "invalid system audio sample");
        Ok(sample)
    }).collect()
}

/// Preserve this stream as a separate source; EOF releases ScreenCaptureKit before the next start.
pub fn capture(helper: &Path, stop: &AtomicBool, mut samples: impl FnMut(Vec<f32>) -> Result<()>, ready: impl FnOnce()) -> Result<()> {
    let mut command = Command::new(helper);
    command.arg("system-audio");
    capture_from(Helper::spawn(command)?, stop, &mut samples, ready)
}

fn capture_from(helper: Helper, stop: &AtomicBool, samples: &mut impl FnMut(Vec<f32>) -> Result<()>, ready: impl FnOnce()) -> Result<()> {
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

    fn fake_helper() -> Option<std::path::PathBuf> {
        Command::new("python3").arg("--version").output().ok()?;
        let dir = std::env::temp_dir().join(format!("boardcast-speech-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).ok()?;
        let script = dir.join("fake.py");
        std::fs::write(&script, FAKE).ok()?;
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
        let Some(helper) = fake_helper() else { return };
        let mut speech = SystemSpeech::start(&helper, "ja-JP").unwrap();
        assert_eq!(speech.engine, "fake");
        assert_eq!(speech.locale, "ja-JP");
        assert_eq!(speech.recognize(&[0.5; 1600]).unwrap(), "1600 samples, sum 800.0");
        assert_eq!(speech.recognize(&[0.25; 8]).unwrap(), "8 samples, sum 2.0");
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
        assert_eq!(system_samples(&serde_json::json!({"samples":[0.25,-0.5,0]})).unwrap(), vec![0.25,-0.5,0.0]);
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
                heard.extend(samples); stop.store(true, Ordering::Relaxed); Ok(())
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
