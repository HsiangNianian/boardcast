//! The Apple helper's pipes and lifetime, shared by recognition and system capture.

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(crate) struct Helper {
    child: Child,
    input: Option<SyncSender<Vec<u8>>>,
    output: Receiver<Result<Value>>,
    threads: Vec<JoinHandle<()>>,
}

impl Helper {
    pub(crate) fn spawn(mut command: Command) -> Result<Self> {
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().context("starting the audio helper")?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (send, output) = sync_channel(64);
        let (input, receive) = sync_channel::<Vec<u8>>(64);
        let errors = send.clone();
        let writer = thread::spawn(move || {
            for bytes in receive {
                if let Err(e) = stdin.write_all(&bytes).and_then(|()| stdin.flush()) {
                    let _ = errors.try_send(Err(e.into()));
                    break;
                }
            }
        });
        let reader = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                let event = (|| -> Result<Value> {
                    let n = reader.by_ref().take(1_048_577).read_until(b'\n', &mut line)?;
                    if n == 0 { bail!("the audio helper stopped"); }
                    if n > 1_048_576 || !line.ends_with(b"\n") { bail!("invalid audio helper message length"); }
                    serde_json::from_slice(&line).context("invalid audio helper message")
                })();
                let failed = event.is_err();
                if send.try_send(event).is_err() || failed { break; }
            }
        });
        Ok(Self { child, input: Some(input), output, threads: vec![reader, writer] })
    }

    pub(crate) fn write(&self, bytes: Vec<u8>) -> Result<()> {
        self.input.as_ref().context("audio helper stopped")?.try_send(bytes).context("audio helper is not reading its input")
    }

    pub(crate) fn close_input(&mut self) { self.input.take(); }

    pub(crate) fn next(&self, timeout: Duration, stop: &AtomicBool) -> Result<Value> {
        let end = Instant::now() + timeout;
        loop {
            if stop.load(Ordering::Relaxed) { bail!("audio cancelled"); }
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() { bail!("audio helper timed out"); }
            if let Some(event) = self.poll(left.min(Duration::from_millis(50)))? { return Ok(event); }
        }
    }

    pub(crate) fn poll(&self, timeout: Duration) -> Result<Option<Value>> {
        match self.output.recv_timeout(timeout) {
            Ok(event) => event.map(Some),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => bail!("the audio helper stopped"),
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        // EOF lets ScreenCaptureKit release its stream; a stuck helper still has a deadline.
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(1);
        while self.child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        if self.child.try_wait().ok().flatten().is_none() { let _ = self.child.kill(); }
        let _ = self.child.wait();
        for thread in self.threads.drain(..) { let _ = thread.join(); }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    fn spawn(script: &str) -> Helper {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        Helper::spawn(command).unwrap()
    }
    #[test]
    fn eof_is_graceful_and_a_stuck_child_is_reaped() {
        let marker = std::env::temp_dir().join(format!("boardcast-helper-eof-{}", std::process::id()));
        let mut command = Command::new("sh");
        command.args(["-c", "read line; printf done > \"$1\"", "helper", marker.to_str().unwrap()]);
        drop(Helper::spawn(command).unwrap());
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "done");
        std::fs::remove_file(marker).unwrap();
        let started = Instant::now();
        drop(spawn("exec sleep 30"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn silence_cancel_and_bad_messages_are_errors() {
        let helper = spawn("exec sleep 30");
        assert!(helper.next(Duration::from_millis(20), &AtomicBool::new(false)).unwrap_err().to_string().contains("timed out"));
        assert!(helper.next(Duration::from_secs(5), &AtomicBool::new(true)).unwrap_err().to_string().contains("cancelled"));
        let malformed = spawn("printf 'broken\\n'");
        assert!(malformed.next(Duration::from_secs(2), &AtomicBool::new(false)).is_err());
    }
}
