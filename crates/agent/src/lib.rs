//! The board agent: turns finished speech into board operations through a local or remote model.
//!
//! Evidence for the provider defaults, from the eval sets:
//! the layout rules lowered Qwen 4B's holdout scores, so local runs leave them out;
//! they raised DeepSeek's node and edge precision on holdout with recall unchanged, so DeepSeek uses them.
//! Other remote models have not been measured, so they go without.
//! Hints from the checker did not help on either model, so they stay opt-in.

pub mod settings;
pub mod summary;

use anyhow::{anyhow, bail, Context, Result};
use boardcast_board::graph::{board_is_english, grammar, parse_ops, user_message, Graph, Utterance};
use serde_json::{json, Value};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const CONTEXT_LINES: usize = 2;
const LLAMA_PORT: u16 = 18_765;
const ATTEMPTS: u32 = 4;
const NO_MODEL: &str = "No board model is set up. Download one or add a remote model in Settings.";

/// The layout skill's rules, short enough for a prompt. The full guidance is skills/board-layout/SKILL.md.
pub const SKILL_RULES: &str = include_str!("../skills/board-layout/RULES.md");

/// The fixed examples and system prompt, recorded from the TypeScript engine.
fn prompts() -> Value {
    serde_json::from_str(include_str!("../prompts.json")).expect("prompts.json is valid")
}

#[derive(Clone)]
pub enum Provider {
    /// A llama-server: output is constrained by a grammar built from the current board.
    Local { base: String },
    /// An OpenAI-compatible service. The key is held in memory and in `remote.json` only.
    Remote { base_url: String, key: String, model: String },
    /// A scripted reply, for testing what happens around a model turn.
    #[cfg(test)]
    Scripted(Arc<dyn Fn(&str) -> Result<String> + Send + Sync>),
}

/// `<base>/chat/completions`, unless the setting already names the full path.
fn chat_url(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") { base.to_string() } else { format!("{base}/chat/completions") }
}

/// Reasoning models may write their thinking into the reply; only what follows it is the answer.
fn strip_thinking(text: &str) -> &str {
    text.rfind("</think>").map_or(text, |at| &text[at + "</think>".len()..]).trim()
}

impl Provider {
    /// DeepSeek with the settings the user runs it with. `Err` if the environment names no key.
    pub fn deepseek_from_env() -> Result<Self> {
        Ok(Self::Remote {
            base_url: "https://api.deepseek.com".into(),
            key: std::env::var("DEEPSEEK_API_KEY").context("DEEPSEEK_API_KEY is not set")?,
            model: "deepseek-flash".into(),
        })
    }

    fn is_deepseek(&self) -> bool {
        matches!(self, Self::Remote { base_url, .. } if base_url.contains("deepseek.com"))
    }

    pub fn default_guidance(&self) -> Guidance {
        Guidance { rules: self.is_deepseek(), hint: false }
    }

    /// An ordinary question with no grammar, as a summary needs.
    pub fn chat(&self, messages: &[Value], max_tokens: u32) -> Result<String> {
        let body = match self {
            Self::Remote { model, .. } => {
                let mut body = json!({ "model": model, "messages": messages, "temperature": 0.2, "max_tokens": max_tokens, "stream": false });
                if self.is_deepseek() {
                    body["thinking"] = json!({ "type": "enabled" });
                    body["reasoning_effort"] = json!("high");
                }
                body
            }
            Self::Local { .. } => json!({ "model": "local", "messages": messages, "temperature": 0.2, "max_tokens": max_tokens, "cache_prompt": true, "chat_template_kwargs": { "enable_thinking": false } }),
            #[cfg(test)]
            Self::Scripted(reply) => {
                let said = messages.last().and_then(|m| m["content"].as_str()).unwrap_or_default().to_string();
                return reply(&said);
            }
        };
        let reply = self.send(&self.endpoint(), &body)?;
        Ok(strip_thinking(reply["choices"][0]["message"]["content"].as_str().unwrap_or("")).to_string())
    }

