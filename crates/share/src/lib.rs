//! A meeting shown to others on the same network: a small read-only web server the desktop app starts by
//! hand. A viewer opens its address in a browser, gives the access code (the QR code carries it), and
//! watches the board grow as the meeting goes on.
//!
//! The page is one HTML file; the meeting comes after it as Server-Sent Events on `/events`: the whole of it
//! first, then again on each change. A board's page image is sent once to each viewer, not with every change.
//! Nothing is read from viewers but the address they ask for.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Ports tried first, so the address stays the same from one meeting to the next when it can.
const PORTS: std::ops::RangeInclusive<u16> = 4747..=4757;
/// The code's letters: no 0/O, 1/I/L, so it reads aloud and types without mistakes.
const ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";
const CODE_LENGTH: usize = 6;
/// How long a wrong code waits before it is answered, which makes guessing one too slow to be worth it.
const WRONG_CODE_DELAY: Duration = Duration::from_secs(1);
/// However quiet the meeting, a stream says something this often, so a viewer gone away is noticed.
const KEEP_ALIVE: Duration = Duration::from_secs(15);

/// What viewers are shown now.
#[derive(Default)]
struct Shown {
    /// Bumped on every change, so each viewer's stream knows when there is something new.
    version: u64,
    /// The page, with the meeting as it is now inside it, for a viewer arriving.
    page: String,
    /// The meeting without its page images.
    data: Value,
    /// Each board's page image as a data URL, with the version it last changed at.
    images: Vec<(u64, Option<String>)>,
}

struct State {
    code: String,
    shown: Mutex<Shown>,
    changed: Condvar,
    stop: AtomicBool,
    viewers: AtomicUsize,
}

/// A running share; it stops when dropped.
pub struct Share {
    state: Arc<State>,
    address: SocketAddr,
    /// The other addresses it can be reached at, on other networks this computer is on.
    others: Vec<IpAddr>,
}

impl Share {
    /// Start listening on the network, with a new access code.
    pub fn start() -> Result<Self> {
        let listener = PORTS
            .clone()
            .find_map(|port| TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).ok())
            .map_or_else(|| TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)), Ok)
            .context("listening on the network")?;
        let port = listener.local_addr()?.port();
        let mut addresses = local_addresses();
        let address = SocketAddr::new(addresses.remove(0), port);
        let state = Arc::new(State { code: new_code()?, shown: Mutex::default(), changed: Condvar::new(), stop: AtomicBool::new(false), viewers: AtomicUsize::new(0) });
        let accepting = state.clone();
        std::thread::Builder::new().name("share".into()).spawn(move || {
            for stream in listener.incoming() {
                if accepting.stop.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(stream) = stream else { continue };
                let state = accepting.clone();
                let _ = std::thread::Builder::new().name("share-viewer".into()).spawn(move || {
                    let _ = serve(stream, &state);
                });
            }
        })?;
        Ok(Self { state, address, others: addresses })
    }

    /// The access code a viewer gives.
    pub fn code(&self) -> &str {
        &self.state.code
    }

    /// The addresses to open, without the code, the likeliest first.
    pub fn addresses(&self) -> Vec<String> {
        std::iter::once(self.address.ip()).chain(self.others.iter().copied()).map(|ip| format!("http://{}", SocketAddr::new(ip, self.address.port()))).collect()
    }

    /// An address with the code in it, for the QR code: opening it needs nothing typed.
    pub fn link(&self, address: &str) -> String {
        format!("{address}/?c={}", self.state.code)
    }

    /// How many are watching now.
    pub fn viewers(&self) -> usize {
        self.state.viewers.load(Ordering::Relaxed)
    }

    /// Show this from now on: the page a viewer arriving gets, the meeting for the stream, and each board's
    /// page image as a data URL.
    pub fn show(&self, page: String, data: Value, images: Vec<Option<String>>) {
        let mut shown = self.state.shown.lock().unwrap_or_else(|e| e.into_inner());
        shown.version += 1;
        let version = shown.version;
        let before = std::mem::take(&mut shown.images);
        shown.images = images
            .into_iter()
            .enumerate()
            .map(|(i, image)| match before.get(i) {
                Some((at, same)) if *same == image => (*at, image),
                _ => (version, image),
            })
            .collect();
        shown.page = page;
        shown.data = data;
        drop(shown);
        self.state.changed.notify_all();
    }
}

