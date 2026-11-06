//! The board agent: turns finished speech into board operations through a local or remote model.
//!
//! Evidence for the provider defaults, from the eval sets:
//! the layout rules lowered Qwen 4B's holdout scores, so local runs leave them out;
//! they raised DeepSeek's node and edge precision on holdout with recall unchanged, so remote runs use them.
//! Hints from the checker did not help on either model, so they stay opt-in.

use anyhow::{anyhow, Context, Result};
use boardcast_board::graph::{board_is_english, grammar, parse_ops, user_message, Graph, Utterance};
use serde_json::{json, Value};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const CONTEXT_LINES: usize = 2;
const LLAMA_PORT: u16 = 18_765;
const DEEPSEEK: &str = "https://api.deepseek.com/chat/completions";
const ATTEMPTS: u32 = 4;

/// The layout skill's rules, short enough for a prompt. The full guidance is skills/board-layout/SKILL.md.
pub const SKILL_RULES: &str = include_str!("../skills/board-layout/RULES.md");

/// The fixed examples and system prompt, recorded from the TypeScript engine.
fn prompts() -> Value {
    serde_json::from_str(include_str!("../prompts.json")).expect("prompts.json is valid")
}

pub enum Provider {
    /// A llama-server: output is constrained by a grammar built from the current board.
    Local { base: String },
    /// DeepSeek with thinking on, as the user runs it. The key is held in memory only.
    DeepSeek { key: String },
}

impl Provider {
    /// `Err` if the environment names no key. The key is never logged or written anywhere.
    pub fn deepseek_from_env() -> Result<Self> {
        Ok(Self::DeepSeek { key: std::env::var("DEEPSEEK_API_KEY").context("DEEPSEEK_API_KEY is not set")? })
    }

    pub fn default_rules(&self) -> bool {
        matches!(self, Self::DeepSeek { .. })
    }

    fn call(&self, messages: Vec<Value>, graph: &Graph, english: bool) -> Result<Value> {
        let (url, body) = match self {
            Self::DeepSeek { .. } => (
                DEEPSEEK.to_string(),
                json!({ "model": "deepseek-flash", "messages": messages, "temperature": 0, "max_tokens": 4096, "thinking": { "type": "enabled" }, "reasoning_effort": "high", "stream": false }),
            ),
            Self::Local { base } => (
                format!("{base}/v1/chat/completions"),
                json!({ "model": "local", "messages": messages, "temperature": 0, "max_tokens": 160, "grammar": grammar(graph, 8, english), "cache_prompt": true, "chat_template_kwargs": { "enable_thinking": false } }),
            ),
        };
        let mut last = anyhow!("no attempt made");
        for attempt in 0..ATTEMPTS {
            let mut call = ureq::post(&url).timeout(Duration::from_secs(300));
            if let Self::DeepSeek { key } = self {
                call = call.set("Authorization", &format!("Bearer {key}"));
            }
            match call.send_json(body.clone()) {
                Ok(reply) => return reply.into_json().map_err(Into::into),
                // A rejected request will not improve by retrying, and its body names the reason.
                Err(ureq::Error::Status(code, reply)) if code != 429 && code < 500 => {
                    return Err(anyhow!("model answered {code}: {}", reply.into_string().unwrap_or_default()));
                }
                Err(e) => last = anyhow!("model request failed: {e}"),
            }
            thread::sleep(Duration::from_secs(2_u64.pow(attempt + 1)));
        }
        Err(last)
    }
}

/// What to add to the prompt beyond the fixed instructions.
#[derive(Clone, Copy, Default)]
pub struct Guidance {
    pub rules: bool,
    pub hint: bool,
}

