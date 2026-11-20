//! The board agent: turns finished speech into board operations through a local or remote model.
//!
//! Evidence for the provider defaults, from the eval sets:
//! the layout rules lowered Qwen 4B's holdout scores, so local runs leave them out;
//! they raised DeepSeek's node and edge precision on holdout with recall unchanged, so DeepSeek uses them.
//! Other remote models have not been measured, so they go without.
//! Hints from the checker did not help on either model, so they stay opt-in.

pub mod consolidate;
pub mod turns;
pub mod settings;
pub mod sound;
pub mod summary;
pub mod tools;
pub mod vision;

use anyhow::{anyhow, bail, Context, Result};
use boardcast_board::graph::{board_is_english, grammar, parse_ops, user_message_within, Graph, Kind, Utterance};
use boardcast_board::speakers::{attribute, rename_by};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
#[cfg(target_os = "linux")]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// How many earlier lines the model hears with each new one.
pub const CONTEXT_LINES: usize = 2;
/// How many earlier lines of the transcript the model reads with each whole turn ([`turns`]). On the AliMeeting
/// phone meeting as the app heard it, DeepSeek over nine passes: 0.61 item F1 with six lines, 0.52 with two;
/// a turn holds more than a line did, and the remarks around it say what it answers.
pub const TURN_CONTEXT_LINES: usize = 6;
const LLAMA_PORT: u16 = 18_765;
const ATTEMPTS: u32 = 4;
/// How long a local model may think before answering a long request, such as rewriting a whole board: on a
/// laptop whose graphics card is shared, the 4B wrote 12 tokens a second, five and a half minutes for one
/// rewrite, past the 300 s every other request is given.
const LOCAL_THINKING_LIMIT: Duration = Duration::from_secs(600);
const NO_MODEL: &str = "No board model is set up. Download one or add a remote model in Settings.";

/// The layout skill's rules, short enough for a prompt. The full guidance is skills/board-layout/SKILL.md.
pub const SKILL_RULES: &str = include_str!("../skills/board-layout/RULES.md");

/// The fixed examples and system prompt, first recorded from the TypeScript engine and now edited by hand.
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

    /// Whether a reply is held to a few operations, so a long line has to be said in pieces: a local model,
    /// whose grammar allows 8 operations in 160 tokens. A remote reply has room for the whole line.
    fn short_reply(&self) -> bool {
        match self {
            Self::Local { .. } => true,
            Self::Remote { .. } => false,
            #[cfg(test)]
            Self::Scripted(_) => true,
        }
    }

    fn is_deepseek(&self) -> bool {
        matches!(self, Self::Remote { base_url, .. } if base_url.contains("deepseek.com"))
    }

    /// How this model is asked, unless told otherwise. A remote model answers with tool calls, which make the
    /// agent easy to extend and scored the same as command lines on DeepSeek (edge F1 0.868 against 0.863
    /// over three passes); a local one keeps command lines held to a grammar, which the 4B answers better
    /// (item F1 0.89 against 0.81) and in less context than the tools' 4,899-token prompt.
    pub fn default_guidance(&self) -> Guidance {
        let harness = match self {
            Self::Remote { .. } => tools::Harness::Tools,
            _ => tools::Harness::Commands,
        };
        Guidance { rules: self.is_deepseek(), hint: false, harness }
    }

    /// An ordinary question with no grammar, as a summary needs.
    pub fn chat(&self, messages: &[Value], max_tokens: u32) -> Result<String> {
        let reply = self.chat_reply(messages, max_tokens)?;
        Ok(strip_thinking(message(&reply)?).to_string())
    }

    /// As [`Self::chat`], the whole reply: with its usage, and a reasoning model's thinking.
    pub fn chat_reply(&self, messages: &[Value], max_tokens: u32) -> Result<Value> {
        self.chat_reply_with(messages, max_tokens, "high")
    }

    /// As [`Self::chat_reply`], with how hard a reasoning model thinks: high, medium or low. Thinking less is
    /// how a long task is made cheaper; cutting its room short only cuts the answer off.
    pub fn chat_reply_with(&self, messages: &[Value], max_tokens: u32, effort: &str) -> Result<Value> {
        self.chat_reply_thinking(messages, max_tokens, effort, false)
    }

    /// As [`Self::chat_reply_with`], and a local model thinks first when `think_locally`: slower, but a small
    /// model rewriting a whole board does better for it. A remote model thinks as `effort` says either way.
    pub fn chat_reply_thinking(&self, messages: &[Value], max_tokens: u32, effort: &str, think_locally: bool) -> Result<Value> {
        let body = match self {
            Self::Remote { model, .. } => {
                let mut body = json!({ "model": model, "messages": messages, "temperature": 0.2, "max_tokens": max_tokens, "stream": false });
                if self.is_deepseek() {
                    body["thinking"] = json!({ "type": "enabled" });
                    body["reasoning_effort"] = json!(effort);
                }
                body
            }
            // Left to itself a small model can fall into repeating the same few lines until it runs out of room.
            Self::Local { .. } => {
                let body = json!({ "model": "local", "messages": messages, "temperature": 0.2, "repeat_penalty": 1.15, "repeat_last_n": 256, "max_tokens": max_tokens, "cache_prompt": true, "chat_template_kwargs": { "enable_thinking": think_locally } });
                if think_locally {
                    // Once, within ten minutes: the caller asks again without thinking if it fails.
                    return self.send_within(&self.endpoint(), &body, LOCAL_THINKING_LIMIT, 1);
                }
                body
            }
            #[cfg(test)]
            Self::Scripted(reply) => {
                let said = messages.last().and_then(|m| m["content"].as_str()).unwrap_or_default().to_string();
                return Ok(json!({ "choices": [{ "message": { "content": reply(&said)? } }] }));
            }
        };
        self.send(&self.endpoint(), &body)
    }

    /// For a local server: its context window, and how much of it `messages` take, from the server itself
    /// (`/props`, `/apply-template`, `/tokenize`). `None` for a remote model, or if the server cannot say.
    pub fn room_for(&self, messages: &[Value]) -> Option<(u64, u64)> {
        let Self::Local { base } = self else { return None };
        let get = |path: &str| -> Option<Value> { http(base).get(&format!("{base}{path}")).timeout(Duration::from_secs(10)).call().ok()?.into_json().ok() };
        let post = |path: &str, body: Value| -> Option<Value> { http(base).post(&format!("{base}{path}")).timeout(Duration::from_secs(30)).send_json(body).ok()?.into_json().ok() };
        let props = get("/props")?;
        let window = props["default_generation_settings"]["n_ctx"].as_u64().or_else(|| props["n_ctx"].as_u64())?;
        let prompt = post("/apply-template", json!({ "messages": messages }))?["prompt"].as_str()?.to_string();
        let used = post("/tokenize", json!({ "content": prompt }))?["tokens"].as_array()?.len() as u64;
        Some((window, used))
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

    /// A turn answered with tool calls rather than command lines: no grammar, room for the calls.
    fn call_tools(&self, messages: &[Value]) -> Result<Value> {
        let tools = tools::definitions();
        let body = match self {
            Self::Remote { model, .. } => {
                let mut body = json!({ "model": model, "messages": messages, "tools": tools, "tool_choice": "auto", "temperature": 0, "max_tokens": 4096, "stream": false });
                if self.is_deepseek() {
                    body["thinking"] = json!({ "type": "enabled" });
                    body["reasoning_effort"] = json!("high");
                }
                body
            }
            Self::Local { .. } => json!({ "model": "local", "messages": messages, "tools": tools, "tool_choice": "auto", "temperature": 0, "max_tokens": 1024, "cache_prompt": true, "chat_template_kwargs": { "enable_thinking": false } }),
            #[cfg(test)]
            Self::Scripted(reply) => {
                // A script answers with command lines; each becomes the call it stands for.
                let said = messages.last().and_then(|m| m["content"].as_str()).unwrap_or_default().to_string();
                let text = reply(&said)?;
                let calls: Vec<Value> = parse_ops(&text).iter().filter_map(tools::call).enumerate().map(|(i, (name, args))| json!({ "id": format!("s{i}"), "type": "function", "function": { "name": name, "arguments": args.to_string() } })).collect();
                return Ok(json!({ "choices": [{ "message": { "content": "", "tool_calls": calls } }] }));
            }
        };
        self.send(&self.endpoint(), &body)
    }

    /// One request, retried while the failure might pass. A reply that cannot be used says what came
    /// back, cut short and with the key taken out, so a failure can be told apart afterwards.
    fn send(&self, url: &str, body: &Value) -> Result<Value> {
        self.send_within(url, body, Duration::from_secs(300), ATTEMPTS)
    }

    /// As [`Self::send`], with its own time limit and number of attempts: a local model thinking over a whole
    /// board can take minutes, and trying such a request again after it ran out of time only loses more.
    fn send_within(&self, url: &str, body: &Value, limit: Duration, attempts: u32) -> Result<Value> {
        let mut last = anyhow!("no attempt made");
        for attempt in 0..attempts {
            let mut call = http(url).post(url).timeout(limit);
            if let Self::Remote { key, .. } = self {
                call = call.set("Authorization", &format!("Bearer {key}"));
            }
            let started = Instant::now();
            match call.send_json(body.clone()) {
                Ok(reply) => {
                    let kind = reply.content_type().to_string();
                    match reply.into_string() {
                        Ok(text) => {
                            return serde_json::from_str(&text).map_err(|e| {
                                anyhow!("the model's reply is not JSON ({kind}, {} bytes, {} ms): {e}; it began: {}", text.len(), started.elapsed().as_millis(), self.excerpt(&text))
                            });
                        }
                        // The connection broke mid-reply; another attempt may get all of it.
                        Err(e) => last = anyhow!("the model's reply broke off after {} ms: {e}", started.elapsed().as_millis()),
                    }
                }
                // A rejected request will not improve by retrying, and its body names the reason.
                Err(ureq::Error::Status(code, reply)) if code != 429 && code < 500 => {
                    return Err(anyhow!("model answered {code}: {}", self.excerpt(&reply.into_string().unwrap_or_default())));
                }
                Err(e) => last = anyhow!("model request failed: {e}"),
            }
            if attempt + 1 < attempts {
                thread::sleep(Duration::from_secs(2_u64.pow(attempt + 1)));
            }
        }
        Err(last)
    }
}

