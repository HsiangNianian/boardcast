//! A meeting shown to others on the same network: a small read-only web server the desktop app starts by
//! hand. A viewer opens its address in a browser, gives the access code (the QR code carries it), and
//! watches the board grow as the meeting goes on.
//!
//! The page is one HTML file with the meeting as it was inside; the meeting then follows as Server-Sent Events
//! on `/events`. Each board, the transcript and each page image are sent to a viewer only when they changed
//! since that viewer last had them, so a long meeting stays light on a phone and on Wi-Fi. Nothing is read
//! from viewers but the address they ask for.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Ports tried first, so the address stays the same from one meeting to the next when it can.
const PORTS: std::ops::RangeInclusive<u16> = 4747..=4757;
/// The code's letters: no 0/O, 1/I/L, so it reads aloud and types without mistakes.
const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const CODE_LENGTH: usize = 6;
/// How long a wrong code waits before it is answered.
const WRONG_CODE_DELAY: Duration = Duration::from_secs(1);
/// Wrong codes one device may give in a while before it is refused for the rest of it.
const WRONG_PER_DEVICE: u32 = 5;
const WRONG_WINDOW: Duration = Duration::from_secs(10 * 60);
/// Wrong codes from all devices together after which the code is given up and sharing stops: someone is
/// guessing, and a new code is the answer.
const WRONG_IN_ALL: usize = 50;
/// Connections at once, viewers and requests together: more are turned away rather than given a thread.
const MAX_CONNECTIONS: usize = 64;
/// However quiet the meeting, a stream says something this often, so a viewer gone away is noticed.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// What viewers are shown now.
#[derive(Default)]
struct Shown {
    /// Bumped on every change, so each viewer's stream knows when there is something new.
    version: u64,
    /// The page, with the meeting as it is now inside it, for a viewer arriving.
    page: String,
    /// The meeting without its boards, its transcript and its page images: small, so sent on every change.
    data: Value,
    /// Each board, the transcript and each board's page image, with the version each last changed at.
    boards: Vec<(u64, Value)>,
    lines: (u64, Value),
    images: Vec<(u64, Option<String>)>,
}

/// Each part as it is now, keeping the version it changed at when it is the same as before.
fn versioned<T: PartialEq>(before: Vec<(u64, T)>, now: Vec<T>, version: u64) -> Vec<(u64, T)> {
    let mut before = before.into_iter();
    now.into_iter()
        .map(|part| match before.next() {
            Some((at, same)) if same == part => (at, part),
            _ => (version, part),
        })
        .collect()
}

struct State {
    code: String,
    shown: Mutex<Shown>,
    changed: Condvar,
    stop: AtomicBool,
    viewers: AtomicUsize,
    connections: AtomicUsize,
    /// Wrong codes by device, with when the first of them in the current while was given.
    wrong: Mutex<HashMap<IpAddr, (u32, Instant)>>,
    wrong_in_all: AtomicUsize,
    /// Too many wrong codes were given: the code is given up and sharing has stopped.
    retired: AtomicBool,
}

/// A running share; it stops when dropped.
pub struct Share {
    state: Arc<State>,
    port: u16,
    /// Where it can be reached, the likeliest first: on each network this computer is on.
    networks: Vec<Network>,
}

/// A network this computer is on: its address there, and what kind it is, as far as its name tells.
#[derive(Clone, Debug, PartialEq)]
pub struct Network {
    pub ip: IpAddr,
    pub kind: Kind,
    /// The interface's own name, for when its kind cannot be told.
    pub interface: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    WiFi,
    Wired,
    Unknown,
}

/// What kind of network an interface is on, from its name: `wlan0`, `wlp3s0` and `Wi-Fi` are wireless,
/// `eth0`, `enp59s0` and `Ethernet` are wired. A Mac names both `en`, so it is not told.
fn kind(interface: &str) -> Kind {
    let name = interface.to_ascii_lowercase();
    if name.starts_with("wl") || name.contains("wi-fi") || name.contains("wifi") || name.contains("wireless") || name.contains("wlan") {
        Kind::WiFi
    } else if (name.starts_with("eth") || name.starts_with("enp") || name.starts_with("eno") || name.starts_with("ens") || name.starts_with("enx") || name.contains("ethernet")) && !cfg!(target_os = "macos") {
        Kind::Wired
    } else {
        Kind::Unknown
    }
}