pub struct Reply {
    pub ops: String,
    pub ms: u128,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

/// One model turn for one line of speech.
pub fn propose(provider: &Provider, graph: &Graph, recent: &[Utterance], next: &Utterance, guidance: Guidance, hint: Option<&str>) -> Result<Reply> {
    let english = board_is_english(recent, next);
    let all = prompts();
    let mut messages: Vec<Value> = all[if english { "en" } else { "zh" }].as_array().cloned().context("prompts")?;
    if guidance.rules {
        // Appended to the system prompt, which never changes, so prefix caching still applies.
        let text = messages[0]["content"].as_str().unwrap_or("").to_string();
        messages[0]["content"] = json!(format!("{text}\n\n{SKILL_RULES}"));
    }
    let mut content = user_message(graph, recent, next);
    if let Some(h) = hint.filter(|_| guidance.hint) {
        content = format!("{h}\n{content}");
    }
    messages.push(json!({ "role": "user", "content": content }));
    let started = Instant::now();
    let reply = provider.call(messages, graph, english)?;
    Ok(Reply {
        ops: reply["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string(),
        ms: started.elapsed().as_millis(),
        tokens_in: reply["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        tokens_out: reply["usage"]["completion_tokens"].as_u64().unwrap_or(0),
    })
}

pub struct Said {
    pub line: Utterance,
    /// When speech ended or the line was submitted; end-to-end timings start here.
    pub ended: Instant,
    pub asr_ms: Option<u128>,
}

pub enum Update {
    Status(String),
    Drawn { graph: Graph, ops: String, text: String, asr_ms: Option<u128>, llm_ms: u128, batch: usize, ended: Instant },
    Failed(String),
}

pub struct LocalServer {
    /// A boardcast checkout's `src-tauri/llama`, until the app bundles its own copy.
    pub binaries: PathBuf,
    pub model: PathBuf,
}

/// The same launch arguments as the Tauri app. The child dies with the thread that started it.
fn spawn_llama(server: &LocalServer) -> Result<Child> {
    let device = std::env::var("LLAMA_DEVICE").unwrap_or_else(|_| "Vulkan1".into());
    let mut cmd = Command::new(server.binaries.join("llama-server"));
    cmd.env("LD_LIBRARY_PATH", &server.binaries)
        .arg("-m")
        .arg(&server.model)
        .args(["--host", "127.0.0.1", "--port", &LLAMA_PORT.to_string()])
        .args(["-c", "4096", "-np", "1", "--jinja"])
        .args(["--device", &device, "-ngl", "auto", "--fit", "on", "-fa", "auto"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: prctl is async-signal-safe and touches no parent state between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    }
    cmd.spawn().context("starting llama-server")
}

fn wait_for(url: &str) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(180) {
        if ureq::get(url).timeout(Duration::from_secs(2)).call().is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(anyhow!("timed out waiting for {url}"))
}

pub enum Backend {
    /// Start a llama-server and talk to it.
    ManagedLocal(LocalServer),
    Ready(Provider),
}

/// The running session: one request at a time, with lines heard meanwhile merged into the next.
/// `graph` is shared so a hand-made mark takes effect at once; a reply applies on the latest board.
pub struct Session {
    pub graph: Arc<Mutex<Graph>>,
    pub hint: Arc<Mutex<Option<String>>>,
}

impl Session {
    pub fn start(backend: Backend, guidance: Option<Guidance>, said: Receiver<Said>, ready: Sender<()>, drained: Sender<()>, updates: Sender<Update>) -> Self {
        let graph = Arc::new(Mutex::new(Graph::default()));
        let hint = Arc::new(Mutex::new(None::<String>));
        let (shared, shared_hint) = (graph.clone(), hint.clone());
        thread::spawn(move || {
            let run = || -> Result<()> {
                // Held for the thread's lifetime: the child's death signal is tied to this thread.
                let mut _server = None;
                let provider = match backend {
                    Backend::Ready(p) => p,
                    Backend::ManagedLocal(server) => {
                        let _ = updates.send(Update::Status("正在加载板书模型…".into()));
                        _server = Some(spawn_llama(&server)?);
                        let base = format!("http://127.0.0.1:{LLAMA_PORT}");
                        wait_for(&format!("{base}/health"))?;
                        Provider::Local { base }
                    }
                };
                let guidance = guidance.unwrap_or(Guidance { rules: provider.default_rules(), hint: false });
                let _ = updates.send(Update::Status("板书模型就绪".into()));
                let _ = ready.send(());
                let mut history: Vec<Utterance> = vec![];
                while let Ok(first) = said.recv() {
                    let mut batch = vec![first];
                    while let Ok(more) = said.try_recv() {
                        batch.push(more);
                    }
                    let next = if batch.len() == 1 {
                        batch[0].line.clone()
                    } else {
                        let text: Vec<String> = batch
                            .iter()
                            .map(|s| s.line.speaker.as_ref().map_or_else(|| s.line.text.clone(), |n| format!("{n}：{}", s.line.text)))
                            .collect();
                        Utterance { speaker: None, text: text.join(" ") }
                    };
                    let recent = &history[history.len().saturating_sub(CONTEXT_LINES)..];
                    let seen = shared.lock().expect("graph lock").clone();
                    let note = shared_hint.lock().expect("hint lock").take();
                    match propose(&provider, &seen, recent, &next, guidance, note.as_deref()) {
                        Ok(reply) => {
                            let current = {
                                let mut g = shared.lock().expect("graph lock");
                                g.apply(&parse_ops(&reply.ops));
                                g.clone()
                            };
                            let tail = &batch[batch.len() - 1];
                            let _ = updates.send(Update::Drawn {
                                graph: current,
                                ops: reply.ops,
                                text: tail.line.text.clone(),
                                asr_ms: tail.asr_ms,
                                llm_ms: reply.ms,
                                batch: batch.len(),
                                ended: batch[0].ended,
                            });
                        }
                        Err(e) => {
                            let _ = updates.send(Update::Failed(e.to_string()));
                        }
                    }
                    history.extend(batch.into_iter().map(|s| s.line));
                    let _ = drained.send(());
                }
                Ok(())
            };
            if let Err(e) = run() {
                let _ = updates.send(Update::Failed(format!("{e:#}")));
            }
        });
        Self { graph, hint }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shipped_prompts_match_the_typescript_engine() {
        let goldens: serde_json::Value = serde_json::from_str(include_str!("../../board/fixtures/goldens.json")).unwrap();
        assert_eq!(super::prompts(), goldens["prompts"]);
    }

    #[test]
    fn rules_follow_the_eval_results_for_each_provider() {
        assert!(!super::Provider::Local { base: String::new() }.default_rules());
        assert!(super::Provider::DeepSeek { key: String::new() }.default_rules());
    }
}