    fn endpoint(&self) -> String {
        match self {
            Self::Remote { base_url, .. } => chat_url(base_url),
            Self::Local { base } => format!("{base}/v1/chat/completions"),
            #[cfg(test)]
            Self::Scripted(_) => String::new(),
        }
    }

    fn call(&self, messages: Vec<Value>, graph: &Graph, english: bool) -> Result<Value> {
        let (url, body) = match self {
            Self::Remote { base_url, model, .. } => {
                let mut body = json!({ "model": model, "messages": messages, "temperature": 0, "max_tokens": 4096, "stream": false });
                if self.is_deepseek() {
                    body["thinking"] = json!({ "type": "enabled" });
                    body["reasoning_effort"] = json!("high");
                }
                (chat_url(base_url), body)
            }
            Self::Local { base } => (
                format!("{base}/v1/chat/completions"),
                json!({ "model": "local", "messages": messages, "temperature": 0, "max_tokens": 160, "grammar": grammar(graph, 8, english), "cache_prompt": true, "chat_template_kwargs": { "enable_thinking": false } }),
            ),
            #[cfg(test)]
            Self::Scripted(reply) => {
                let said = messages.last().and_then(|m| m["content"].as_str()).unwrap_or_default().to_string();
                return Ok(json!({ "choices": [{ "message": { "content": reply(&said)? } }] }));
            }
        };
        self.send(&url, &body)
    }