impl Share {
    /// Start listening on the network, with a new access code.
    /// Listen on each of this computer's local networks, and nowhere else: not on a VPN, a proxy's tunnel or
    /// a container's bridge.
    pub fn start() -> Result<Self> {
        Self::start_on(local_networks())
    }

    fn start_on(networks: Vec<Network>) -> Result<Self> {
        let bind = |port: u16| networks.iter().map(|n| TcpListener::bind((n.ip, port))).collect::<std::io::Result<Vec<_>>>();
        // One port for every network, so the address differs only in the network's part.
        let listeners = match PORTS.clone().find_map(|port| bind(port).ok()) {
            Some(listeners) => listeners,
            None => {
                let first = TcpListener::bind((networks.first().context("no network to share on")?.ip, 0)).context("listening on the network")?;
                let port = first.local_addr()?.port();
                std::iter::once(Ok(first)).chain(networks[1..].iter().map(|n| TcpListener::bind((n.ip, port)))).collect::<std::io::Result<Vec<_>>>().context("listening on the network")?
            }
        };
        let port = listeners[0].local_addr()?.port();
        let state = Arc::new(State {
            code: new_code()?,
            shown: Mutex::default(),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
            viewers: AtomicUsize::new(0),
            connections: AtomicUsize::new(0),
            wrong: Mutex::default(),
            wrong_in_all: AtomicUsize::new(0),
            retired: AtomicBool::new(false),
        });
        for listener in listeners {
            let accepting = state.clone();
            std::thread::Builder::new().name("share".into()).spawn(move || {
                for stream in listener.incoming() {
                    if accepting.stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(mut stream) = stream else { continue };
                    if accepting.connections.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
                        let _ = respond(&mut stream, "503 Service Unavailable", "text/plain", "too many connections");
                        continue;
                    }
                    accepting.connections.fetch_add(1, Ordering::Relaxed);
                    let state = accepting.clone();
                    let spawned = std::thread::Builder::new().name("share-viewer".into()).spawn(move || {
                        let _ = serve(stream, &state);
                        state.connections.fetch_sub(1, Ordering::Relaxed);
                    });
                    if spawned.is_err() {
                        accepting.connections.fetch_sub(1, Ordering::Relaxed);
                    }
                }
            })?;
        }
        Ok(Self { state, port, networks })
    }

    /// Whether sharing stopped because too many wrong codes were given.
    pub fn retired(&self) -> bool {
        self.state.retired.load(Ordering::Relaxed)
    }

    /// The access code a viewer gives.
    pub fn code(&self) -> &str {
        &self.state.code
    }

    /// The networks it can be reached on, the likeliest first: Wi-Fi before wired, where phones are.
    pub fn networks(&self) -> &[Network] {
        &self.networks
    }

    /// The address to open on a network, without the code.
    pub fn address(&self, network: &Network) -> String {
        format!("http://{}", SocketAddr::new(network.ip, self.port))
    }

    /// An address with the code in it, for the QR code: opening it needs nothing typed.
    pub fn link(&self, address: &str) -> String {
        format!("{address}/?c={}", self.state.code)
    }

    /// How many are watching now.
    pub fn viewers(&self) -> usize {
        self.state.viewers.load(Ordering::Relaxed)
    }

    /// Show this from now on: the page a viewer arriving gets, the meeting for the stream (its `boards` and
    /// `lines` are sent apart, each when it changed), and each board's page image as a data URL. The page
    /// learns which version it holds from `"live":true` in it, so its stream starts after that.
    pub fn show(&self, page: String, mut data: Value, images: Vec<Option<String>>) {
        let mut shown = self.state.shown.lock().unwrap_or_else(|e| e.into_inner());
        shown.version += 1;
        let version = shown.version;
        let (boards, lines) = match data.as_object_mut() {
            Some(parts) => (
                match parts.remove("boards") {
                    Some(Value::Array(boards)) => Some(boards),
                    _ => None,
                },
                parts.remove("lines").unwrap_or(Value::Null),
            ),
            None => (None, Value::Null),
        };
        data["boards"] = json!(boards.as_ref().map_or(0, Vec::len));
        let before = std::mem::take(&mut shown.boards);
        shown.boards = versioned(before, boards.unwrap_or_default(), version);
        if shown.lines.1 != lines {
            shown.lines = (version, lines);
        }
        let before = std::mem::take(&mut shown.images);
        shown.images = versioned(before, images, version);
        shown.page = page.replacen("\"live\":true", &format!("\"live\":{version}"), 1);
        shown.data = data;
        drop(shown);
        self.state.changed.notify_all();
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::Relaxed);
        self.state.changed.notify_all();
        // Each listener is waiting for a connection; one from here lets it see that it should stop.
        for network in &self.networks {
            let _ = TcpStream::connect_timeout(&SocketAddr::new(network.ip, self.port), Duration::from_millis(200));
        }
    }
}