impl Provider {
    /// The start of a reply, enough to recognize it, without the key should a server echo it back.
    fn excerpt(&self, text: &str) -> String {
        let text = match self {
            Self::Remote { key, .. } if !key.is_empty() => text.replace(key.as_str(), "…"),
            _ => text.to_string(),
        };
        let short: String = text.chars().take(EXCERPT).collect();
        if short.len() < text.len() { format!("{short}…") } else { short }
    }
}

/// How much of an unusable reply an error quotes.
const EXCERPT: usize = 240;

/// How many of a reply's tokens went to thinking, as OpenAI-style services report it.
fn reasoning_tokens(reply: &Value) -> u64 {
    reply["usage"]["completion_tokens_details"]["reasoning_tokens"].as_u64().unwrap_or(0)
}

/// The reasoning a reply carries apart from its answer, as DeepSeek's `reasoning_content`.
fn reasoning(reply: &Value) -> Option<&str> {
    reply["choices"][0]["message"]["reasoning_content"].as_str().filter(|t| !t.trim().is_empty())
}

/// The words of a chat reply. One without a message is a failure, not an empty answer.
fn message(reply: &Value) -> Result<&str> {
    reply["choices"][0]["message"]["content"].as_str().ok_or_else(|| {
        let shown: String = reply.to_string().chars().take(EXCERPT).collect();
        anyhow!("the model's reply has no message: {shown}")
    })
}

/// What to add to the prompt beyond the fixed instructions.
#[derive(Clone, Copy, Default)]
pub struct Guidance {
    pub rules: bool,
    pub hint: bool,
    /// Command lines, as the app answers, or tool calls, which only the evaluation tries for now.
    pub harness: tools::Harness,
}

#[derive(Default)]
pub struct Reply {
    pub ops: String,
    pub ms: u128,
    pub tokens_in: u64,
    /// How many of `tokens_in` the service served from its prompt cache, which it bills for less. DeepSeek
    /// names them `prompt_cache_hit_tokens`, OpenAI-style services `prompt_tokens_details.cached_tokens`.
    pub tokens_cached: u64,
    pub tokens_out: u64,
    /// What llama-server reports: the prompt tokens it actually read, after its prefix cache, and how long
    /// reading and writing took. Remote services leave these out.
    pub prompt_read: Option<u64>,
    pub prompt_ms: Option<f64>,
    pub predicted_ms: Option<f64>,
    /// What a reasoning model thought before answering, as DeepSeek returns it in `reasoning_content`: why it
    /// drew what it drew, for improving the prompt. Empty when the model gives none.
    pub thinking: String,
    /// How many of `tokens_out` went to thinking, where the service says.
    pub reasoning_tokens: u64,
}

/// Sounds a listener makes to keep a speaker going, in Chinese: one character each.
const FILLERS_ZH: &str = "嗯恩啊呃额对是好行哦噢唉吧呢嘛诶嘿哈呀哇";
/// And in English, a word each.
const FILLERS_EN: [&str; 15] = ["yeah", "yes", "yep", "ok", "okay", "right", "uh", "um", "mm", "hmm", "mhm", "oh", "ah", "sure", "uh-huh"];

/// A line that only acknowledges or trails off, "嗯。", "对对对。", "啊。嗯后。", "Yeah.": what a far microphone
/// catches of a listener, or of a speaker cut off. It goes into the discussion the model reads with the next
/// line, but is not worth a turn of its own: in the AliMeeting phone meeting a turn on such a line drew a
/// guess at best. At most one other character may be left, the end of a word the recognizer cut off.
pub fn backchannel(text: &str) -> bool {
    let mut left = 0;
    for word in text.split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '\'')).filter(|w| !w.is_empty()) {
        if word.is_ascii() {
            if !FILLERS_EN.contains(&word.to_lowercase().as_str()) {
                return false;
            }
            continue;
        }
        left += word.chars().filter(|c| !FILLERS_ZH.contains(*c)).count();
    }
    left <= 1
}

/// Words that drop something: what a strike needs the line to say.
const DROP_CUES: [&str; 33] = [
    "不用", "不要了", "取消", "不做", "划掉", "删掉", "去掉", "放弃", "算了", "不考虑", "不搞", "砍掉", "否了", "不行", "不适合", "换成", "改成", "不采用", "不选",
    "drop", "cancel", "scrap", "not doing", "strike", "cross", "forget", "won't", "no longer", "instead", "rule out", "skip", "not going", "remove",
];
/// Words that stress something: what a circle, star, underline or highlight needs the line to say.
const STRESS_CUES: [&str; 33] = [
    "重点", "关键", "最重要", "重要", "一定要", "必须", "务必", "核心", "圈", "星", "强调", "注意", "首要", "优先", "下划线", "画线", "标黄", "高亮", "结论", "定了",
    "key", "important", "crucial", "must", "critical", "priority", "highlight", "circle", "star", "emphas", "underline", "main", "decided",
];