impl Drop for Share {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::Relaxed);
        self.state.changed.notify_all();
        // The listener is waiting for a connection; one from here lets it see that it should stop.
        let _ = TcpStream::connect_timeout(&SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), self.address.port()), Duration::from_millis(200));
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
fn local_addresses() -> Vec<IpAddr> {
    let mut lan: Vec<((u8, u8), Ipv4Addr)> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            IpAddr::V4(ip) if ip.is_private() => Some((rank(&i.name, ip), ip)),
            _ => None,
        })
        .collect();
    lan.sort();
    lan.dedup_by_key(|(_, ip)| *ip);
    // A virtual network only when there is no real one: a container's bridge is not where a phone is.
    if lan.iter().any(|((virtual_, _), _)| *virtual_ == 0) {
        lan.retain(|((virtual_, _), _)| *virtual_ == 0);
    }
    let mut out: Vec<IpAddr> = lan.into_iter().map(|(_, ip)| IpAddr::V4(ip)).collect();
    if out.is_empty() {
        out.push(local_address());
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

/// Which private address to show first: a home or office network's usual ranges before the rest, and a
/// physical interface before a virtual one (containers, virtual machines, tunnels).
fn rank(interface: &str, ip: Ipv4Addr) -> (u8, u8) {
    let [a, b, ..] = ip.octets();
    let range = match (a, b) {
        (192, 168) => 0,
        (10, _) => 1,
        _ => 2,
    };
    let name = interface.to_ascii_lowercase();
    let virtual_ = ["docker", "br-", "veth", "virbr", "vmnet", "vboxnet", "tun", "tap", "utun", "wg", "zt", "tailscale", "vethernet", "flclash", "clash", "mihomo"].iter().any(|v| name.starts_with(v));
    (u8::from(virtual_), range)
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
    let given = query_value(&query, "c");
    let allowed = given.is_some_and(|c| same_code(c, &state.code));
    if given.is_some() && !allowed {
        std::thread::sleep(WRONG_CODE_DELAY);
    }
    match path.as_str() {
        "/" if allowed => {
            let page = state.shown.lock().unwrap_or_else(|e| e.into_inner()).page.clone();
            respond(&mut stream, "200 OK", "text/html; charset=utf-8", &page)
        }
        "/" => respond(&mut stream, if given.is_some() { "403 Forbidden" } else { "200 OK" }, "text/html; charset=utf-8", &code_page(given.is_some())),
        "/events" if allowed => events(stream, state),
        "/events" => respond(&mut stream, "403 Forbidden", "text/plain", "wrong code"),
        _ => respond(&mut stream, "404 Not Found", "text/plain", "not found"),
    }
}

/// The meeting as it changes, until the viewer leaves or sharing stops.
fn events(mut stream: TcpStream, state: &State) -> Result<()> {
    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nX-Accel-Buffering: no\r\nConnection: keep-alive\r\n\r\nretry: 2000\n\n")?;
    state.viewers.fetch_add(1, Ordering::Relaxed);
    let result = stream_changes(&mut stream, state);
    state.viewers.fetch_sub(1, Ordering::Relaxed);
    result
}

fn stream_changes(stream: &mut TcpStream, state: &State) -> Result<()> {
    let (mut seen, mut images_seen) = (0u64, Vec::<u64>::new());
    loop {
        let mut shown = state.shown.lock().unwrap_or_else(|e| e.into_inner());
        while shown.version == seen && !state.stop.load(Ordering::Relaxed) {
            let (next, waited) = state.changed.wait_timeout(shown, KEEP_ALIVE).unwrap_or_else(|e| e.into_inner());
            shown = next;
            if waited.timed_out() {
                break;
            }
        }
        if state.stop.load(Ordering::Relaxed) {
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
fn code_page(wrong: bool) -> String {
    let note = if wrong { "<p class=wrong>That code is not right. / 访问码不对。</p>" } else { "" };
    format!(
        r#"<!doctype html><html><head><meta charset=utf-8><meta name=viewport content="width=device-width,initial-scale=1"><title>boardcast</title>
<style>body{{font:16px system-ui,sans-serif;display:grid;place-items:center;min-height:100vh;margin:0;background:#fffefa;color:#1e1e1e}}form{{display:grid;gap:12px;padding:24px;max-width:320px}}input{{font:600 24px ui-monospace,monospace;letter-spacing:4px;text-transform:uppercase;padding:8px 12px;border:1px solid #ccc;border-radius:8px;text-align:center}}button{{font:inherit;padding:10px;border:0;border-radius:8px;background:#1e1e1e;color:#fff}}.wrong{{color:#b3261e;margin:0}}p{{margin:0;color:#62665c}}</style></head>
<body><form method=get action="/"><b>boardcast</b><p>Enter the access code shown on the host's screen. / 输入主持人屏幕上的访问码。</p>{note}<input name=c autocomplete=off autofocus maxlength=12><button>Watch / 观看</button></form></body></html>"#
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
        assert!(!local_addresses().is_empty());
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
    fn only_the_code_opens_the_meeting_and_its_changes_follow() {
        let share = Share::start().unwrap();
        share.show("<p>the meeting</p>".into(), json!({ "lines": 1 }), vec![Some("data:image/png;base64,AA".into())]);
        let local = format!("127.0.0.1:{}", share.address.port());
        assert!(get(&local, "/").contains("access code"), "no code: the form");
        assert!(get(&local, "/?c=WRONG2").starts_with("HTTP/1.1 403"));
        assert!(get(&local, &format!("/?c={}", share.code())).contains("the meeting"));
        assert!(get(&local, "/events?c=WRONG2").starts_with("HTTP/1.1 403"));

        // A viewer gets the image and the meeting, then each change.
        let mut stream = TcpStream::connect(&local).unwrap();
        write!(stream, "GET /events?c={} HTTP/1.1\r\n\r\n", share.code()).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut reader = BufReader::new(stream);
        let mut seen = String::new();
        let read_until = |reader: &mut BufReader<TcpStream>, seen: &mut String, what: &str| {
            while !seen.contains(what) {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0, "stream ended before {what}");
                seen.push_str(&line);
            }
        };
        read_until(&mut reader, &mut seen, "\"lines\":1");
        assert!(seen.find("event: image").unwrap() < seen.find("event: meeting").unwrap(), "the image comes first");
        assert_eq!(share.viewers(), 1);
        share.show("<p>later</p>".into(), json!({ "lines": 2 }), vec![Some("data:image/png;base64,AA".into())]);
        read_until(&mut reader, &mut seen, "\"lines\":2");
        assert_eq!(seen.matches("event: image").count(), 1, "an image that did not change is not sent again");
        drop(share);
        read_until(&mut reader, &mut seen, "event: stopped");
    }
}