/// A code from the system's randomness.
fn new_code() -> Result<String> {
    let mut bytes = [0u8; CODE_LENGTH];
    getrandom::fill(&mut bytes).map_err(|e| anyhow::anyhow!("no randomness for the access code: {e}"))?;
    Ok(bytes.iter().map(|b| ALPHABET[usize::from(*b) % ALPHABET.len()] as char).collect())
}

/// This computer's addresses on the local networks it is on, where others reach it, the likeliest first: it
/// can be on a wired network and on Wi-Fi at once. Private addresses of real interfaces only: a VPN or a
/// proxy's tunnel often holds the route out (198.18.0.0/15 is a common one), and its address is no use to
/// someone in the same room. Failing all of them, the address traffic leaves from.
fn local_networks() -> Vec<Network> {
    let mut lan: Vec<((u8, u8, u8), Network)> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            IpAddr::V4(ip) if ip.is_private() => Some((rank(&i.name, ip), Network { ip: IpAddr::V4(ip), kind: kind(&i.name), interface: i.name.clone() })),
            _ => None,
        })
        .collect();
    lan.sort_by(|a, b| a.0.cmp(&b.0));
    lan.dedup_by(|a, b| a.1.ip == b.1.ip);
    // A virtual network only when there is no real one: a container's bridge is not where a phone is.
    if lan.iter().any(|((virtual_, ..), _)| *virtual_ == 0) {
        lan.retain(|((virtual_, ..), _)| *virtual_ == 0);
    }
    let mut out: Vec<Network> = lan.into_iter().map(|(_, network)| network).collect();
    if out.is_empty() {
        out.push(Network { ip: local_address(), kind: Kind::Unknown, interface: String::new() });
    }
    out
}

/// The address traffic leaves from, when no interface has a private one. A UDP socket only picks its route on
/// connecting, and sends nothing.
fn local_address() -> IpAddr {
    UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .and_then(|socket| {
            socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9))?;
            socket.local_addr()
        })
        .map(|a| a.ip())
        .ok()
        .filter(|ip| !ip.is_unspecified())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

/// Which private address to show first: a physical interface before a virtual one (containers, virtual
/// machines, tunnels), Wi-Fi before the rest, since phones are on it, and a home or office network's usual
/// ranges before others.
fn rank(interface: &str, ip: Ipv4Addr) -> (u8, u8, u8) {
    let [a, b, ..] = ip.octets();
    let range = match (a, b) {
        (192, 168) => 0,
        (10, _) => 1,
        _ => 2,
    };
    let name = interface.to_ascii_lowercase();
    let virtual_ = ["docker", "br-", "veth", "virbr", "vmnet", "vboxnet", "tun", "tap", "utun", "wg", "zt", "tailscale", "vethernet", "flclash", "clash", "mihomo"].iter().any(|v| name.starts_with(v));
    (u8::from(virtual_), u8::from(kind(interface) != Kind::WiFi), range)
}

/// The path and query of a request, from its first line.
fn request(stream: &TcpStream) -> Result<(String, String)> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut reader = BufReader::new(stream.take(16 * 1024));
    let mut first = String::new();
    reader.read_line(&mut first)?;
    // The headers are read and left: nothing in them changes the answer.
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        line.clear();
    }
    let target = first.split_whitespace().nth(1).context("not an HTTP request")?;
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    Ok((path.to_string(), query.to_string()))
}

/// A value from a query string such as `c=ABC234`.
fn query_value<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| pair.split_once('=').filter(|(k, _)| *k == name).map(|(_, v)| v))
}

/// The same code, compared in the same time whatever it is, as typed: case and spaces do not matter.
fn same_code(given: &str, code: &str) -> bool {
    let given: Vec<u8> = given.bytes().filter(|b| b.is_ascii_alphanumeric()).map(|b| b.to_ascii_uppercase()).collect();
    given.len() == code.len() && given.iter().zip(code.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &str) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    Ok(())
}