/// A small model's marks, kept only where the line asks for one. Left to itself the 4B strikes and circles
/// points nobody dropped or stressed: on the AliMeeting phone meeting it struck ten of 37 nodes, a third of
/// them points still standing. A strike stays when the line drops something ("不用了", "cancel"), any other
/// mark when it stresses something ("重点", "key"). Marks on an imported page stay, as there the line reads
/// the page aloud rather than saying why, and so does anything a line points at.
fn marks_said(ops: &str, said: &str, graph: &Graph) -> String {
    let said = said.to_lowercase();
    let drops = DROP_CUES.iter().any(|c| said.contains(c));
    let stresses = STRESS_CUES.iter().any(|c| said.contains(c));
    let on_page = |target: &str| {
        let name = target.trim_start_matches('「').trim_end_matches('」');
        graph.nodes.iter().any(|n| (n.id == target || n.label == name) && matches!(n.kind, Kind::Region | Kind::Clip))
    };
    ops.lines()
        .filter(|line| {
            let mut words = line.split_whitespace();
            if words.next() != Some("M") {
                return true;
            }
            let (Some(target), Some(mark)) = (words.next(), words.next()) else { return true };
            on_page(target) || if mark == "划掉" { drops } else { stresses }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One model turn for one line of speech.
pub fn propose(provider: &Provider, graph: &Graph, recent: &[Utterance], next: &Utterance, guidance: Guidance, hint: Option<&str>) -> Result<Reply> {
    propose_with(provider, graph, recent, next, guidance, hint, &sound::Glossary::default(), &[])
}

/// How long a line a local model hears in one turn, counting a Chinese or Japanese character as 1 and any
/// other character as 0.3. A local reply holds at most 8 operations, which a longer line runs past: the rest
/// of it never reaches the board. On the eval's long-line case, cutting the line raised Qwen 4B from node
/// 0.50 / edge 0.00 to 0.90 / 0.61, but lowered DeepSeek from 0.93 / 0.80 to 0.85 / 0.63 over three
/// passes each: a remote reply has room for the whole line and does better seeing all of it.
const PIECE_UNITS: f32 = 90.0;
/// A line of this many sentences or more is cut about a sentence at a time, however short it is.
const MANY_SENTENCES: usize = 3;
/// About one sentence: room for one of 20 Chinese characters, or two short ones together.
const SENTENCE_UNITS: f32 = 24.0;

fn units(text: &str) -> f32 {
    text.chars().filter(|c| !c.is_whitespace()).map(|c| if (c as u32) >= 0x2E80 { 1.0 } else { 0.3 }).sum()
}

/// A line cut into pieces the model can answer whole: at sentence ends, and at commas where one sentence
/// alone is too long, each piece as many sentences as fit. A line short enough stays as it is, unless it
/// runs several sentences together, as lines heard while the model was busy do: then each piece is about a
/// sentence ([`MANY_SENTENCES`]), which a small model answers step by step instead of folding the steps into
/// a few nodes.
pub fn pieces(text: &str) -> Vec<String> {
    let ends = ['。', '！', '？', '；', '!', '?', ';', '.'];
    // A full stop ends a sentence only before a space or the end, so "10.5" and "v0.29" are not counted.
    let chars: Vec<char> = text.trim_end().chars().collect();
    let is_end = |i: usize| ends.contains(&chars[i]) && (chars[i] != '.' || chars.get(i + 1).is_none_or(|n| n.is_whitespace()));
    let sentences_in = (0..chars.len()).filter(|&i| is_end(i)).count() + usize::from(chars.last().is_some_and(|_| !is_end(chars.len() - 1)));
    let limit = if sentences_in >= MANY_SENTENCES { SENTENCE_UNITS } else { PIECE_UNITS };
    if units(text) <= limit {
        return vec![text.to_string()];
    }
    // Ends of sentences, and of clauses as a fallback; a full stop ends one only before a space or the end,
    // so "10.5" and "v0.29" stay whole.
    let cut = |text: &str, marks: &[char]| -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let (mut out, mut start) = (vec![], 0);
        for (i, &c) in chars.iter().enumerate() {
            let full_stop = c == '.' && chars.get(i + 1).is_none_or(|n| n.is_whitespace());
            if marks.contains(&c) || (full_stop && marks.contains(&'.')) {
                out.push(chars[start..=i].iter().collect::<String>());
                start = i + 1;
            }
        }
        if start < chars.len() {
            out.push(chars[start..].iter().collect());
        }
        out.into_iter().filter(|s: &String| !s.trim().is_empty()).collect()
    };
    let mut sentences = vec![];
    // A sentence is cut at its commas only when it is too long for a piece of its own.
    for sentence in cut(text, &ends) {
        if units(&sentence) > PIECE_UNITS {
            sentences.extend(cut(&sentence, &['，', '、', ',']));
        } else {
            sentences.push(sentence);
        }
    }
    let mut out: Vec<String> = vec![];
    for sentence in sentences {
        match out.last_mut() {
            Some(last) if units(last) + units(&sentence) <= limit => last.push_str(&sentence),
            _ => out.push(sentence),
        }
    }
    out.into_iter().map(|p| p.trim().to_string()).collect()
}

/// As [`propose`], telling the model how the glossary writes names and which words in the line may be the
/// board's labels heard in other characters. A long line is said a few sentences at a time ([`pieces`]):
/// each piece sees the board as the pieces before it left it, and has them as its recent lines. The
/// replies come back as one, as if the line had been answered in one turn. Only a model with a short reply
/// is given pieces (see [`PIECE_UNITS`]), and a line about what was circled is not cut, since what it asks
/// for must happen once.
pub fn propose_with(provider: &Provider, graph: &Graph, recent: &[Utterance], next: &Utterance, guidance: Guidance, hint: Option<&str>, glossary: &sound::Glossary, references: &[(u32, String)]) -> Result<Reply> {
    if next.pointing.is_empty() && backchannel(&next.text) {
        return Ok(Reply::default());
    }
    let parts = if provider.short_reply() && next.pointing.is_empty() { pieces(&next.text) } else { vec![next.text.clone()] };
    if parts.len() == 1 {
        return propose_once(provider, graph, recent, next, guidance, hint, glossary, references);
    }
    let (mut board, mut heard) = (graph.clone(), recent.to_vec());
    let mut whole = Reply::default();
    for (i, text) in parts.into_iter().enumerate() {
        let piece = Utterance { speaker: next.speaker.clone(), text, pointing: vec![] };
        let context = &heard[heard.len().saturating_sub(CONTEXT_LINES)..];
        let reply = propose_once(provider, &board, context, &piece, guidance, if i == 0 { hint } else { None }, glossary, references)?;
        board.apply(&parse_ops(&reply.ops));
        if !reply.ops.trim().is_empty() {
            if !whole.ops.is_empty() {
                whole.ops.push('\n');
            }
            whole.ops.push_str(reply.ops.trim());
        }
        whole.ms += reply.ms;
        whole.tokens_in += reply.tokens_in;
        whole.tokens_cached += reply.tokens_cached;
        whole.tokens_out += reply.tokens_out;
        let add = |a: Option<u64>, b: Option<u64>| match (a, b) { (None, None) => None, (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)) };
        let addf = |a: Option<f64>, b: Option<f64>| match (a, b) { (None, None) => None, (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)) };
        whole.prompt_read = add(whole.prompt_read, reply.prompt_read);
        whole.prompt_ms = addf(whole.prompt_ms, reply.prompt_ms);
        whole.predicted_ms = addf(whole.predicted_ms, reply.predicted_ms);
        heard.push(piece);
    }
    Ok(whole)
}

/// A turn answered with tool calls. Each call is the command line it stands for, so the reply is the same
/// as the command harness's and the board applies it the same way. Checked, the model then sees the board its
/// calls drew and may correct it, until it calls nothing or the rounds run out.
fn propose_tools(provider: &Provider, graph: &Graph, prompt: Vec<Value>, harness: tools::Harness) -> Result<Reply> {
    let mut messages = tools::messages(&prompt[..prompt.len() - 1]);
    messages.push(prompt[prompt.len() - 1].clone());
    let started = Instant::now();
    let (mut board, mut lines) = (graph.clone(), vec![]);
    let mut whole = Reply::default();
    let rounds = if harness == tools::Harness::ToolsChecked { 1 + tools::CHECK_ROUNDS } else { 1 };
    for round in 0..rounds {
        let reply = provider.call_tools(&messages)?;
        whole.tokens_in += reply["usage"]["prompt_tokens"].as_u64().unwrap_or(0);
        whole.tokens_cached += reply["usage"]["prompt_cache_hit_tokens"].as_u64().or_else(|| reply["usage"]["prompt_tokens_details"]["cached_tokens"].as_u64()).unwrap_or(0);
        whole.tokens_out += reply["usage"]["completion_tokens"].as_u64().unwrap_or(0);
        whole.reasoning_tokens += reasoning_tokens(&reply);
        if let Some(thought) = reasoning(&reply) {
            whole.thinking.push_str(thought);
        }
        let calls = tools::calls(&reply);
        if calls.is_empty() {
            break;
        }
        for command in calls.iter().filter_map(|(_, c)| c.as_ref()) {
            board.apply(&parse_ops(command));
            lines.push(command.clone());
        }
        if round + 1 == rounds {
            break;
        }
        messages.push(tools::echo(&reply)?);
        messages.extend(tools::results(&calls, &board, round + 2 == rounds));
    }
    whole.ops = lines.join("\n");
    whole.ms = started.elapsed().as_millis();
    Ok(whole)
}

/// How much of a line's message, in characters, goes to the board and the lines before it, for a local
/// model: its instructions and examples take about 2,400 tokens of the server's 6,144, and the board of a long
/// meeting would otherwise take the rest and more, failing every line (#314). A character is a token or less
/// for Qwen; models that spend more still fit, and one that does not is asked again with half.
const LOCAL_PROMPT_CHARS: usize = 2000;
/// The same for a remote model, whose context is large: a long meeting's board would only cost more.
const REMOTE_PROMPT_CHARS: usize = 12_000;

