//! How the app reaches the internet for model downloads and remote models: through the system's proxy, a proxy
//! the user gave, or none. Nothing local ever goes through a proxy: the app's own model server and any service
//! on this computer are reached directly, whatever is chosen.

use std::sync::RwLock;
use std::time::Duration;

/// The user's choice in Settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Proxy {
    /// The proxy the system or the environment names (HTTP_PROXY, HTTPS_PROXY, ALL_PROXY), if any.
    #[default]
    System,
    /// A proxy the user gave: http://host:port, or socks5://host:port.
    Custom(String),
    /// Straight to the internet, whatever the system says.
    Off,
}

impl Proxy {
    /// From the settings file's words: "system", "custom" with an address, or "off".
    pub fn from_setting(choice: &str, address: &str) -> Self {
        match choice {
            "off" => Self::Off,
            "custom" if !address.trim().is_empty() => Self::Custom(address.trim().to_string()),
            _ => Self::System,
        }
    }

    /// Whether an address can be used as a proxy, and why not if it cannot.
    pub fn check(address: &str) -> Result<(), String> {
        let address = address.trim();
        let known = ["http://", "https://", "socks5://", "socks5h://", "socks4://"];
        if !known.iter().any(|s| address.starts_with(s)) {
            return Err("no scheme".into());
        }
        ureq::Proxy::new(address).map(|_| ()).map_err(|e| e.to_string())
    }
}

/// The address with a username and password put in, as `scheme://user:password@host:port`, the form proxies
/// take them in. Both are optional: with neither, the address is left as it is, so one typed with its login
/// still works. A username cannot hold a colon, which would end it early.
pub fn with_login(address: &str, user: &str, password: &str) -> Result<String, String> {
    let (address, user) = (address.trim(), user.trim());
    if user.is_empty() && password.is_empty() {
        return Ok(address.to_string());
    }
    if user.contains(':') {
        return Err("colon in username".into());
    }
    let (scheme, rest) = address.split_once("://").unwrap_or(("", address));
    // A login already in the address gives way to the one in the fields.
    let host = rest.rsplit_once('@').map_or(rest, |(_, host)| host);
    // Always both halves, an empty one included: a login without its colon is not taken.
    let login = format!("{user}:{password}");
    Ok(if scheme.is_empty() { format!("{login}@{host}") } else { format!("{scheme}://{login}@{host}") })
}

static CHOSEN: RwLock<Option<Proxy>> = RwLock::new(None);

/// Take the user's choice; every client made after this follows it.
pub fn set(proxy: Proxy) {
    *CHOSEN.write().expect("proxy lock") = Some(proxy);
}

pub fn chosen() -> Proxy {
    CHOSEN.read().expect("proxy lock").clone().unwrap_or_default()
}

/// Whether a URL is on this computer, which is never reached through a proxy.
pub fn is_local(url: &str) -> bool {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    // [::1]:8080, 127.0.0.1:18765, localhost:8080
    let host = if host.starts_with('[') { host.split(']').next().unwrap_or_default().trim_start_matches('[') } else { host.rsplit_once(':').map_or(host, |(h, _)| h) };
    host.eq_ignore_ascii_case("localhost") || host == "::1" || host.starts_with("127.") || host == "0.0.0.0"
}

/// A client for `url`. `direct_by_default` is for a source that is faster without the system's proxy, such as
/// a mirror in China: it skips the system's proxy, but a proxy the user gave still applies.
pub fn agent_for(url: &str, direct_by_default: bool, timeout_connect: Duration) -> ureq::AgentBuilder {
    let builder = ureq::AgentBuilder::new().timeout_connect(timeout_connect).try_proxy_from_env(false);
    if is_local(url) {
        return builder;
    }
    match chosen() {
        Proxy::System if !direct_by_default => builder.try_proxy_from_env(true),
        Proxy::System | Proxy::Off => builder,
        Proxy::Custom(address) => match ureq::Proxy::new(&address) {
            Ok(proxy) => builder.proxy(proxy),
            // Settings refuses an address that does not parse, so this is a file edited by hand: go direct.
            Err(_) => builder,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_addresses_are_told_apart() {
        for url in ["http://127.0.0.1:18765/v1/chat/completions", "http://localhost:8080", "http://[::1]:9000/v1", "http://127.1.2.3"] {
            assert!(is_local(url), "{url}");
        }
        for url in ["https://api.deepseek.com", "https://huggingface.co/x", "http://10.0.0.5:8080", "https://localhost.example.com"] {
            assert!(!is_local(url), "{url}");
        }
    }

    #[test]
    fn a_proxy_address_is_checked_and_the_setting_read() {
        assert!(Proxy::check("http://127.0.0.1:7890").is_ok());
        assert!(Proxy::check("socks5://127.0.0.1:7891").is_ok());
        assert!(Proxy::check("127.0.0.1:7890").is_err(), "the scheme is needed");
        assert_eq!(Proxy::from_setting("custom", " http://p:1 "), Proxy::Custom("http://p:1".into()));
        assert_eq!(Proxy::from_setting("custom", ""), Proxy::System, "no address, no custom proxy");
        assert_eq!(Proxy::from_setting("off", "http://p:1"), Proxy::Off);
        assert_eq!(Proxy::from_setting("", ""), Proxy::System);
    }

    #[test]
    fn a_login_goes_into_the_address() {
        assert_eq!(with_login("http://127.0.0.1:7890", "", "").unwrap(), "http://127.0.0.1:7890", "neither, no login");
        assert_eq!(with_login("http://user:pw@127.0.0.1:7890", "", "").unwrap(), "http://user:pw@127.0.0.1:7890", "a login typed in the address stays");
        assert_eq!(with_login("http://127.0.0.1:7890", "", "x").unwrap(), "http://:x@127.0.0.1:7890", "a password alone");
        assert_eq!(with_login("http://127.0.0.1:7890", "me", "p@ss:w").unwrap(), "http://me:p@ss:w@127.0.0.1:7890");
        assert_eq!(with_login("socks5://old:pw@proxy:1080", "me", "").unwrap(), "socks5://me:@proxy:1080", "the fields replace a login in the address");
        for (user, password) in [("me", ""), ("", "x"), ("me", "p@ss:w")] {
            assert!(ureq::Proxy::new(with_login("http://proxy.example:8080", user, password).unwrap()).is_ok(), "{user} {password}");
        }
        assert!(with_login("http://p:1", "a:b", "c").is_err());
    }

    #[test]
    fn the_proxy_is_asked_with_the_login() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let heard = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut lines = vec![];
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
                lines.push(line.trim_end().to_string());
            }
            (&stream).write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n").unwrap();
            lines
        });
        let proxy = ureq::Proxy::new(with_login(&address, "me", "p@ss:w").unwrap()).unwrap();
        let answer = ureq::AgentBuilder::new().proxy(proxy).build().get("https://models.example/x").call();
        assert!(answer.is_err(), "the proxy refused");
        let lines = heard.join().unwrap();
        // base64 of "me:p@ss:w"
        assert!(lines.iter().any(|l| l.eq_ignore_ascii_case("proxy-authorization: basic bWU6cEBzczp3")), "{lines:?}");
    }
}
