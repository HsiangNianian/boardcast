//! Meeting search as a worker the app talks to by messages: it keeps an index for each meeting up to date
//! and answers queries, with every slow and blocking call on its own thread.
//!
//! The embedding model runs in its own llama-server on the CPU, apart from the board model so neither
//! waits for the other or competes for the graphics card. It starts when first needed and stops after a
//! few idle minutes. Indexes live in `<data>/search/<meeting>.index`, beside the meetings and not in their
//! files, so the meeting format and the older app are untouched.

use crate::chunk::{self, Chunk, Said, Source};
use crate::embed::{cut, Embedder};
use crate::index::{fingerprint, Index};
use crate::{rank, DIMS, FAMILY, MIN_SCORE, MODEL, WITHIN_BEST, WORD_WEIGHT};
use anyhow::{bail, Context, Result};
use boardcast_meetings::{Live, Store};
use std::collections::HashMap;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::collections::VecDeque;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

/// The most passages shown from any one meeting.
const PER_MEETING: usize = 3;
/// How long the embedding server is kept once nothing has asked for it.
const IDLE: Duration = Duration::from_secs(180);

/// Starts llama-server with the given arguments, tied to the app's life. The app passes it in, so this
/// crate needs nothing of how the app finds and starts its binaries.
pub type Spawn = Box<dyn Fn(&[String]) -> Result<Child> + Send>;

pub enum Job {
    /// Bring these meetings' indexes up to date, in this order.
    Index(Vec<String>),
    /// Find `text` among these meetings: at most `limit` passages of where things stand, then at most
    /// `limit` of what was said.
    Find { text: String, meetings: Vec<String>, limit: usize, ticket: u64 },
    /// Forget a meeting's index, as when it is removed for good.
    Forget(String),
}

/// A passage found, with what the interface needs to show it and open the meeting there.
#[derive(Clone, Debug)]
pub struct Found {
    pub meeting: String,
    pub title: String,
    pub source: Source,
    pub text: String,
    pub score: f32,
}

impl Found {
    /// Whether a passage says where things stand, as a board or a summary does, rather than what was said.
    pub fn stands_for(source: &Source) -> bool {
        !matches!(source, Source::Transcript { .. })
    }

    pub fn stands(&self) -> bool {
        Self::stands_for(&self.source)
    }
}

pub enum Event {
    /// Indexing went on: `done` of `total` meetings looked at in this round.
    Indexing { done: usize, total: usize },
    /// A meeting's index was made again: it has `passages`, of which `embedded` had to be embedded anew.
    Indexed { meeting: String, passages: usize, embedded: usize },
    Found { ticket: u64, hits: Vec<Found> },
    Failed(String),
}

/// The passages of a meeting, and the text its index fingerprint is taken from.
pub fn passages(live: &Live) -> Vec<Chunk> {
    let said: Vec<Said> = live
        .lines
        .iter()
        .map(|l| Said { speaker: l["speaker"].as_str(), text: l["text"].as_str().unwrap_or_default() })
        .collect();
    let mut out = chunk::transcript(&said);
    for (page, (graph, _)) in live.board_pages().iter().enumerate() {
        out.extend(chunk::board(page, graph));
    }
    out.extend(chunk::summary(live.summary()));
    out
}

struct Server {
    child: Child,
    embedder: Embedder,
    last_used: Instant,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Worker {
    store: Store,
    folder: PathBuf,
    model: PathBuf,
    spawn: Spawn,
    server: Option<Server>,
    /// Indexes read from disk, with the time of the file they came from.
    cache: HashMap<String, (SystemTime, Index)>,
}

/// Where a meeting's index is kept, under the app's data folder.
fn index_path(folder: &Path, meeting: &str) -> PathBuf {
    folder.join(format!("{meeting}.index"))
}

/// How many of these meetings have no index yet, as before the first search on this computer. The app
/// says so while it builds them, as that takes a while; keeping indexes up to date afterwards does not.
pub fn unindexed(data: &Path, meetings: &[String]) -> usize {
    let folder = data.join("search");
    meetings.iter().filter(|m| !index_path(&folder, m).is_file()).count()
}

impl Worker {
    fn path(&self, meeting: &str) -> PathBuf {
        index_path(&self.folder, meeting)
    }