fn serve(mut stream: TcpStream, state: &State) -> Result<()> {
    let (path, query) = request(&stream)?;
    let device = stream.peer_addr()?.ip();
    if state.retired.load(Ordering::Relaxed) || state.stop.load(Ordering::Relaxed) {
        return respond(&mut stream, "410 Gone", "text/plain", "sharing has stopped");
    }
    let given = query_value(&query, "c");
    // A device that gave too many wrong codes is not asked again for a while, right or wrong.
    if given.is_some() && refused(state, device) {
        return respond(&mut stream, "429 Too Many Requests", "text/html; charset=utf-8", &code_page(Some("Too many wrong codes from this device. Try again later. / 这台设备输错太多次，请稍后再试。")));
    }
    let allowed = given.is_some_and(|c| same_code(c, &state.code));
    if given.is_some() && !allowed {
        wrong_code(state, device);
        std::thread::sleep(WRONG_CODE_DELAY);
    }
    match path.as_str() {
        "/" if allowed => {
            let page = state.shown.lock().unwrap_or_else(|e| e.into_inner()).page.clone();
            respond(&mut stream, "200 OK", "text/html; charset=utf-8", &page)
        }
        "/" => respond(&mut stream, if given.is_some() { "403 Forbidden" } else { "200 OK" }, "text/html; charset=utf-8", &code_page(given.is_some().then_some("That code is not right. / 访问码不对。"))),
        "/events" if allowed => events(stream, state, query_value(&query, "since").and_then(|v| v.parse().ok()).unwrap_or(0)),
        "/events" => respond(&mut stream, "403 Forbidden", "text/plain", "wrong code"),
        _ => respond(&mut stream, "404 Not Found", "text/plain", "not found"),
    }
}

/// Whether a device has given too many wrong codes lately.
fn refused(state: &State, device: IpAddr) -> bool {
    let wrong = state.wrong.lock().unwrap_or_else(|e| e.into_inner());
    wrong.get(&device).is_some_and(|(count, since)| *count >= WRONG_PER_DEVICE && since.elapsed() < WRONG_WINDOW)
}

/// Count a wrong code against its device and against them all; too many in all gives the code up.
fn wrong_code(state: &State, device: IpAddr) {
    let mut wrong = state.wrong.lock().unwrap_or_else(|e| e.into_inner());
    let entry = wrong.entry(device).or_insert((0, Instant::now()));
    if entry.1.elapsed() >= WRONG_WINDOW {
        *entry = (0, Instant::now());
    }
    entry.0 += 1;
    drop(wrong);
    if state.wrong_in_all.fetch_add(1, Ordering::Relaxed) + 1 >= WRONG_IN_ALL {
        state.retired.store(true, Ordering::Relaxed);
        state.changed.notify_all();
    }
}

/// The meeting as it changes, until the viewer leaves or sharing stops. `since` is the version the viewer's
/// page already holds, whose boards and transcript need not come again.
fn events(mut stream: TcpStream, state: &State, since: u64) -> Result<()> {
    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nX-Accel-Buffering: no\r\nConnection: keep-alive\r\n\r\nretry: 2000\n\n")?;
    state.viewers.fetch_add(1, Ordering::Relaxed);
    let result = stream_changes(&mut stream, state, since);
    state.viewers.fetch_sub(1, Ordering::Relaxed);
    result
}

fn stream_changes(stream: &mut TcpStream, state: &State, since: u64) -> Result<()> {
    // What this viewer has: everything up to `since` but the page images, which the page never holds.
    let (mut seen, mut images_seen, mut boards_seen, mut lines_seen) = (0u64, Vec::<u64>::new(), Vec::<u64>::new(), since);
    loop {
        let mut shown = state.shown.lock().unwrap_or_else(|e| e.into_inner());
        let ended = || state.stop.load(Ordering::Relaxed) || state.retired.load(Ordering::Relaxed);
        while shown.version == seen && !ended() {
            let (next, waited) = state.changed.wait_timeout(shown, KEEP_ALIVE).unwrap_or_else(|e| e.into_inner());
            shown = next;
            if waited.timed_out() {
                break;
            }
        }
        if ended() {
            let _ = stream.write_all(b"event: stopped\ndata: {}\n\n");
            return Ok(());
        }
        if shown.version == seen {
            drop(shown);
            stream.write_all(b": still here\n\n")?;
            continue;
        }
        seen = shown.version;
        let mut out = String::new();
        // Images first, so a board never shows before the page under it.
        images_seen.resize(shown.images.len(), 0);
        for (i, (at, image)) in shown.images.iter().enumerate() {
            if *at > images_seen[i] {
                images_seen[i] = *at;
                out.push_str(&event("image", &json!({ "index": i, "href": image })));
            }
        }
        boards_seen.resize(shown.boards.len(), since);
        for (i, (at, board)) in shown.boards.iter().enumerate() {
            if *at > boards_seen[i] {
                boards_seen[i] = *at;
                out.push_str(&event("board", &json!({ "index": i, "board": board })));
            }
        }
        if shown.lines.0 > lines_seen {
            lines_seen = shown.lines.0;
            out.push_str(&event("lines", &shown.lines.1));
        }
        // Last: what puts the parts together and shows them.
        out.push_str(&event("meeting", &shown.data));
        drop(shown);
        stream.write_all(out.as_bytes())?;
        stream.flush()?;
    }
}

