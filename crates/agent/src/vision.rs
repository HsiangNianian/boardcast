//! Reading the text in a piece of an image with a local model. It runs on this machine only and goes
//! away when done, so a meeting using a remote model never sends it an image.

use crate::{list_gpus, pick_gpu, spawn_llama, wait_until_up, LocalServer, LLAMA_PORT};
use anyhow::{anyhow, bail, Context, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Child;
use std::time::Duration;

/// Beside the board model's port, as both may be up at once.
const VISION_PORT: u16 = LLAMA_PORT + 1;
const PROMPT: &str = "图里写的是什么字？原样输出这些字，不要解释，不要加引号。图里没有字就只输出：无";
const NEEDS_PROJECTOR: [&str; 4] = ["image", "mmproj", "multimodal", "vision"];

/// A model that can see, for as long as this value lives.
pub struct VisionServer {
    child: Child,
    base: String,
}

impl VisionServer {
    pub fn start(binaries: &Path, model: &Path, projector: &Path) -> Result<Self> {
        let server = LocalServer { binaries: binaries.to_path_buf(), model: model.to_path_buf() };
        let base = format!("http://127.0.0.1:{VISION_PORT}");
        let health = format!("{base}/health");
        let device = pick_gpu(&list_gpus(binaries));
        let mut child = spawn_llama(&server, device.as_deref(), VISION_PORT, Some(projector))?;
        if let Err(first) = wait_until_up(&health, &mut child) {
            let _ = child.kill();
            let _ = child.wait();
            if device.is_none() {
                return Err(first);
            }
            eprintln!("image model did not start on {device:?}: {first:#}; trying the CPU");
            child = spawn_llama(&server, None, VISION_PORT, Some(projector))?;
            wait_until_up(&health, &mut child)?;
        }
        Ok(Self { child, base })
    }

    pub fn read(&self, png: &[u8]) -> Result<String> {
        read_text(&self.base, png)
    }
}

impl Drop for VisionServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Ask a local server what a picture says. The image must not travel anywhere else, so nothing here
/// goes through a proxy or follows a redirect.
pub fn read_text(base: &str, png: &[u8]) -> Result<String> {
    let url = format!("data:image/png;base64,{}", BASE64.encode(png));
    let body = json!({
        "model": "local",
        "temperature": 0,
        "max_tokens": 160,
        "chat_template_kwargs": { "enable_thinking": false },
        "messages": [{ "role": "user", "content": [{ "type": "image_url", "image_url": { "url": url } }, { "type": "text", "text": PROMPT }] }],
    });
    let agent = ureq::AgentBuilder::new().try_proxy_from_env(false).redirects(0).timeout(Duration::from_secs(180)).build();
    let response = agent.post(&format!("{base}/v1/chat/completions")).send_json(&body);
    let reply: Value = match response {
        Ok(reply) if reply.status() == 200 => reply.into_json().context("reading the image model's reply")?,
        Ok(reply) => bail!("the image model answered with status {}", reply.status()),
        Err(ureq::Error::Status(code, reply)) => {
            let text = reply.into_string().unwrap_or_default().to_lowercase();
            if NEEDS_PROJECTOR.iter().any(|word| text.contains(word)) {
                return Err(anyhow!("this local model can't read images"));
            }
            bail!("the image model answered with status {code}");
        }
        Err(e) => return Err(e).context("asking the image model"),
    };
    Ok(reply["choices"][0]["message"]["content"].as_str().unwrap_or_default().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A server that gives this reply to everything, and counts what it was asked.
    fn answering(status_line: &'static str, headers: &'static str, body: &'static str) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                seen.fetch_add(1, Ordering::SeqCst);
                let mut request = vec![0_u8; 1 << 16];
                let _ = stream.read(&mut request);
                let _ = write!(stream, "HTTP/1.1 {status_line}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        (base, count)
    }

    #[test]
    fn a_redirect_is_never_followed_with_the_image() {
        let (base, count) = answering("307 Temporary Redirect", "Location: /somewhere-else\r\n", "");
        assert!(read_text(&base, b"picture").is_err());
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_reply_is_read_and_a_model_that_cannot_see_says_so() {
        let (base, _) = answering("200 OK", "Content-Type: application/json\r\n", r#"{"choices":[{"message":{"content":"订单服务"}}]}"#);
        assert_eq!(read_text(&base, b"picture").unwrap(), "订单服务");
        let (base, _) = answering("500 Internal Server Error", "", "image input is not supported, no mmproj");
        assert!(format!("{:#}", read_text(&base, b"picture").unwrap_err()).contains("can't read images"));
    }
}