    fn embedder(&mut self) -> Result<&Embedder> {
        if self.server.as_mut().is_some_and(|s| matches!(s.child.try_wait(), Ok(None))) {
            let server = self.server.as_mut().expect("checked");
            server.last_used = Instant::now();
            return Ok(&server.embedder);
        }
        self.server = None;
        if !self.model.is_file() {
            bail!("the meeting search model is not downloaded");
        }
        let port = TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
        let args: Vec<String> = [
            "-m", &self.model.to_string_lossy(), "--embeddings", "--host", "127.0.0.1", "--port", &port.to_string(),
            // On the CPU, a few passages at a time, each well within the model's context.
            "-c", "8192", "-np", "4", "-b", "2048", "-ub", "2048", "--device", "none", "-ngl", "0",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let child = (self.spawn)(&args).context("starting the meeting search model")?;
        let base = format!("http://127.0.0.1:{port}");
        let health = format!("{base}/health");
        let mut server = Server { child, embedder: Embedder::new(&base, FAMILY, DIMS), last_used: Instant::now() };
        let started = Instant::now();
        loop {
            if ureq::get(&health).timeout(Duration::from_secs(2)).call().is_ok_and(|r| r.status() == 200) {
                break;
            }
            if !matches!(server.child.try_wait(), Ok(None)) {
                bail!("the meeting search model stopped while starting");
            }
            if started.elapsed() > Duration::from_secs(60) {
                bail!("the meeting search model did not start within a minute");
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Ok(&self.server.insert(server).embedder)
    }

    /// Make one meeting's index again if what it was made from has changed. Passages it already had, read
    /// with the same title, keep their vectors, so a meeting that grew by a few lines costs only those.
    /// Says how many passages it has, and how many were embedded.
    fn index(&mut self, meeting: &str) -> Result<Option<(usize, usize)>> {
        let Some(loaded) = self.store.load(meeting)? else { return Ok(None) };
        let live = Live::from_loaded(loaded);
        let chunks = passages(&live);
        let content: String = std::iter::once(live.title().to_string()).chain(chunks.iter().map(|c| c.text.clone())).collect::<Vec<_>>().join("\n\u{1f}");
        let print = fingerprint(MODEL, DIMS, &content);
        let path = self.path(meeting);
        let before = Index::load(&path).ok();
        if before.as_ref().is_some_and(|i| i.fingerprint == print) {
            return Ok(None);
        }
        let title = live.title().to_string();
        let texts: Vec<String> = chunks.iter().map(|c| FAMILY.passage(&title, &c.text)).collect();
        let known: HashMap<String, Vec<f32>> = before
            .filter(|i| i.same_kind(&print))
            .map(|i| i.chunks.iter().enumerate().map(|(n, c)| (FAMILY.passage(&i.title, &c.text), i.vector(n).to_vec())).collect())
            .unwrap_or_default();
        let missing: Vec<String> = texts.iter().filter(|t| !known.contains_key(*t)).cloned().collect();
        let fresh: HashMap<String, Vec<f32>> = if missing.is_empty() { HashMap::new() } else { missing.iter().cloned().zip(self.embedder()?.embed(&missing)?).collect() };
        let vectors: Vec<Vec<f32>> = texts.iter().map(|t| known.get(t).or_else(|| fresh.get(t)).cloned().expect("every passage has a vector")).collect();
        let (count, embedded) = (chunks.len(), missing.len());
        std::fs::create_dir_all(&self.folder)?;
        Index::new(print, DIMS, chunks, vectors).titled(&title).save(&path)?;
        self.cache.remove(meeting);
        Ok(Some((count, embedded)))
    }

    /// An index from disk, read again only when its file changed.
    fn load(&mut self, meeting: &str) -> Option<&Index> {
        let path = self.path(meeting);
        let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
        let fresh = self.cache.get(meeting).is_some_and(|(at, _)| *at == modified);
        if !fresh {
            let index = Index::load(&path).ok()?;
            self.cache.insert(meeting.to_string(), (modified, index));
        }
        self.cache.get(meeting).map(|(_, i)| i)
    }

    fn find(&mut self, text: &str, meetings: &[String], limit: usize) -> Result<Vec<Found>> {
        let query = cut(&self.embedder()?.query(text)?, DIMS);
        let titles: HashMap<String, String> = self.store.list().unwrap_or_default().into_iter().map(|m| (m.id, m.title)).collect();
        let mut indexes = vec![];
        for id in meetings {
            if let Some(index) = self.load(id) {
                indexes.push((id.clone(), index.clone()));
            }
        }
        let hits = rank(&indexes, &query, text, WORD_WEIGHT, usize::MAX);
        // Where things stand (boards and summaries, as the meeting left them) and what was said on the way
        // (the transcript) answer different questions, so each is cut on its own: a conclusion a little less
        // like the question than some remark is still the answer.
        let mut out = vec![];
        for stands in [true, false] {
            let group: Vec<_> = hits.iter().filter(|h| Found::stands_for(&h.chunk.source) == stands).collect();
            // Nothing that matches only weakly, by itself or next to what matched best: no answer beats a wrong one.
            let best = group.first().map_or(0.0, |h| h.score);
            // A few passages from each meeting at most, so one long meeting does not crowd out the rest.
            let mut per_meeting: HashMap<&str, usize> = HashMap::new();
            // The same words in one meeting, as a node kept on two boards, are shown once.
            let mut seen: std::collections::HashSet<(&str, &str)> = std::collections::HashSet::new();
            out.extend(
                group
                    .into_iter()
                    .filter(|h| h.score >= MIN_SCORE && h.score >= best - WITHIN_BEST)
                    .filter(|h| seen.insert((h.meeting, h.chunk.text.as_str())))
                    .filter(|h| {
                        let seen = per_meeting.entry(h.meeting).or_default();
                        *seen += 1;
                        *seen <= PER_MEETING
                    })
                    .take(limit)
                    .map(|h| Found { meeting: h.meeting.to_string(), title: titles.get(h.meeting).cloned().unwrap_or_default(), source: h.chunk.source.clone(), text: h.chunk.text.clone(), score: h.score }),
            );
        }
        Ok(out)
    }
}

/// Start the worker. `data` is the app's data folder (the one holding `meetings/`), `model` the
/// embedding model's file. Dropping the sender stops it, and its server with it.
pub fn start(data: &Path, model: PathBuf, spawn: Spawn) -> (Sender<Job>, Receiver<Event>) {
    let (jobs, inbox) = channel::<Job>();
    let (outbox, events) = channel::<Event>();
    let mut worker = Worker { store: Store::new(data.to_path_buf()), folder: data.join("search"), model, spawn, server: None, cache: HashMap::new() };
    std::thread::Builder::new()
        .name("meeting search".into())
        .spawn(move || {
            // Meetings waiting to be indexed, and how far through this round it is. A question asked
            // meanwhile is answered between two meetings, not after all of them.
            let mut todo: VecDeque<String> = VecDeque::new();
            let (mut done, mut total) = (0, 0);
            loop {
                let job = if todo.is_empty() {
                    match inbox.recv_timeout(Duration::from_secs(15)) {
                        Ok(job) => Some(job),
                        Err(RecvTimeoutError::Timeout) => {
                            if worker.server.as_ref().is_some_and(|s| s.last_used.elapsed() > IDLE) {
                                worker.server = None;
                            }
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                } else {
                    match inbox.try_recv() {
                        Ok(job) => Some(job),
                        Err(TryRecvError::Empty) => None,
                        Err(TryRecvError::Disconnected) => return,
                    }
                };
                let reply = match job {
                    Some(Job::Index(meetings)) => {
                        for id in meetings {
                            if !todo.contains(&id) {
                                todo.push_back(id);
                                total += 1;
                            }
                        }
                        continue;
                    }
                    Some(Job::Find { text, meetings, limit, ticket }) => match worker.find(&text, &meetings, limit) {
                        Ok(hits) => Event::Found { ticket, hits },
                        Err(e) => Event::Failed(format!("{e:#}")),
                    },
                    Some(Job::Forget(id)) => {
                        let _ = std::fs::remove_file(worker.path(&id));
                        worker.cache.remove(&id);
                        todo.retain(|t| *t != id);
                        continue;
                    }
                    None => {
                        let id = todo.pop_front().expect("there is a meeting to index");
                        let _ = outbox.send(Event::Indexing { done, total });
                        let result = worker.index(&id);
                        done += 1;
                        let reply = match result {
                            Ok(Some((passages, embedded))) => Some(Event::Indexed { meeting: id, passages, embedded }),
                            Ok(None) => None,
                            Err(e) => {
                                todo.clear();
                                Some(Event::Failed(format!("{e:#}")))
                            }
                        };
                        if let Some(reply) = reply {
                            if outbox.send(reply).is_err() {
                                return;
                            }
                        }
                        if !todo.is_empty() {
                            continue;
                        }
                        let finished = Event::Indexing { done: total, total };
                        (done, total) = (0, 0);
                        finished
                    }
                };
                if outbox.send(reply).is_err() {
                    return;
                }
            }
        })
        .expect("a thread for meeting search");
    (jobs, events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn meetings_without_an_index_are_counted() {
        let data = std::env::temp_dir().join(format!("boardcast-unindexed-{}", std::process::id()));
        let meetings = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(unindexed(&data, &meetings), 3, "no search folder at all");
        std::fs::create_dir_all(data.join("search")).unwrap();
        std::fs::write(data.join("search/b.index"), b"").unwrap();
        assert_eq!(unindexed(&data, &meetings), 2);
        let _ = std::fs::remove_dir_all(&data);
    }

    /// The whole round on a real model: index two meetings, then find one by what it means. Skipped where
    /// the model (eval/search/.models) or llama-server (resources/llama) is not there.
    #[test]
    fn a_question_finds_its_meeting_by_meaning() {
        let root = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
        let model = root.join("eval/search/.models/embeddinggemma-300M-Q8_0.gguf");
        let binaries = root.join("resources/llama");
        if !model.is_file() || !binaries.join("llama-server").is_file() {
            eprintln!("skipped: no search model or llama-server here");
            return;
        }
        let data = std::env::temp_dir().join(format!("boardcast-search-service-{}", std::process::id()));
        let meetings = data.join("meetings/v2");
        std::fs::create_dir_all(&meetings).unwrap();
        let meeting = |id: &str, title: &str, lines: &[&str]| {
            let lines: Vec<_> = lines.iter().map(|t| json!({ "text": t, "speaker": "A" })).collect();
            std::fs::write(meetings.join(format!("{id}.json")), json!({ "version": 2, "id": id, "title": title, "lines": lines }).to_string()).unwrap();
        };
        meeting("20260101-000000-aaaa", "Release review", &["We moved the ship date from Friday to Tuesday.", "Load testing has to finish before launch."]);
        meeting("20260101-000000-bbbb", "物业例会", &["地下车库的车位按先来后到分配。", "物业费下个月起按季度收。"]);
        let spawn: Spawn = Box::new(move |args| {
            let mut cmd = std::process::Command::new(binaries.join("llama-server"));
            cmd.env("LD_LIBRARY_PATH", &binaries).args(args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
            Ok(cmd.spawn()?)
        });
        let (jobs, events) = start(&data, model, spawn);
        let ids = vec!["20260101-000000-aaaa".to_string(), "20260101-000000-bbbb".to_string()];
        jobs.send(Job::Index(ids.clone())).unwrap();
        let wait = |want: &dyn Fn(&Event) -> bool| loop {
            let event = events.recv_timeout(Duration::from_secs(120)).expect("an answer from the worker");
            if let Event::Failed(e) = &event {
                panic!("{e}");
            }
            if want(&event) {
                return event;
            }
        };
        wait(&|e| matches!(e, Event::Indexing { done, total } if done == total));
        assert!(data.join("search/20260101-000000-aaaa.index").is_file());
        jobs.send(Job::Find { text: "when are we shipping now?".into(), meetings: ids.clone(), limit: 3, ticket: 1 }).unwrap();
        let Event::Found { hits, .. } = wait(&|e| matches!(e, Event::Found { .. })) else { unreachable!() };
        assert_eq!(hits[0].meeting, "20260101-000000-aaaa", "{:?}", hits.iter().map(|h| &h.text).collect::<Vec<_>>());
        assert_eq!(hits[0].title, "Release review");
        // Across languages: an English question about the Chinese meeting.
        jobs.send(Job::Find { text: "how is the parking garage shared out".into(), meetings: ids, limit: 3, ticket: 2 }).unwrap();
        let Event::Found { hits, .. } = wait(&|e| matches!(e, Event::Found { .. })) else { unreachable!() };
        assert_eq!(hits[0].meeting, "20260101-000000-bbbb");
        // A meeting that grew by a line embeds only the passages at its end, not the whole meeting again.
        let long: Vec<String> = (1..=14).map(|i| format!("Item {i} of the plan: the team reviewed the budget, the schedule and the risks for part {i} in detail.")).collect();
        let lines: Vec<&str> = long.iter().map(String::as_str).collect();
        meeting("20260101-000000-cccc", "Planning", &lines);
        jobs.send(Job::Index(vec!["20260101-000000-cccc".into()])).unwrap();
        let Event::Indexed { passages, embedded, .. } = wait(&|e| matches!(e, Event::Indexed { .. })) else { unreachable!() };
        assert_eq!(embedded, passages);
        let mut grown = lines.clone();
        grown.push("One more thing: the launch moves to Tuesday.");
        meeting("20260101-000000-cccc", "Planning", &grown);
        jobs.send(Job::Index(vec!["20260101-000000-cccc".into()])).unwrap();
        let Event::Indexed { passages, embedded, .. } = wait(&|e| matches!(e, Event::Indexed { .. })) else { unreachable!() };
        assert!(passages >= 4 && embedded <= 2, "{embedded} of {passages} embedded again");
        // A second round leaves unchanged meetings as they are.
        let before = std::fs::metadata(data.join("search/20260101-000000-aaaa.index")).unwrap().modified().unwrap();
        jobs.send(Job::Index(vec!["20260101-000000-aaaa".into()])).unwrap();
        wait(&|e| matches!(e, Event::Indexing { done, total } if done == total));
        assert_eq!(std::fs::metadata(data.join("search/20260101-000000-aaaa.index")).unwrap().modified().unwrap(), before);
        drop(jobs);
        let _ = std::fs::remove_dir_all(&data);
    }
}