/// One Server-Sent Event. JSON has no raw line breaks, so it is a single data line.
fn event(name: &str, data: &Value) -> String {
    format!("event: {name}\ndata: {data}\n\n")
}

/// Where a viewer without the code is asked for it: a plain form that reloads the page with it.
fn code_page(problem: Option<&str>) -> String {
    let note = problem.map(|p| format!("<p class=wrong>{p}</p>")).unwrap_or_default();
    format!(
        r#"<!doctype html><html><head><meta charset=utf-8><meta name=viewport content="width=device-width,initial-scale=1"><title>Alcyo</title>
<style>body{{font:16px system-ui,sans-serif;display:grid;place-items:center;min-height:100vh;margin:0;background:#fffefa;color:#1e1e1e}}form{{display:grid;gap:12px;padding:24px;max-width:320px}}input{{font:600 24px ui-monospace,monospace;letter-spacing:4px;text-transform:uppercase;padding:8px 12px;border:1px solid #ccc;border-radius:8px;text-align:center}}button{{font:inherit;padding:10px;border:0;border-radius:8px;background:#1e1e1e;color:#fff}}.wrong{{color:#b3261e;margin:0}}p{{margin:0;color:#62665c}}</style></head>
<body><form method=get action="/"><b>Alcyo</b><p>Enter the access code shown on the host's screen. / 输入主持人屏幕上的访问码。</p>{note}<input name=c autocomplete=off autofocus maxlength=12><button>Watch / 观看</button></form></body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(address: &str, target: &str) -> String {
        let address = address.trim_start_matches("http://");
        let mut stream = TcpStream::connect(address).unwrap();
        write!(stream, "GET {target} HTTP/1.1\r\nHost: {address}\r\n\r\n").unwrap();
        let mut out = String::new();
        stream.read_to_string(&mut out).unwrap();
        out
    }

    #[test]
    fn a_real_network_comes_before_a_virtual_one() {
        let home = rank("wlan0", Ipv4Addr::new(192, 168, 1, 20));
        assert!(home < rank("docker0", Ipv4Addr::new(172, 17, 0, 1)));
        assert!(home < rank("eth0", Ipv4Addr::new(10, 0, 0, 5)));
        assert!(rank("en0", Ipv4Addr::new(10, 0, 0, 5)) < rank("utun3", Ipv4Addr::new(192, 168, 2, 1)));
        assert!(!local_address().is_unspecified());
        assert!(!local_networks().is_empty());
        assert_eq!(kind("wlp3s0"), Kind::WiFi);
        assert_eq!(kind("Wi-Fi 2"), Kind::WiFi);
        assert!(rank("wlan0", Ipv4Addr::new(192, 168, 100, 70)) < rank("enp59s0", Ipv4Addr::new(192, 168, 9, 105)), "Wi-Fi first, where phones are");
    }

    #[test]
    fn guessing_is_refused_and_too_much_of_it_ends_the_share() {
        let share = Share::start_on(vec![Network { ip: IpAddr::V4(Ipv4Addr::LOCALHOST), kind: Kind::Unknown, interface: "lo".into() }]).unwrap();
        let device = IpAddr::V4(Ipv4Addr::LOCALHOST);
        for _ in 0..WRONG_PER_DEVICE {
            wrong_code(&share.state, device);
        }
        assert!(refused(&share.state, device), "this device has had its tries");
        assert!(!refused(&share.state, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 9))), "another has not");
        let local = format!("127.0.0.1:{}", share.port);
        assert!(get(&local, &format!("/?c={}", share.code())).starts_with("HTTP/1.1 429"), "even the right code waits");
        assert!(!share.retired());
        for _ in WRONG_PER_DEVICE as usize..WRONG_IN_ALL {
            wrong_code(&share.state, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2)));
        }
        assert!(share.retired(), "the code is given up");
        assert!(get(&local, "/").starts_with("HTTP/1.1 410"));
    }

    #[test]
    fn it_listens_only_where_it_says() {
        let share = Share::start_on(vec![Network { ip: IpAddr::V4(Ipv4Addr::LOCALHOST), kind: Kind::Unknown, interface: "lo".into() }]).unwrap();
        // Another address of this computer, if it has one, is not listened on.
        if let Some(other) = local_networks().into_iter().find(|n| !n.ip.is_loopback()) {
            assert!(TcpStream::connect_timeout(&SocketAddr::new(other.ip, share.port), Duration::from_millis(300)).is_err(), "{}", other.ip);
        }
    }

    #[test]
    fn codes_are_hard_to_guess_and_easy_to_type() {
        let code = new_code().unwrap();
        assert_eq!(code.len(), CODE_LENGTH);
        assert!(code.bytes().all(|b| ALPHABET.contains(&b)), "{code}");
        assert_ne!(code, new_code().unwrap());
        assert!(same_code(&code.to_lowercase(), &code));
        assert!(same_code(&format!("{} {}", &code[..3], &code[3..]), &code));
        assert!(!same_code(&code[..5], &code));
    }

    #[test]
    fn only_the_code_opens_the_meeting_and_only_what_changed_follows() {
        let share = Share::start_on(vec![Network { ip: IpAddr::V4(Ipv4Addr::LOCALHOST), kind: Kind::Unknown, interface: "lo".into() }]).unwrap();
        let meeting = |b: u32, line: &str| json!({ "title": "t", "live": true, "boards": [{ "a": 1 }, { "b": b }], "lines": [line] });
        share.show(r#"<script>let DATA={"live":true}</script>"#.into(), meeting(1, "one"), vec![Some("data:image/png;base64,AA".into()), None]);
        let local = format!("127.0.0.1:{}", share.port);
        assert!(get(&local, "/").contains("access code"), "no code: the form");
        assert!(get(&local, "/?c=WRONG2").starts_with("HTTP/1.1 403"));
        assert!(get(&local, &format!("/?c={}", share.code())).contains(r#""live":1"#), "the page says which version it holds");
        assert!(get(&local, "/events?c=WRONG2").starts_with("HTTP/1.1 403"));

        let read_until = |reader: &mut BufReader<TcpStream>, seen: &mut String, what: &str| {
            while !seen.contains(what) {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0, "stream ended before {what}");
                seen.push_str(&line);
            }
        };
        let watch = |since: u64| {
            let mut stream = TcpStream::connect(&local).unwrap();
            write!(stream, "GET /events?c={}&since={since} HTTP/1.1\r\n\r\n", share.code()).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            BufReader::new(stream)
        };
        // A viewer with nothing gets the image, both boards and the transcript, then the meeting.
        let (mut reader, mut seen) = (watch(0), String::new());
        read_until(&mut reader, &mut seen, r#""title":"t""#);
        assert_eq!(seen.matches("event: board").count(), 2);
        assert!(seen.contains("event: lines") && seen.contains("event: image"));
        assert!(seen.find("event: image").unwrap() < seen.find("event: meeting").unwrap(), "the parts come first");
        assert!(seen.contains(r#""boards":2"#), "the meeting says how many boards, not what is on them");
        // A viewer whose page holds version 1 gets only the image, which no page holds.
        let (mut fresh, mut fresh_seen) = (watch(1), String::new());
        read_until(&mut fresh, &mut fresh_seen, r#""title":"t""#);
        assert!(!fresh_seen.contains("event: board") && !fresh_seen.contains("event: lines") && fresh_seen.contains("event: image"));
        assert_eq!(share.viewers(), 2);

        // One board changes: only it follows, not the other, the transcript or the image.
        share.show("<p>later</p>".into(), meeting(2, "one"), vec![Some("data:image/png;base64,AA".into()), None]);
        let before = seen.len();
        while seen[before..].matches(r#""title":"t""#).count() < 1 {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            seen.push_str(&line);
        }
        let later = &seen[before..];
        assert!(later.contains(r#""b":2"#), "{later}");
        assert_eq!(later.matches("event: board").count(), 1, "{later}");
        assert!(!later.contains("event: lines") && !later.contains("event: image"), "{later}");
        drop(share);
        read_until(&mut reader, &mut seen, "event: stopped");
    }
}