    fn send(&self, url: &str, body: &Value) -> Result<Value> {
        let mut last = anyhow!("no attempt made");
        for attempt in 0..ATTEMPTS {
            let mut call = http().post(url).timeout(Duration::from_secs(300));
            if let Self::Remote { key, .. } = self {
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
            if attempt + 1 < ATTEMPTS {
                thread::sleep(Duration::from_secs(2_u64.pow(attempt + 1)));
            }
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
        ops: strip_thinking(reply["choices"][0]["message"]["content"].as_str().unwrap_or("")).to_string(),
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
    /// The meeting this was said in, from `Session::meeting`. A line from another meeting is dropped.
    pub meeting: u64,
}

pub enum Update {
    Loading,
    Ready,
    /// Nothing is installed and no remote model is set up, so the user has to choose.
    NoModel,
    Drawn { meeting: u64, graph: Graph, ops: String, lines: Vec<Utterance>, asr_ms: Option<u128>, llm_ms: u128, ended: Instant },
    Failed { meeting: u64, message: String, lines: Vec<Utterance> },
    /// Something went wrong outside any line, like the local model failing to start.
    Broken(String),
}

pub struct LocalServer {
    /// The directory holding `llama-server` and its libraries.
    pub binaries: PathBuf,
    pub model: PathBuf,
}

/// A graphics device llama-server can use.
#[derive(Clone, Debug, PartialEq)]
pub struct Gpu {
    pub id: String,
    pub name: String,
    pub total_mib: u64,
    pub discrete: bool,
}

/// Parse `--list-devices`, whose rows look like `Vulkan1: NVIDIA GeForce RTX 3060 (6144 MiB, 5822 MiB free)`.
fn parse_devices(text: &str) -> Vec<Gpu> {
    const DISCRETE: [&str; 6] = ["nvidia", "geforce", "radeon rx", "radeon pro", " arc", "adreno"];
    text.lines()
        .filter_map(|line| {
            let (id, rest) = line.trim().split_once(": ")?;
            if id.is_empty() || id.contains(' ') {
                return None;
            }
            // Log lines share the "word: text" shape, but only devices state their memory.
            let (name, memory) = rest.rsplit_once(" (")?;
            let total_mib = memory.split_once(" MiB")?.0.trim().parse().ok()?;
            let lower = name.to_lowercase();
            Some(Gpu { id: id.to_string(), name: name.to_string(), total_mib, discrete: DISCRETE.iter().any(|k| lower.contains(k)) })
        })
        .collect()
}

fn server_command(binaries: &std::path::Path) -> Command {
    let mut cmd = Command::new(binaries.join("llama-server"));
    cmd.env("LD_LIBRARY_PATH", binaries).stdin(Stdio::null());
    cmd
}

/// The graphics devices on this machine; empty if there are none or the server cannot run.
pub fn list_gpus(binaries: &std::path::Path) -> Vec<Gpu> {
    let Ok(out) = server_command(binaries).arg("--list-devices").output() else { return vec![] };
    parse_devices(&(String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr)))
}

/// The discrete card with the most memory, else any device. Numbering differs per machine, so it
/// cannot be assumed. `LLAMA_DEVICE` overrides this for testing.
fn pick_gpu(gpus: &[Gpu]) -> Option<String> {
    if let Ok(device) = std::env::var("LLAMA_DEVICE") {
        return Some(device);
    }
    gpus.iter().filter(|g| g.discrete).max_by_key(|g| g.total_mib).or_else(|| gpus.first()).map(|g| g.id.clone())
}

/// The same launch arguments as the older app. The child dies with the thread that started it.
fn spawn_llama(server: &LocalServer, device: Option<&str>) -> Result<Child> {
    let mut cmd = server_command(&server.binaries);
    cmd.arg("-m")
        .arg(&server.model)
        .args(["--host", "127.0.0.1", "--port", &LLAMA_PORT.to_string()])
        .args(["-c", "4096", "-np", "1", "--jinja"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match device {
        // Not a fixed layer count: if the model does not fit in VRAM, as many layers as fit go there.
        Some(device) => cmd.args(["--device", device, "-ngl", "auto", "--fit", "on", "-fa", "auto"]),
        None => cmd.args(["--device", "none", "-ngl", "0"]),
    };
    // SAFETY: prctl is async-signal-safe and touches no parent state between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    }
    cmd.spawn().with_context(|| format!("starting {}", server.binaries.join("llama-server").display()))
}

/// Start the server on the best device, falling back to the CPU if that fails to come up.
fn start_llama(server: &LocalServer) -> Result<(Child, String)> {
    let base = format!("http://127.0.0.1:{LLAMA_PORT}");
    let health = format!("{base}/health");
    let device = pick_gpu(&list_gpus(&server.binaries));
    let mut child = spawn_llama(server, device.as_deref())?;
    match wait_until_up(&health, &mut child) {
        Ok(()) => Ok((child, base)),
        Err(first) if device.is_some() => {
            eprintln!("local model did not start on {device:?}: {first:#}; trying the CPU");
            let _ = child.kill();
            let _ = child.wait();
            let mut child = spawn_llama(server, None)?;
            wait_until_up(&health, &mut child)?;
            Ok((child, base))
        }
        Err(first) => Err(first),
    }
}

/// Wait for the server to answer, giving up early if it has already exited.
fn wait_until_up(url: &str, child: &mut Child) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(180) {
        if let Some(status) = child.try_wait()? {
            bail!("llama-server exited during startup ({status})");
        }
        if http().get(url).timeout(Duration::from_secs(2)).call().is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(anyhow!("timed out waiting for {url}"))
}

/// Requests honour the proxy settings in the environment, as the older app does.
fn http() -> ureq::Agent {
    ureq::AgentBuilder::new().try_proxy_from_env(true).build()
}

/// Which model answers the next line. It can change at any time from the settings.
#[derive(Clone)]
pub enum Backend {
    /// Start a llama-server the first time it is needed, and talk to it.
    ManagedLocal(Arc<LocalServer>),
    Ready(Provider),
    /// No model is available yet: nothing is installed and no remote model is set up.
    NoModel,
}

/// The running session: one request at a time, with lines heard meanwhile merged into the next.
///
/// `graph` is shared so a hand-made mark takes effect at once; a reply applies on the latest board.
/// Opening another meeting bumps `meeting`, which fences off everything still in flight: a reply
/// that arrives late, or a line still queued, can never reach the new meeting's board or context.
pub struct Session {
    graph: Arc<Mutex<Graph>>,
    history: Arc<Mutex<Vec<Utterance>>>,
    hint: Arc<Mutex<Option<String>>>,
    backend: Arc<Mutex<Backend>>,
    guidance: Arc<Mutex<Option<Guidance>>>,
    meeting: Arc<AtomicU64>,
    /// Where the local model is answering, once it has started.
    local_base: Arc<Mutex<Option<String>>>,
}

impl Session {
    pub fn start(backend: Backend, guidance: Option<Guidance>, said: Receiver<Said>, ready: Sender<()>, drained: Sender<()>, updates: Sender<Update>) -> Self {
        let session = Self {
            graph: Arc::new(Mutex::new(Graph::default())),
            history: Arc::new(Mutex::new(vec![])),
            hint: Arc::new(Mutex::new(None)),
            backend: Arc::new(Mutex::new(backend)),
            guidance: Arc::new(Mutex::new(guidance)),
            meeting: Arc::new(AtomicU64::new(0)),
            local_base: Arc::new(Mutex::new(None)),
        };
        let (graph, history, hint, backend, guidance, meeting) =
            (session.graph.clone(), session.history.clone(), session.hint.clone(), session.backend.clone(), session.guidance.clone(), session.meeting.clone());
        let local_base = session.local_base.clone();
        thread::spawn(move || {
            // Held for the thread's lifetime: the child's death signal is tied to this thread.
            let mut server: Option<(Child, String, PathBuf)> = None;
            let mut provider_now = |updates: &Sender<Update>| -> Result<Provider> {
                let choice = backend.lock().expect("backend lock").clone();
                match choice {
                    Backend::Ready(provider) => Ok(provider),
                    Backend::NoModel => bail!("{NO_MODEL}"),
                    Backend::ManagedLocal(local) => {
                        // Another model was chosen: the running server cannot switch.
                        if server.as_ref().is_some_and(|(_, _, model)| model != &local.model) {
                            *local_base.lock().expect("base lock") = None;
                            if let Some((mut child, _, _)) = server.take() {
                                let _ = child.kill();
                                let _ = child.wait();
                            }
                        }
                        if server.is_none() {
                            let _ = updates.send(Update::Loading);
                            let (child, base) = start_llama(&local)?;
                            let _ = updates.send(Update::Ready);
                            *local_base.lock().expect("base lock") = Some(base.clone());
                            server = Some((child, base, local.model.clone()));
                        }
                        Ok(Provider::Local { base: server.as_ref().map(|(_, b, _)| b.clone()).expect("just set") })
                    }
                }
            };
            let first_use = provider_now(&updates);
            match &first_use {
                Ok(Provider::Local { .. }) => {}
                Ok(_) => {
                    let _ = updates.send(Update::Ready);
                }
                Err(e) if e.to_string() == NO_MODEL => {
                    let _ = updates.send(Update::NoModel);
                }
                Err(e) => {
                    let _ = updates.send(Update::Broken(format!("{e:#}")));
                }
            }
            let _ = ready.send(());
            while let Ok(first) = said.recv() {
                let mut batch = vec![first];
                while let Ok(more) = said.try_recv() {
                    batch.push(more);
                }
                let current = meeting.load(Ordering::SeqCst);
                batch.retain(|s| s.meeting == current);
                if batch.is_empty() {
                    let _ = drained.send(());
                    continue;
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
                let recent: Vec<Utterance> = {
                    let all = history.lock().expect("history lock");
                    all[all.len().saturating_sub(CONTEXT_LINES)..].to_vec()
                };
                let seen = graph.lock().expect("graph lock").clone();
                let note = hint.lock().expect("hint lock").take();
                let lines: Vec<Utterance> = batch.iter().map(|s| s.line.clone()).collect();
                let result = provider_now(&updates).and_then(|provider| {
                    let chosen = guidance.lock().expect("guidance lock").unwrap_or_else(|| provider.default_guidance());
                    propose(&provider, &seen, &recent, &next, chosen, note.as_deref())
                });
                // The lock is held across the check, so a meeting cannot open between it and the write.
                let mut board = graph.lock().expect("graph lock");
                if meeting.load(Ordering::SeqCst) != current {
                    drop(board);
                    let _ = drained.send(());
                    continue;
                }
                history.lock().expect("history lock").extend(lines.iter().cloned());
                let tail = &batch[batch.len() - 1];
                let update = match result {
                    Ok(reply) => {
                        board.apply(&parse_ops(&reply.ops));
                        Update::Drawn { meeting: current, graph: board.clone(), ops: reply.ops, lines, asr_ms: tail.asr_ms, llm_ms: reply.ms, ended: batch[0].ended }
                    }
                    Err(e) => Update::Failed { meeting: current, message: format!("{e:#}"), lines },
                };
                drop(board);
                let _ = updates.send(update);
                let _ = drained.send(());
            }
        });
        session
    }

    /// A handle on the meeting stamp, for threads that hear lines.
    pub fn stamp(&self) -> Arc<AtomicU64> {
        self.meeting.clone()
    }

    /// The current meeting's stamp, for lines being said now and for checking updates.
    pub fn meeting(&self) -> u64 {
        self.meeting.load(Ordering::SeqCst)
    }

    /// Switch to another meeting's board and context. Anything still in flight is discarded.
    pub fn open(&self, graph: Graph, history: Vec<Utterance>) {
        let mut board = self.graph.lock().expect("graph lock");
        self.meeting.fetch_add(1, Ordering::SeqCst);
        *board = graph;
        *self.history.lock().expect("history lock") = history;
        *self.hint.lock().expect("hint lock") = None;
    }

    /// Change the board without touching context or anything in flight, as undo does.
    pub fn set_graph(&self, graph: Graph) {
        *self.graph.lock().expect("graph lock") = graph;
    }

    pub fn board(&self) -> Graph {
        self.graph.lock().expect("graph lock").clone()
    }

    /// A hand-made mark applies at once rather than waiting for a model reply.
    pub fn toggle_mark(&self, id: &str, mark: &str) -> Graph {
        let mut graph = self.graph.lock().expect("graph lock");
        graph.toggle_mark(id, mark);
        graph.clone()
    }

    pub fn set_hint(&self, hint: String) {
        *self.hint.lock().expect("hint lock") = Some(hint);
    }

    /// The model to ask for a summary: the one drawing the board, without starting anything new.
    pub fn provider(&self) -> Result<Provider> {
        let choice = self.backend.lock().expect("backend lock").clone();
        match choice {
            Backend::Ready(provider) => Ok(provider),
            Backend::NoModel => bail!("{NO_MODEL}"),
            Backend::ManagedLocal(_) => {
                let base = self.local_base.lock().expect("base lock").clone();
                base.map(|base| Provider::Local { base }).context("The local model is still loading. Try again in a moment.")
            }
        }
    }

    pub fn set_backend(&self, backend: Backend) {
        *self.backend.lock().expect("backend lock") = backend;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shipped_prompts_match_the_typescript_engine() {
        let goldens: serde_json::Value = serde_json::from_str(include_str!("../../board/fixtures/goldens.json")).unwrap();
        assert_eq!(super::prompts(), goldens["prompts"]);
    }

    use super::*;
    use std::sync::mpsc::{channel, RecvTimeoutError};

    #[test]
    fn rules_follow_the_eval_results_for_each_provider() {
        assert!(!Provider::Local { base: String::new() }.default_guidance().rules);
        let remote = |url: &str| Provider::Remote { base_url: url.into(), key: String::new(), model: String::new() };
        assert!(remote("https://api.deepseek.com").default_guidance().rules);
        assert!(!remote("https://example.com/v1").default_guidance().rules, "only measured on DeepSeek");
        assert!(!remote("https://api.deepseek.com").default_guidance().hint);
    }

    #[test]
    fn devices_are_read_from_the_server_listing() {
        let listing = "load_backend: loaded Vulkan backend\nAvailable devices:\n  Vulkan0: Intel(R) Iris(R) Xe Graphics (ADL GT2) (23827 MiB, 9687 MiB free)\n  Vulkan1: NVIDIA GeForce RTX 3060 Laptop GPU (6144 MiB, 5800 MiB free)\n";
        let gpus = parse_devices(listing);
        assert_eq!(gpus.len(), 2, "{gpus:?}");
        assert_eq!(gpus[0].name, "Intel(R) Iris(R) Xe Graphics (ADL GT2)");
        assert_eq!((gpus[0].total_mib, gpus[0].discrete), (23827, false));
        assert_eq!((gpus[1].id.as_str(), gpus[1].total_mib, gpus[1].discrete), ("Vulkan1", 6144, true));
        assert_eq!(pick_gpu(&gpus).as_deref(), Some("Vulkan1"), "a discrete card wins over a larger shared memory");
        assert_eq!(pick_gpu(&gpus[..1]).as_deref(), Some("Vulkan0"));
        assert_eq!(pick_gpu(&[]), None, "no graphics device means the CPU");
        assert!(parse_devices("Available devices:\n").is_empty());
    }

    #[test]
    fn urls_and_thinking_are_normalized() {
        assert_eq!(chat_url("https://x.com/v1/"), "https://x.com/v1/chat/completions");
        assert_eq!(chat_url(" https://x.com/v1/chat/completions "), "https://x.com/v1/chat/completions");
        assert_eq!(strip_thinking("<think>嗯</think>\nE 「甲」 -> 「乙」"), "E 「甲」 -> 「乙」");
        assert_eq!(strip_thinking(" T 「甲」 "), "T 「甲」");
    }

    struct Harness {
        session: Session,
        said: Sender<Said>,
        updates: Receiver<Update>,
        drained: Receiver<()>,
    }

    fn harness(reply: impl Fn(&str) -> Result<String> + Send + Sync + 'static) -> Harness {
        let (said, said_rx) = channel();
        let (ready_tx, ready_rx) = channel();
        let (drained_tx, drained) = channel();
        let (updates_tx, updates) = channel();
        let session = Session::start(Backend::Ready(Provider::Scripted(Arc::new(reply))), None, said_rx, ready_tx, drained_tx, updates_tx);
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), Update::Ready));
        Harness { session, said, updates, drained }
    }

    impl Harness {
        fn say(&self, text: &str) {
            self.said.send(Said { line: Utterance { speaker: None, text: text.into() }, ended: Instant::now(), asr_ms: None, meeting: self.session.meeting() }).unwrap();
        }

        fn wait(&self) {
            self.drained.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    }

    #[test]
    fn a_reply_draws_on_the_board_and_joins_the_context() {
        let h = harness(|_| Ok("E 「甲」 -> 「乙」".into()));
        h.say("甲调乙");
        h.wait();
        let Update::Drawn { graph, lines, .. } = h.updates.recv_timeout(Duration::from_secs(5)).unwrap() else { panic!("expected a drawing") };
        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(lines[0].text, "甲调乙");
        assert_eq!(h.session.board().nodes.len(), 2);
        assert_eq!(h.session.history.lock().unwrap().len(), 1);
    }

    /// The old app's #60: a reply that arrives after switching meetings wrote into the new one.
    #[test]
    fn a_late_reply_never_reaches_the_next_meeting() {
        let (release_tx, release) = channel::<()>();
        let (started_tx, started) = channel::<()>();
        let release = Mutex::new(release);
        let h = harness(move |_| {
            started_tx.send(()).unwrap();
            release.lock().unwrap().recv_timeout(Duration::from_secs(5)).map_err(|e| anyhow!("{e}"))?;
            Ok("E 「旧会议」 -> 「不该出现」".into())
        });
        h.say("上一场会说的话");
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        h.session.open(Graph::default(), vec![]);
        release_tx.send(()).unwrap();
        h.wait();
        assert!(h.session.board().nodes.is_empty(), "the old reply reached the new board");
        assert!(h.session.history.lock().unwrap().is_empty(), "the old line reached the new context");
        assert!(matches!(h.updates.recv_timeout(Duration::from_millis(100)), Err(RecvTimeoutError::Timeout)), "the UI heard about it");
    }

    #[test]
    fn a_late_failure_stays_silent_too() {
        let (release_tx, release) = channel::<()>();
        let (started_tx, started) = channel::<()>();
        let release = Mutex::new(release);
        let h = harness(move |_| {
            started_tx.send(()).unwrap();
            let _ = release.lock().unwrap().recv_timeout(Duration::from_secs(5));
            Err(anyhow!("late failure"))
        });
        h.say("上一场");
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        h.session.open(Graph::default(), vec![]);
        release_tx.send(()).unwrap();
        h.wait();
        assert!(matches!(h.updates.recv_timeout(Duration::from_millis(100)), Err(RecvTimeoutError::Timeout)));
        assert!(h.session.history.lock().unwrap().is_empty());
    }

    #[test]
    fn lines_queued_for_an_old_meeting_are_dropped() {
        let seen = Arc::new(Mutex::new(vec![]));
        let log = seen.clone();
        let h = harness(move |said| {
            log.lock().unwrap().push(said.to_string());
            Ok("-".into())
        });
        let stale = h.session.meeting();
        h.session.open(Graph::default(), vec![]);
        h.said.send(Said { line: Utterance { speaker: None, text: "排队的旧话".into() }, ended: Instant::now(), asr_ms: None, meeting: stale }).unwrap();
        h.wait();
        h.say("新会议的话");
        h.wait();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].contains("新会议的话") && !seen[0].contains("排队的旧话"));
    }

    #[test]
    fn a_mark_made_during_a_request_survives_its_reply() {
        let (release_tx, release) = channel::<()>();
        let (started_tx, started) = channel::<()>();
        let release = Mutex::new(release);
        let h = harness(move |_| {
            started_tx.send(()).unwrap();
            release.lock().unwrap().recv_timeout(Duration::from_secs(5)).map_err(|e| anyhow!("{e}"))?;
            Ok("T 「新话题」".into())
        });
        let mut board = Graph::default();
        board.apply(&parse_ops("T 「旧话题」"));
        let id = board.nodes[0].id.clone();
        h.session.open(board, vec![]);
        h.say("再加一个话题");
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        h.session.toggle_mark(&id, "星标");
        release_tx.send(()).unwrap();
        h.wait();
        let board = h.session.board();
        assert_eq!(board.nodes.len(), 2);
        assert_eq!(board.nodes[0].marks, ["星标"]);
    }

    #[test]
    fn with_no_model_the_user_is_told_and_a_line_fails_visibly() {
        let (said, said_rx) = channel();
        let (ready_tx, ready_rx) = channel();
        let (drained_tx, drained) = channel();
        let (updates_tx, updates) = channel();
        let session = Session::start(Backend::NoModel, None, said_rx, ready_tx, drained_tx, updates_tx);
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), Update::NoModel));
        said.send(Said { line: Utterance { speaker: None, text: "这句没有模型".into() }, ended: Instant::now(), asr_ms: None, meeting: session.meeting() }).unwrap();
        drained.recv_timeout(Duration::from_secs(5)).unwrap();
        let Update::Failed { lines, .. } = updates.recv_timeout(Duration::from_secs(5)).unwrap() else { panic!("expected a failure") };
        assert_eq!(lines[0].text, "这句没有模型");
        session.set_backend(Backend::Ready(Provider::Scripted(Arc::new(|_| Ok("T 「现在有了」".into())))));
        said.send(Said { line: Utterance { speaker: None, text: "再试".into() }, ended: Instant::now(), asr_ms: None, meeting: session.meeting() }).unwrap();
        drained.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(session.board().nodes.len(), 1, "a model added later takes effect");
    }

    #[test]
    fn a_failed_request_reports_its_lines_and_keeps_the_board() {
        let h = harness(|_| Err(anyhow!("offline")));
        h.say("这句没画上");
        h.wait();
        let Update::Failed { lines, message, .. } = h.updates.recv_timeout(Duration::from_secs(5)).unwrap() else { panic!("expected a failure") };
        assert_eq!(lines[0].text, "这句没画上");
        assert!(message.contains("offline"));
        assert!(h.session.board().nodes.is_empty());
    }
}