/// One model turn for one piece of a line.
#[allow(clippy::too_many_arguments)]
fn propose_once(provider: &Provider, graph: &Graph, recent: &[Utterance], next: &Utterance, guidance: Guidance, hint: Option<&str>, glossary: &sound::Glossary, references: &[(u32, String)]) -> Result<Reply> {
    let english = board_is_english(recent, next);
    let all = prompts();
    let mut messages: Vec<Value> = all[if english { "en" } else { "zh" }].as_array().cloned().context("prompts")?;
    if guidance.rules {
        // Appended to the system prompt, which never changes, so prefix caching still applies.
        let text = messages[0]["content"].as_str().unwrap_or("").to_string();
        messages[0]["content"] = json!(format!("{text}\n\n{SKILL_RULES}"));
    }
    let labels: Vec<&str> = graph.nodes.iter().map(|n| n.label.as_str()).collect();
    let asked = |budget: usize| -> Vec<Value> {
        let mut content = user_message_within(graph, recent, next, budget);
        if let Some(h) = hint.filter(|_| guidance.hint) {
            content = format!("{h}\n{content}");
        }
        if let Some(note) = sound::note_with(glossary, references, &next.text, &labels, english) {
            content = format!("{note}\n{content}");
        }
        let mut asked = messages.clone();
        asked.push(json!({ "role": "user", "content": content }));
        asked
    };
    let budget = if provider.short_reply() { LOCAL_PROMPT_CHARS } else { REMOTE_PROMPT_CHARS };
    if guidance.harness != tools::Harness::Commands {
        return propose_tools(provider, graph, asked(budget), guidance.harness);
    }
    let started = Instant::now();
    // Still too long for the server, as with a model that spends more tokens on a character: once more with
    // half as much of the board.
    let reply = match provider.call(asked(budget), graph, english) {
        Err(e) if format!("{e:#}").contains("context") => provider.call(asked(budget / 2), graph, english)?,
        other => other?,
    };
    let ops = strip_thinking(message(&reply)?);
    Ok(Reply {
        ops: if provider.short_reply() { marks_said(ops, &next.text, graph) } else { ops.to_string() },
        ms: started.elapsed().as_millis(),
        tokens_in: reply["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        tokens_cached: reply["usage"]["prompt_cache_hit_tokens"].as_u64().or_else(|| reply["usage"]["prompt_tokens_details"]["cached_tokens"].as_u64()).unwrap_or(0),
        tokens_out: reply["usage"]["completion_tokens"].as_u64().unwrap_or(0),
        prompt_read: reply["timings"]["prompt_n"].as_u64(),
        prompt_ms: reply["timings"]["prompt_ms"].as_f64(),
        predicted_ms: reply["timings"]["predicted_ms"].as_f64(),
        thinking: reasoning(&reply).unwrap_or_default().to_string(),
        reasoning_tokens: reasoning_tokens(&reply),
    })
}

pub struct Said {
    pub line: Utterance,
    /// When speech ended or the line was submitted; end-to-end timings start here.
    pub ended: Instant,
    pub asr_ms: Option<u128>,
    /// The meeting this was said in, from `Session::meeting`. A line from another meeting is dropped.
    pub meeting: u64,
    /// From `Session::renames_seen`: speakers renamed after this count are renamed in this line too.
    pub renames: usize,
    /// Where in the input it was heard, in seconds from its start to its end; none for a typed line.
    pub span: Option<(f64, f64)>,
}

impl turns::Piece for Said {
    fn utterance(&self) -> &Utterance {
        &self.line
    }
    fn span(&self) -> Option<(f64, f64)> {
        self.span
    }
    fn urgent(&self) -> bool {
        self.asr_ms.is_none() || !self.line.pointing.is_empty()
    }
}

/// How long a turn is held after the last sound of speech, a line or a draft, before it is drawn: drafts come
/// about three times a second while anyone talks, so a second without one is a pause.
const QUIET: Duration = Duration::from_millis(1000);
/// The same without drafts, when only finished lines say someone is talking: a speaker's next piece can take
/// the recognizer a few seconds to finish.
const QUIET_WITHOUT_DRAFTS: Duration = Duration::from_millis(2500);
/// A turn is drawn after this long however it goes on, so the board never falls far behind a monologue.
const MAX_HOLD: Duration = Duration::from_secs(15);

pub enum Update {
    Loading,
    /// The model is up and working through the fixed part of the prompt, this many tokens of that many.
    Preparing { done: u64, total: u64 },
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
    /// Where the graphics driver keeps the shaders it compiles, so that happens once rather than per launch.
    pub shaders: Option<PathBuf>,
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

/// The server's file name in the folder it ships in.
pub const LLAMA_SERVER: &str = if cfg!(windows) { "llama-server.exe" } else { "llama-server" };

/// `llama-server` from `binaries`, with its libraries found and, on Windows, no console.
pub fn server_command(binaries: &std::path::Path) -> Command {
    let mut cmd = Command::new(binaries.join(LLAMA_SERVER));
    #[cfg(target_os = "macos")]
    cmd.env("DYLD_LIBRARY_PATH", binaries);
    #[cfg(not(any(target_os = "macos", windows)))]
    cmd.env("LD_LIBRARY_PATH", binaries);
    // Windows looks for libraries beside the program already; this keeps it from opening a console.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    cmd.stdin(Stdio::null());
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

fn is_discrete(gpus: &[Gpu], device: Option<&str>) -> bool {
    gpus.iter().any(|g| g.discrete && Some(g.id.as_str()) == device)
}

/// The same launch arguments as the older app.
fn llama_command(server: &LocalServer, device: Option<&str>, discrete: bool, port: u16, projector: Option<&std::path::Path>) -> Command {
    let mut cmd = server_command(&server.binaries);
    if let Some(dir) = &server.shaders {
        let _ = std::fs::create_dir_all(dir);
        // NVIDIA and Mesa each have their own setting. Without the second, NVIDIA trims its cache by age.
        cmd.env("__GL_SHADER_DISK_CACHE_PATH", dir).env("__GL_SHADER_DISK_CACHE_SKIP_CLEANUP", "1").env("MESA_SHADER_CACHE_DIR", dir);
    }
    if discrete {
        // Without this the CPU copies the model into graphics memory across the bus itself, which took 80 s
        // on an RTX 3060 Laptop. This way the card fetches it, in under 2 s.
        cmd.env("GGML_VK_DISABLE_HOST_VISIBLE_VIDMEM", "1");
    }
    if let Some(projector) = projector {
        cmd.arg("--mmproj").arg(projector);
    }
    cmd.arg("-m")
        .arg(&server.model)
        .args(["--host", "127.0.0.1", "--port", &port.to_string()])
        // Instructions about 2,400 tokens, the board and earlier lines at most LOCAL_PROMPT_CHARS, the reply 160.
        .args(["-c", "6144", "-np", "1", "--jinja"])
        // Prompts kept aside in memory, to pick up again without reading them anew: 8 GiB unless told, and a
        // Qwen3.5 prompt holds about 330 MiB, so an eval server with varied prompts grew to 10 GB and ran this
        // computer out of memory (2026-11-19). The lines' own prompt stays in its slot either way.
        .args(["--cache-ram", "1024"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match device {
        // Not a fixed layer count: if the model does not fit in VRAM, as many layers as fit go there.
        Some(device) => cmd.args(["--device", device, "-ngl", "auto", "--fit", "on", "-fa", "auto"]),
        None => cmd.args(["--device", "none", "-ngl", "0"]),
    };
    cmd
}

fn spawn_llama(server: &LocalServer, device: Option<&str>, discrete: bool, port: u16, projector: Option<&std::path::Path>) -> Result<Child> {
    spawn_tied(llama_command(server, device, discrete, port, projector)).with_context(|| format!("starting {}", server.binaries.join(LLAMA_SERVER).display()))
}

/// Start a child that dies with the thread that started it on Linux, and with this process elsewhere, so
/// no model server outlives the app however it ends.
pub fn spawn_tied(mut cmd: Command) -> Result<Child> {
    #[cfg(target_os = "linux")]
    // SAFETY: prctl is async-signal-safe and touches no parent state between fork and exec.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    #[cfg(target_os = "macos")]
    end_with_this_process(child.id());
    #[cfg(windows)]
    end_with_this_process(&child);
    Ok(child)
}

/// On Windows the child joins a job that the system ends, with everything in it, when this process goes.
#[cfg(windows)]
fn end_with_this_process(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;
    use windows_sys::Win32::System::JobObjects::*;
    static JOB: OnceLock<usize> = OnceLock::new();
    let job = *JOB.get_or_init(|| {
        // SAFETY: the job's handle is kept for the life of the process, which is what ends the job; the
        // limits are a plain struct written before it is passed.
        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
            SetInformationJobObject(job, JobObjectExtendedLimitInformation, (&raw const limits).cast(), size);
            job as usize
        }
    });
    // SAFETY: both handles are live: the job's for good, the child's while `child` is borrowed.
    if unsafe { AssignProcessToJobObject(job as _, child.as_raw_handle() as _) } == 0 {
        eprintln!("could not tie the model to this process: {}", std::io::Error::last_os_error());
    }
}

/// A Mac can't tie a child's life to its parent, so a small shell waits for this process to go,
/// however it goes, and then ends the child. It goes away by itself if the child ends first.
#[cfg(target_os = "macos")]
fn end_with_this_process(child: u32) {
    let script = format!("while kill -0 {parent} 2>/dev/null && kill -0 {child} 2>/dev/null; do sleep 1; done; kill {child} 2>/dev/null", parent = std::process::id());
    if let Err(e) = Command::new("/bin/sh").args(["-c", &script]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        eprintln!("could not tie the model to this process: {e}");
    }
}

/// Start the server on the best device, falling back to the CPU if that fails to come up.
fn start_llama(server: &LocalServer) -> Result<(Child, String)> {
    let base = format!("http://127.0.0.1:{LLAMA_PORT}");
    let health = format!("{base}/health");
    let gpus = list_gpus(&server.binaries);
    let device = pick_gpu(&gpus);
    let mut child = spawn_llama(server, device.as_deref(), is_discrete(&gpus, device.as_deref()), LLAMA_PORT, None)?;
    match wait_until_up(&health, &mut child) {
        Ok(()) => Ok((child, base)),
        Err(first) if device.is_some() => {
            eprintln!("local model did not start on {device:?}: {first:#}; trying the CPU");
            let _ = child.kill();
            let _ = child.wait();
            let mut child = spawn_llama(server, None, false, LLAMA_PORT, None)?;
            wait_until_up(&health, &mut child)?;
            Ok((child, base))
        }
        Err(first) => Err(first),
    }
}

/// Read the prompt's progress from one line of a streamed reply.
fn prompt_progress(line: &str) -> Option<(u64, u64)> {
    let value: Value = serde_json::from_str(line.strip_prefix("data: ")?).ok()?;
    let progress = value.get("prompt_progress")?;
    Some((progress["processed"].as_u64()?, progress["total"].as_u64()?))
}

/// Have the server work through the fixed part of the prompt now, and keep it, so the first line said costs
/// no more than the ones after it. This is also what makes the driver compile its shaders if it has to.
/// Chinese, as the two languages share about half of it and English then only costs a moment more. It
/// is only a head start, so a failure here is left for the first real line to report.
fn prepare_prompt(base: &str, updates: &Sender<Update>) {
    let Some(mut messages) = prompts()["zh"].as_array().cloned() else { return };
    messages.push(json!({ "role": "user", "content": "你好" }));
    let body = json!({ "model": "local", "messages": messages, "temperature": 0, "max_tokens": 1, "cache_prompt": true, "stream": true, "return_progress": true, "chat_template_kwargs": { "enable_thinking": false } });
    let response = match http(&chat_url(base)).post(&chat_url(base)).send_json(&body) {
        Ok(response) => response,
        Err(e) => {
            eprintln!("could not prepare the prompt: {e}");
            return;
        }
    };
    for line in BufReader::new(response.into_reader()).lines().map_while(std::result::Result::ok) {
        if let Some((done, total)) = prompt_progress(&line) {
            let _ = updates.send(Update::Preparing { done, total });
        }
    }
}

/// Wait for the server to answer, giving up early if it has already exited.
fn wait_until_up(url: &str, child: &mut Child) -> Result<()> {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(180) {
        if let Some(status) = child.try_wait()? {
            bail!("llama-server exited during startup ({status})");
        }
        if http(url).get(url).timeout(Duration::from_secs(2)).call().is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
    Err(anyhow!("timed out waiting for {url}"))
}

/// Requests honour the proxy settings in the environment, as the older app does.
/// One client for every request, so connections are kept and reused rather than opened each time.
/// A client for a URL: straight to this computer's own servers, and elsewhere through the proxy chosen in
/// Settings. Each keeps its connections, so a remote model is not shaken hands with again for every line.
fn http(url: &str) -> ureq::Agent {
    use boardcast_models::net;
    static LOCAL: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    static REMOTE: Mutex<Option<(net::Proxy, ureq::Agent)>> = Mutex::new(None);
    if net::is_local(url) {
        return LOCAL.get_or_init(|| ureq::AgentBuilder::new().try_proxy_from_env(false).build()).clone();
    }
    let chosen = net::chosen();
    let mut remote = REMOTE.lock().expect("client lock");
    match remote.as_ref() {
        Some((proxy, agent)) if *proxy == chosen => agent.clone(),
        _ => {
            let agent = net::agent_for(url, false, Duration::from_secs(30)).build();
            *remote = Some((chosen, agent.clone()));
            agent
        }
    }
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

/// How many lines a remote model hears between rewrites of the whole board, as agent-eval measures with
/// `--consolidate 8`.
const CONSOLIDATE_EVERY: usize = 8;

/// Rewrite a board that has piled up into an organised outline, beside the line-by-line turns: the model
/// thinks for a minute or two, and the lines heard meanwhile are drawn as usual and kept
/// ([`consolidate::catch_up`]). The result arrives as an ordinary drawing, so it can be undone. A board that
/// does not need it, a failed reply, or another meeting opened meanwhile leaves the board alone.
fn consolidate_live(provider: &Provider, graph: &Mutex<Graph>, history: &Mutex<Vec<Utterance>>, meeting: &AtomicU64, current: u64, updates: &Sender<Update>, when: consolidate::When) {
    let asked = graph.lock().expect("graph lock").clone();
    let heard = history.lock().expect("history lock").clone();
    let Some(last) = heard.last() else { return };
    let english = board_is_english(&heard[heard.len().saturating_sub(CONTEXT_LINES + 1)..heard.len() - 1], last);
    let done = match consolidate::consolidate(provider, &asked, &heard, english, None, "high", when) {
        Ok(Some(done)) => done,
        Ok(None) => return,
        Err(e) => {
            eprintln!("consolidating the board failed: {e:#}");
            return;
        }
    };
    let mut board = graph.lock().expect("graph lock");
    if meeting.load(Ordering::SeqCst) != current {
        return;
    }
    *board = consolidate::catch_up(&asked, &board, &done.graph);
    let _ = updates.send(Update::Drawn { meeting: current, graph: board.clone(), ops: String::new(), lines: vec![], asr_ms: None, llm_ms: done.ms, ended: Instant::now() });
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
    glossary: Arc<Mutex<sound::Glossary>>,
    /// The meeting's numbered references, from an imported paper.
    references: Arc<Mutex<Vec<(u32, String)>>>,
    backend: Arc<Mutex<Backend>>,
    guidance: Arc<Mutex<Option<Guidance>>>,
    meeting: Arc<AtomicU64>,
    /// Where the local model is answering, once it has started.
    local_base: Arc<Mutex<Option<String>>>,
    /// Speakers renamed in this meeting, in order, for lines that were on their way at the time.
    renames: Arc<Mutex<Vec<(String, String)>>>,
    /// When speech was last heard, from the drafts, so a turn is held while its speaker goes on.
    speaking: Arc<Mutex<Option<Instant>>>,
    /// Whether the board is being consolidated now: one at a time.
    consolidating: Arc<std::sync::atomic::AtomicBool>,
    /// Where a last consolidation sends its board, as the worker does.
    updates: Sender<Update>,
}

fn rename_line(line: &mut Utterance, renames: &[(String, String)]) {
    for (from, to) in renames {
        if line.speaker.as_deref() == Some(from) {
            line.speaker = Some(to.clone());
        }
    }
}

impl Session {
    pub fn start(backend: Backend, guidance: Option<Guidance>, said: Receiver<Said>, ready: Sender<()>, drained: Sender<()>, updates: Sender<Update>) -> Self {
        let session = Self {
            graph: Arc::new(Mutex::new(Graph::default())),
            history: Arc::new(Mutex::new(vec![])),
            hint: Arc::new(Mutex::new(None)),
            glossary: Arc::new(Mutex::new(sound::Glossary::default())),
            references: Arc::new(Mutex::new(vec![])),
            backend: Arc::new(Mutex::new(backend)),
            guidance: Arc::new(Mutex::new(guidance)),
            meeting: Arc::new(AtomicU64::new(0)),
            local_base: Arc::new(Mutex::new(None)),
            renames: Arc::new(Mutex::new(vec![])),
            speaking: Arc::new(Mutex::new(None)),
            consolidating: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            updates: updates.clone(),
        };
        let (graph, history, hint, backend, guidance, meeting) =
            (session.graph.clone(), session.history.clone(), session.hint.clone(), session.backend.clone(), session.guidance.clone(), session.meeting.clone());
        let glossary = session.glossary.clone();
        let references = session.references.clone();
        let local_base = session.local_base.clone();
        let renames = session.renames.clone();
        let speaking = session.speaking.clone();
        let consolidating = session.consolidating.clone();
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
                            prepare_prompt(&base, updates);
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
            // Lines heard in this meeting since the board was last consolidated, and whether a
            // consolidation is under way.
            let (mut since, mut since_meeting) = (0_usize, u64::MAX);
            // Lines are put back together into whole turns before the board hears them (turns.rs); the
            // transcript keeps them as they were heard.
            let mut assembler: turns::Assembler<Said> = turns::Assembler::default();
            let (mut last_line, mut held_since) = (Instant::now(), None::<Instant>);
            loop {
                let mut ready: Vec<turns::Ready<Said>> = vec![];
                let heard = *speaking.lock().expect("speaking lock");
                let quiet = if heard.is_some() { QUIET } else { QUIET_WITHOUT_DRAFTS };
                let last_sound = heard.map_or(last_line, |h| h.max(last_line));
                let held_too_long = held_since.is_some_and(|t| t.elapsed() >= MAX_HOLD);
                let got = if !assembler.holding() {
                    said.recv().map_err(|_| std::sync::mpsc::RecvTimeoutError::Disconnected)
                } else if held_too_long {
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                } else {
                    said.recv_timeout((last_sound + quiet).saturating_duration_since(Instant::now()))
                };
                match got {
                    Ok(first) => {
                        last_line = Instant::now();
                        // Read after the wait: a meeting opened while it waited is the one lines belong to now.
                        let current = meeting.load(Ordering::SeqCst);
                        for piece in std::iter::once(first).chain(std::iter::from_fn(|| said.try_recv().ok())) {
                            if piece.meeting == current {
                                ready.extend(assembler.push(piece));
                            }
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // Speech heard again since: its speaker is still going.
                        let again = speaking.lock().expect("speaking lock").is_some_and(|h| h > last_sound);
                        if again && !held_too_long {
                            continue;
                        }
                        ready.extend(assembler.flush());
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
                held_since = if assembler.holding() { held_since.or(Some(Instant::now())) } else { None };
                // What was held for a meeting since closed goes with it.
                ready.retain(|r| r.pieces.iter().all(|p| p.meeting == meeting.load(Ordering::SeqCst)));
                if ready.is_empty() {
                    if !assembler.holding() {
                        let _ = drained.send(());
                    }
                    continue;
                }
                let current = meeting.load(Ordering::SeqCst);
                let mut batch: Vec<Said> = vec![];
                let mut said_turns: Vec<Utterance> = vec![];
                for r in ready {
                    said_turns.extend(r.turn);
                    batch.extend(r.pieces);
                }
                let renames_seen = {
                    let renames = renames.lock().expect("renames lock");
                    for said in &mut batch {
                        rename_line(&mut said.line, renames.get(said.renames..).unwrap_or_default());
                    }
                    // A turn is renamed from its earliest piece's count on; a rename it already had changes nothing.
                    let earliest = batch.iter().map(|s| s.renames).min().unwrap_or(0);
                    for turn in &mut said_turns {
                        rename_line(turn, renames.get(earliest..).unwrap_or_default());
                    }
                    renames.len()
                };
                let next = match said_turns.len() {
                    0 => None,
                    1 => Some(said_turns[0].clone()),
                    _ => {
                        let text: Vec<String> = said_turns.iter().map(|t| t.speaker.as_ref().map_or_else(|| t.text.clone(), |n| format!("{n}：{}", t.text))).collect();
                        // Everything circled across the merged turns still counts, in the order it was circled.
                        let mut pointing: Vec<String> = vec![];
                        for id in said_turns.iter().flat_map(|t| &t.pointing) {
                            if !pointing.contains(id) {
                                pointing.push(id.clone());
                            }
                        }
                        Some(Utterance { speaker: None, text: text.join(" "), pointing })
                    }
                };
                let recent: Vec<Utterance> = {
                    let all = history.lock().expect("history lock");
                    all[all.len().saturating_sub(TURN_CONTEXT_LINES)..].to_vec()
                };
                let seen = graph.lock().expect("graph lock").clone();
                let note = hint.lock().expect("hint lock").take();
                let provider = provider_now(&updates);
                let result = match (&provider, &next) {
                    // Nothing to draw: the lines only join the discussion.
                    (_, None) => Ok(Reply::default()),
                    (Ok(provider), Some(next)) => {
                        let chosen = guidance.lock().expect("guidance lock").unwrap_or_else(|| provider.default_guidance());
                        let terms = glossary.lock().expect("glossary lock").clone();
                        let cited = references.lock().expect("references lock").clone();
                        propose_with(provider, &seen, &recent, next, chosen, note.as_deref(), &terms, &cited)
                    }
                    (Err(e), Some(_)) => Err(anyhow!("{e:#}")),
                };
                // The lock is held across the check, so a meeting cannot open between it and the write.
                let mut board = graph.lock().expect("graph lock");
                if meeting.load(Ordering::SeqCst) != current {
                    drop(board);
                    let _ = drained.send(());
                    continue;
                }
                // A rename also holds the board's lock, so none can come between this and the write.
                let mut lines: Vec<Utterance> = batch.iter().map(|s| s.line.clone()).collect();
                for line in lines.iter_mut().chain(said_turns.iter_mut()) {
                    rename_line(line, renames.lock().expect("renames lock").get(renames_seen..).unwrap_or_default());
                }
                history.lock().expect("history lock").extend(lines.iter().cloned());
                let tail = &batch[batch.len() - 1];
                let update = match result {
                    Ok(reply) => {
                        let before = board.clone();
                        board.apply(&parse_ops(&reply.ops));
                        // New nodes belong to who said the turn, not to whoever said "对" in the middle of it.
                        attribute(&before, &mut board, if said_turns.is_empty() { &lines } else { &said_turns });
                        Update::Drawn { meeting: current, graph: board.clone(), ops: reply.ops, lines, asr_ms: tail.asr_ms, llm_ms: reply.ms, ended: batch[0].ended }
                    }
                    Err(e) => Update::Failed { meeting: current, message: format!("{e:#}"), lines },
                };
                drop(board);
                let _ = updates.send(update);
                if since_meeting != current {
                    (since, since_meeting) = (0, current);
                }
                since += batch.len();
                if let Ok(Provider::Remote { .. }) = &provider {
                    if since >= CONSOLIDATE_EVERY && !consolidating.swap(true, Ordering::SeqCst) {
                        since = 0;
                        let provider = provider.as_ref().expect("checked above").clone();
                        let (graph, history, meeting, updates, consolidating) = (graph.clone(), history.clone(), meeting.clone(), updates.clone(), consolidating.clone());
                        thread::spawn(move || {
                            consolidate_live(&provider, &graph, &history, &meeting, current, &updates, consolidate::When::Piled);
                            consolidating.store(false, Ordering::SeqCst);
                        });
                    }
                }
                let _ = drained.send(());
            }
        });
        session
    }

    /// Listening stopped: once the last turn has been drawn, consolidate the board a last time, as one that
    /// piled up since the last rewrite would otherwise stay as drawn line by line. In the AliMeeting phone
    /// meeting the second half (promotion channels) was never rewritten, so its dropped channel was never
    /// struck. Remote models only, as during the meeting; nothing happens if a rewrite is running.
    pub fn wrap_up(&self) {
        let Backend::Ready(provider @ Provider::Remote { .. }) = self.backend.lock().expect("backend lock").clone() else { return };
        if self.consolidating.swap(true, Ordering::SeqCst) {
            return;
        }
        let (graph, history, meeting, consolidating) = (self.graph.clone(), self.history.clone(), self.meeting.clone(), self.consolidating.clone());
        let updates = self.updates.clone();
        let current = self.meeting();
        thread::spawn(move || {
            // The last turn is held until its speaker has been quiet, then drawn: it should be on the board.
            thread::sleep(QUIET_WITHOUT_DRAFTS + Duration::from_millis(500));
            consolidate_live(&provider, &graph, &history, &meeting, current, &updates, consolidate::When::Final);
            consolidating.store(false, Ordering::SeqCst);
        });
    }

    /// Someone is talking now, as a draft of what they say shows: a turn being held waits for them.
    pub fn heard_speech(&self) {
        *self.speaking.lock().expect("speaking lock") = Some(Instant::now());
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
        self.renames.lock().expect("renames lock").clear();
    }

    /// Give a speaker a new name, or join them to the speaker who has it, including in anything in flight.
    pub fn rename_speaker(&self, from: &str, to: &str) {
        let mut board = self.graph.lock().expect("graph lock");
        rename_by(&mut board, from, to);
        for line in self.history.lock().expect("history lock").iter_mut() {
            if line.speaker.as_deref() == Some(from) {
                line.speaker = Some(to.to_string());
            }
        }
        self.renames.lock().expect("renames lock").push((from.to_string(), to.to_string()));
    }

    /// To go in a line being sent now.
    pub fn renames_seen(&self) -> usize {
        self.renames.lock().expect("renames lock").len()
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

    /// A change made by hand, such as rewriting or removing something, applies at once.
    pub fn edit(&self, ops: &[boardcast_board::graph::Op]) -> Graph {
        let mut graph = self.graph.lock().expect("graph lock");
        graph.apply(ops);
        graph.clone()
    }

    /// Put a clip cut from the imported page on the board, and give back the board and the clip's id.
    pub fn add_clip(&self, label: &str, text: &str, clip: &boardcast_board::graph::Clip) -> (Graph, String) {
        let mut graph = self.graph.lock().expect("graph lock");
        let id = graph.add_clip(label, text, clip);
        (graph.clone(), id)
    }

    /// Take away hand annotations, by their places in the board's list.
    pub fn erase(&self, which: &[usize]) -> Graph {
        let mut graph = self.graph.lock().expect("graph lock");
        graph.erase(which);
        graph.clone()
    }

    /// Draw by hand over a page or a clip.
    pub fn annotate(&self, annotation: boardcast_board::graph::Annotation) -> Graph {
        let mut graph = self.graph.lock().expect("graph lock");
        graph.annotate(annotation);
        graph.clone()
    }

    /// Put a card from the inbox on the board where it was dropped, and give back the board and its id.
    pub fn add_card(&self, label: &str, text: &str, clip: &boardcast_board::graph::Clip, marks: &[String], drawn: &[boardcast_board::graph::Annotation], at: [f64; 2]) -> (Graph, String) {
        let mut graph = self.graph.lock().expect("graph lock");
        let id = graph.add_card(label, text, clip, marks, drawn, at);
        (graph.clone(), id)
    }

    /// Put an imported document's card on the board, and give back the board.
    pub fn add_document_card(&self, name: &str, first_page: &boardcast_board::graph::Clip, doc: &str) -> Graph {
        let mut graph = self.graph.lock().expect("graph lock");
        graph.add_document_card(name, first_page, doc);
        graph.clone()
    }

    /// Leave a clip card where it was dragged to.
    pub fn place(&self, id: &str, x: f64, y: f64) -> Graph {
        let mut graph = self.graph.lock().expect("graph lock");
        graph.place(id, x, y);
        graph.clone()
    }

    /// Mark a word or sentence of imported text by hand, or take the mark off.
    pub fn toggle_span(&self, id: &str, span: boardcast_board::spans::Span) -> Graph {
        let mut graph = self.graph.lock().expect("graph lock");
        graph.toggle_span(id, span);
        graph.clone()
    }

    /// The references a cited number is looked up in, for every line from now on.
    pub fn set_references(&self, references: Vec<(u32, String)>) {
        *self.references.lock().expect("references lock") = references;
    }

    /// How names are written, for every line from now on.
    pub fn set_glossary(&self, glossary: sound::Glossary) {
        *self.glossary.lock().expect("glossary lock") = glossary;
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
    fn a_long_meetings_board_stays_within_a_local_models_context() {
        let mut ops = String::new();
        for t in 0..70 {
            ops += &format!("T 「议题{t}」\n");
            for i in 0..8 {
                ops += &format!("L 「议题{t}」 「议题{t}的第{i}条要点，说得比较长」\n");
            }
        }
        ops += "T 「无人机巡检」\nL 「无人机巡检」 「夜间用红外摄像头」\n";
        let mut board = Graph::default();
        board.apply(&parse_ops(&ops));
        assert!(board.describe().chars().count() > 10_000);
        let asked = Arc::new(Mutex::new(String::new()));
        let seen = asked.clone();
        let provider = Provider::Scripted(Arc::new(move |said: &str| {
            *seen.lock().unwrap() = said.to_string();
            Ok("-".into())
        }));
        let next = Utterance { speaker: Some("A".into()), text: "夜间巡检的红外摄像头再看看".into(), pointing: vec![] };
        let guidance = provider.default_guidance();
        propose(&provider, &board, &[], &next, guidance, None).unwrap();
        let said = asked.lock().unwrap().clone();
        assert!(said.chars().count() <= LOCAL_PROMPT_CHARS + 200, "{} characters", said.chars().count());
        assert!(said.contains("夜间用红外摄像头"), "{said}");
    }

    #[test]
    fn a_small_model_marks_only_what_the_line_asks_for() {
        let mut board = Graph::default();
        board.apply(&parse_ops("T 「方案」\nL 「方案」 「自建」\nL 「方案」 「SaaS」"));
        let ops = "L n1 「三个月」\nM n2 划掉\nM n3 圈出";
        assert_eq!(marks_said(ops, "自建的坏处是至少要三个月。", &board), "L n1 「三个月」");
        assert_eq!(marks_said(ops, "那自建先不做了，时间等不起。", &board), "L n1 「三个月」\nM n2 划掉");
        assert_eq!(marks_said(ops, "成本是最关键的，一定要标出来。", &board), "L n1 「三个月」\nM n3 圈出");
        assert_eq!(marks_said("M n2 划掉", "We're not doing self-hosting.", &board), "M n2 划掉");
    }

    #[test]
    fn a_line_that_only_acknowledges_gets_no_turn() {
        for line in ["嗯。", "对对对。", "是吧。", "Yeah.", "OK, right.", "啊。嗯后。"] {
            assert!(super::backchannel(line), "{line}");
        }
        for line in ["人群。", "No.", "对，就这么定。", "Yeah, ship it.", "什么手机壳。", "好的，周五前交"] {
            assert!(!super::backchannel(line), "{line}");
        }
    }

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
        assert_eq!(remote("https://example.com/v1").default_guidance().harness, tools::Harness::Tools, "a remote model answers with tools");
        assert_eq!(Provider::Local { base: String::new() }.default_guidance().harness, tools::Harness::Commands, "a local one with command lines");
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
    fn a_discrete_card_fetches_the_model_itself() {
        let server = LocalServer { binaries: "bin".into(), model: "m.gguf".into(), shaders: Some(std::env::temp_dir().join("boardcast-agent-shaders")) };
        assert!(llama_command(&server, None, false, 1, None).get_envs().any(|(k, v)| k == "__GL_SHADER_DISK_CACHE_PATH" && v.is_some()));
        let asks = |discrete| llama_command(&server, Some("Vulkan1"), discrete, 1, None).get_envs().any(|(k, _)| k == "GGML_VK_DISABLE_HOST_VISIBLE_VIDMEM");
        assert!(asks(true));
        assert!(!asks(false));
        let gpus = [Gpu { id: "Vulkan0".into(), name: "Intel".into(), total_mib: 8000, discrete: false }, Gpu { id: "Vulkan1".into(), name: "NVIDIA".into(), total_mib: 6000, discrete: true }];
        assert!(is_discrete(&gpus, Some("Vulkan1")));
        assert!(!is_discrete(&gpus, Some("Vulkan0")));
        assert!(!is_discrete(&gpus, None));
    }

    #[test]
    fn the_local_server_keeps_little_aside_in_memory() {
        let server = LocalServer { binaries: "bin".into(), model: "m.gguf".into(), shaders: None };
        let args: Vec<String> = llama_command(&server, None, false, 1, None).get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        let at = args.iter().position(|a| a == "--cache-ram").expect("the prompt cache is capped");
        assert!(args[at + 1].parse::<u32>().unwrap() <= 1024, "{args:?}");
    }

    #[test]
    fn the_prompt_reports_how_far_it_has_got() {
        assert_eq!(prompt_progress(r#"data: {"choices":[],"prompt_progress":{"total":2151,"cache":0,"processed":1118,"time_ms":1}}"#), Some((1118, 2151)));
        assert_eq!(prompt_progress(r#"data: {"choices":[{"delta":{"content":"N"}}]}"#), None);
        assert_eq!(prompt_progress("data: [DONE]"), None);
        assert_eq!(prompt_progress(""), None);
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

/// A server that answers every request with this status, type and body.
    fn serve(status: &'static str, kind: &'static str, body: &'static str) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut request = [0_u8; 65536];
                let _ = stream.read(&mut request);
                let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        base
    }

    /// A server that answers every request with `body` and keeps each request's body, read whole.
    fn serve_recording(body: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let asked = Arc::new(Mutex::new(vec![]));
        let keep = asked.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(n) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = n.trim().parse().unwrap_or(0);
                    }
                }
                let mut request = vec![0_u8; length];
                let _ = reader.read_exact(&mut request);
                keep.lock().unwrap().push(String::from_utf8_lossy(&request).to_string());
                let mut stream = stream;
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }
        });
        (base, asked)
    }

    fn remote(base: String) -> Provider {
        Provider::Remote { base_url: base, key: "sk-secret-123".into(), model: "m".into() }
    }

    #[test]
    fn a_long_line_is_cut_at_sentence_ends_and_a_short_one_is_not() {
        assert_eq!(pieces("加一个支付服务。"), vec!["加一个支付服务。"]);
        let long = "首先用户在 App 里点下单，App 不直接打订单服务，而是先过 API 网关，网关这边做鉴权和限流。".repeat(4);
        let cut = pieces(&long);
        assert!(cut.len() > 1, "{cut:?}");
        assert!(cut.iter().all(|p| units(p) <= PIECE_UNITS), "{cut:?}");
        assert_eq!(cut.join("").chars().filter(|c| !c.is_whitespace()).collect::<String>(), long.chars().filter(|c| !c.is_whitespace()).collect::<String>());
        assert!(cut.iter().all(|p| p.ends_with('。')), "cut where sentences end: {cut:?}");
        // English, and a full stop inside a number is not an end.
        let en = "The mobile app talks to billing through the gateway at version 10.5 today. Billing writes to Postgres via a pooler. ".repeat(4);
        let cut = pieces(&en);
        assert!(cut.len() > 1 && cut.iter().all(|p| p.ends_with('.')), "{cut:?}");
        assert!(cut.iter().any(|p| p.contains("10.5")), "{cut:?}");
        // One sentence too long for a piece is cut at its commas.
        let run_on = format!("{}。", "订单服务调库存服务做预占，".repeat(12));
        assert!(pieces(&run_on).len() > 1);
        // Several sentences run together, as lines heard while the model was busy are, go about one at a time,
        // though the line is short; two sentences do not.
        let merged = "用户填手机号，后端发验证码。验证码校验通过就建账号。建完账号跳到引导页。引导页里选兴趣，兴趣交给推荐服务。";
        assert_eq!(pieces(merged), vec!["用户填手机号，后端发验证码。", "验证码校验通过就建账号。建完账号跳到引导页。", "引导页里选兴趣，兴趣交给推荐服务。"]);
        assert_eq!(pieces("先说注册。用户填手机号。").len(), 1);
    }

    #[test]
    fn each_piece_of_a_long_line_sees_the_board_the_pieces_before_it_drew() {
        let heard = Arc::new(Mutex::new(vec![]));
        let log = heard.clone();
        let provider = Provider::Scripted(Arc::new(move |said: &str| {
            log.lock().unwrap().push(said.to_string());
            let n = log.lock().unwrap().len();
            Ok(format!("N 「服务{n}」"))
        }));
        let line = Utterance { speaker: Some("A".into()), text: "我们有一个订单服务，它负责接收用户下单的请求并且做参数校验。".repeat(4), pointing: vec![] };
        let reply = propose_with(&provider, &Graph::default(), &[], &line, Guidance::default(), None, &sound::Glossary::default(), &[]).unwrap();
        let said = heard.lock().unwrap().clone();
        assert!(said.len() > 1, "said in pieces");
        assert!(said[1].contains("服务1"), "the second piece sees the first piece's node: {}", said[1]);
        let mut g = Graph::default();
        g.apply(&parse_ops(&reply.ops));
        assert_eq!(g.nodes.len(), said.len(), "every piece's ops come back: {}", reply.ops);
        // A remote model hears the line whole.
        let whole = remote("http://127.0.0.1:1".into());
        assert!(!whole.short_reply() && provider.short_reply() && Provider::Local { base: String::new() }.short_reply());
        // What was circled is not cut, so it happens once.
        heard.lock().unwrap().clear();
        let circled = Utterance { pointing: vec!["n1".into()], ..line };
        propose_with(&provider, &Graph::default(), &[], &circled, Guidance::default(), None, &sound::Glossary::default(), &[]).unwrap();
        assert_eq!(heard.lock().unwrap().len(), 1);
    }

    #[test]
    fn the_model_hears_the_glossary_and_the_labels_a_line_may_mean() {
        let seen = Arc::new(Mutex::new(String::new()));
        let heard = seen.clone();
        let provider = Provider::Scripted(Arc::new(move |said: &str| {
            *heard.lock().unwrap() = said.to_string();
            Ok(String::new())
        }));
        let mut g = Graph::default();
        g.apply(&boardcast_board::graph::parse_ops("T 「登录」\nL 「登录」 「鉴权」"));
        let line = Utterance { speaker: None, text: "把健全圈起来".into(), pointing: vec![] };
        propose_with(&provider, &g, &[], &line, Guidance::default(), None, &sound::Glossary::parse("OpenAI; API = A P I"), &[]).unwrap();
        let said = seen.lock().unwrap().clone();
        assert!(said.contains("术语表，名字照这样写：OpenAI、API"), "{said}");
        assert!(said.contains("「健全」可能是板上的「鉴权」"), "{said}");
        propose(&provider, &g, &[], &Utterance { speaker: None, text: "加一个支付服务".into(), pointing: vec![] }, Guidance::default(), None).unwrap();
        assert!(!seen.lock().unwrap().contains("术语表"), "nothing added without a glossary or a likely label");
    }

        #[test]
    fn an_unusable_reply_says_what_came_back() {
        let page = remote(serve("200 OK", "text/html", "<html>Bad gateway, try later</html>")).call(vec![json!({ "role": "user", "content": "hi" })], &Graph::default(), true);
        let e = format!("{:#}", page.unwrap_err());
        assert!(e.contains("not JSON") && e.contains("text/html") && e.contains("Bad gateway"), "{e}");

        let empty = remote(serve("200 OK", "application/json", r#"{"error":"overloaded"}"#)).call(vec![json!({ "role": "user", "content": "hi" })], &Graph::default(), true);
        let reply = empty.unwrap();
        let e = format!("{:#}", message(&reply).unwrap_err());
        assert!(e.contains("no message") && e.contains("overloaded"), "{e}");

        let refused = remote(serve("401 Unauthorized", "application/json", r#"{"error":"bad key sk-secret-123"}"#)).call(vec![json!({ "role": "user", "content": "hi" })], &Graph::default(), true);
        let e = format!("{:#}", refused.unwrap_err());
        assert!(e.contains("401") && !e.contains("sk-secret-123"), "the key never shows: {e}");

        let fine = remote(serve("200 OK", "application/json", r#"{"choices":[{"message":{"content":"T 「x」"}}]}"#)).call(vec![json!({ "role": "user", "content": "hi" })], &Graph::default(), true);
        assert_eq!(message(&fine.unwrap()).unwrap(), "T 「x」");
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
            self.said.send(Said { line: Utterance { speaker: None, text: text.into(), pointing: vec![] }, ended: Instant::now(), asr_ms: None, meeting: self.session.meeting(), renames: 0, span: None }).unwrap();
        }

        fn wait(&self) {
            self.drained.recv_timeout(Duration::from_secs(5)).unwrap();
        }
    }

    #[test]
    fn a_remote_model_is_offered_the_tools_and_its_calls_draw_the_line() {
        let (base, asked) = serve_recording(
            r#"{"choices":[{"message":{"content":"","tool_calls":[{"id":"c1","type":"function","function":{"name":"link","arguments":"{\"from\":\"甲\",\"to\":\"乙\",\"label\":\"调用\"}"}}]}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        );
        let (said, said_rx) = channel();
        let (ready_tx, ready_rx) = channel();
        let (drained_tx, drained) = channel();
        let (updates_tx, updates) = channel();
        let session = Session::start(Backend::Ready(remote(base)), None, said_rx, ready_tx, drained_tx, updates_tx);
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(updates.recv_timeout(Duration::from_secs(5)).unwrap(), Update::Ready));
        let h = Harness { session, said, updates, drained };
        h.say("甲调用乙");
        h.wait();
        let Update::Drawn { graph, ops, .. } = h.updates.recv_timeout(Duration::from_secs(5)).unwrap() else { panic!("expected a drawing") };
        assert_eq!((graph.nodes.len(), graph.edges.len()), (2, 1));
        assert_eq!(ops, "E 「甲」 -> 「乙」 : 「调用」", "the call comes back as the command it stands for");
        let body: Value = serde_json::from_str(&asked.lock().unwrap()[0]).unwrap();
        assert!(body["tools"].as_array().is_some_and(|t| t.iter().any(|t| t["function"]["name"] == "link")), "offered the tools");
        assert!(body.get("grammar").is_none(), "no grammar for a remote model");
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
        h.said.send(Said { line: Utterance { speaker: None, text: "排队的旧话".into(), pointing: vec![] }, ended: Instant::now(), asr_ms: None, meeting: stale, renames: 0, span: None }).unwrap();
        h.wait();
        h.say("新会议的话");
        h.wait();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].contains("新会议的话") && !seen[0].contains("排队的旧话"));
    }

    #[test]
    fn what_is_drawn_remembers_who_said_it_under_their_latest_name() {
        let (entered_tx, entered) = channel();
        let (release, release_rx) = channel::<()>();
        let release_rx = Mutex::new(release_rx);
        let h = harness(move |_| {
            entered_tx.send(()).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
            Ok("N n1 「网关」".into())
        });
        let line = |renames| Said { line: Utterance { speaker: Some("B".into()), text: "网关".into(), pointing: vec![] }, ended: Instant::now(), asr_ms: None, meeting: h.session.meeting(), renames, span: None };
        h.said.send(line(0)).unwrap();
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        // Renamed while the model is answering, with the name then given to someone else's next line.
        h.session.rename_speaker("B", "李工");
        h.session.rename_speaker("C", "B");
        h.said.send(Said { line: Utterance { speaker: Some("B".into()), text: "日志".into(), pointing: vec![] }, ..line(h.session.renames_seen()) }).unwrap();
        release.send(()).unwrap();
        h.wait();
        let Update::Drawn { graph, lines, .. } = h.updates.recv_timeout(Duration::from_secs(5)).unwrap() else { panic!("not drawn") };
        assert_eq!(lines[0].speaker.as_deref(), Some("李工"));
        assert_eq!(boardcast_board::speakers::by_of_node(&graph.nodes[0]), Some("李工"));
        release.send(()).unwrap();
        h.wait();
        let Update::Drawn { lines, .. } = h.updates.recv_timeout(Duration::from_secs(5)).unwrap() else { panic!("not drawn") };
        assert_eq!(lines[0].speaker.as_deref(), Some("B"), "the name now belongs to the next speaker");
    }

    #[test]
    fn a_speakers_pieces_reach_the_board_as_one_turn() {
        let asked = Arc::new(Mutex::new(Vec::<String>::new()));
        let seen = asked.clone();
        let h = harness(move |said| {
            seen.lock().unwrap().push(said.to_string());
            Ok("-".into())
        });
        let heard = |who: &str, text: &str, from: f64, to: f64| Said {
            line: Utterance { speaker: Some(who.into()), text: text.into(), pointing: vec![] },
            ended: Instant::now(),
            asr_ms: Some(100),
            meeting: h.session.meeting(),
            renames: 0,
            span: Some((from, to)),
        };
        h.said.send(heard("C", "对，而且你现在设计的产品。", 0.0, 2.0)).unwrap();
        h.said.send(heard("A", "对啊", 2.1, 2.4)).unwrap();
        h.said.send(heard("C", "主要推出的是它有照相的功能。", 2.5, 5.0)).unwrap();
        // Held until the speaker has been quiet a while, then drawn once.
        h.wait();
        let asked = asked.lock().unwrap();
        assert_eq!(asked.len(), 1, "{asked:?}");
        assert!(asked[0].contains("新的一句：C：对，而且你现在设计的产品。主要推出的是它有照相的功能。"), "{}", asked[0]);
        assert_eq!(h.session.history.lock().unwrap().len(), 3, "every piece is in the discussion, the aside too");
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
        said.send(Said { line: Utterance { speaker: None, text: "这句没有模型".into(), pointing: vec![] }, ended: Instant::now(), asr_ms: None, meeting: session.meeting(), renames: 0, span: None }).unwrap();
        drained.recv_timeout(Duration::from_secs(5)).unwrap();
        let Update::Failed { lines, .. } = updates.recv_timeout(Duration::from_secs(5)).unwrap() else { panic!("expected a failure") };
        assert_eq!(lines[0].text, "这句没有模型");
        session.set_backend(Backend::Ready(Provider::Scripted(Arc::new(|_| Ok("T 「现在有了」".into())))));
        said.send(Said { line: Utterance { speaker: None, text: "再试".into(), pointing: vec![] }, ended: Instant::now(), asr_ms: None, meeting: session.meeting(), renames: 0, span: None }).unwrap();
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
