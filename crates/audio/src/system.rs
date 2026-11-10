//! The system's own speech recognition, through a helper process: on a Mac, `boardcast-speech`, which uses
//! SpeechAnalyzer or, before macOS 26 or where it can't, SFSpeechRecognizer. Each sentence goes to it as a
//! little-endian sample count and 16 kHz float32 samples; its text comes back as a line of JSON.

use crate::Recognizer;
use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub struct SystemSpeech {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    /// Which of the system's recognizers answered, such as "analyzer" or "legacy".
    pub engine: String,
}

impl SystemSpeech {
    /// Start the helper for `locale` ("auto" for the system's language) and wait until it can recognize,
    /// which the first time can include downloading the language's model.
    pub fn start(helper: &Path, locale: &str) -> Result<Self> {
        let mut child = Command::new(helper)
            .args(["serve", locale])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .with_context(|| format!("starting {}", helper.display()))?;
        let input = child.stdin.take().context("no input to the speech helper")?;
        let output = BufReader::new(child.stdout.take().context("no output from the speech helper")?);
        let mut speech = Self { child, input, output, engine: String::new() };
        loop {
            let event = speech.next_event()?;
            match event["type"].as_str() {
                Some("ready") => {
                    speech.engine = event["engine"].as_str().unwrap_or_default().to_string();
                    return Ok(speech);
                }
                Some("error") => bail!("{}", event["message"].as_str().unwrap_or("the system's speech recognition did not start")),
                Some("fallback") => eprintln!("SpeechAnalyzer could not be used, so the older recognizer is: {}", event["message"].as_str().unwrap_or_default()),
                _ => {}
            }
        }
    }

    fn next_event(&mut self) -> Result<Value> {
        let mut line = String::new();
        loop {
            line.clear();
            if self.output.read_line(&mut line)? == 0 {
                bail!("the speech helper stopped");
            }
            if let Ok(event) = serde_json::from_str::<Value>(line.trim()) {
                return Ok(event);
            }
        }
    }
}

impl Recognizer for SystemSpeech {
    fn recognize(&mut self, samples: &[f32]) -> Result<String> {
        let count = u32::try_from(samples.len()).map_err(|_| anyhow!("sentence too long"))?;
        let mut bytes = Vec::with_capacity(4 + samples.len() * 4);
        bytes.extend_from_slice(&count.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        self.input.write_all(&bytes).and_then(|()| self.input.flush()).context("sending a sentence to the speech helper")?;
        loop {
            let event = self.next_event()?;
            match event["type"].as_str() {
                Some("text") => return Ok(event["text"].as_str().unwrap_or_default().trim().to_string()),
                Some("error") => bail!("{}", event["message"].as_str().unwrap_or("recognition failed")),
                _ => {}
            }
        }
    }
}

impl Drop for SystemSpeech {
    fn drop(&mut self) {
        // A count of 0 ends it; the helper also leaves once this process is gone.
        let _ = self.input.write_all(&0_u32.to_le_bytes());
        let _ = self.input.flush();
        if self.child.try_wait().ok().flatten().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
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
        assert_eq!(speech.recognize(&[0.5; 1600]).unwrap(), "1600 samples, sum 800.0");
        assert_eq!(speech.recognize(&[0.25; 8]).unwrap(), "8 samples, sum 2.0");
    }

    #[test]
    fn a_missing_helper_is_an_error() {
        assert!(SystemSpeech::start(Path::new("/nowhere/boardcast-speech"), "auto").is_err());
    }
}
