//! Fetching a paper from arXiv. Only arXiv IDs and only arxiv.org, with a size limit, so this can
//! never be used as a general downloader.

use anyhow::{bail, Context, Result};
use regex::Regex;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;
use std::time::Duration;

const LIMIT: u64 = 200 * 1024 * 1024;

static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(?:https?://)?(?:[a-z0-9-]+\.)*arxiv\.org/(?:abs|pdf|html|format)/([^?#\s]+)(?:[?#]\S*)?$").expect("valid"));
static PREFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^arxiv:\s*").expect("valid"));
static SUFFIX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\.pdf$").expect("valid"));
static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:([0-9]{4}\.[0-9]{4,5}(?:v[0-9]+)?)|([a-z][a-z-]*(?:\.[A-Z]{2})?/[0-9]{7}(?:v[0-9]+)?))$").expect("valid"));

/// Find an arXiv ID in what someone pasted: a link, "arXiv:ID", or the ID alone.
pub fn parse(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    let body = match URL.captures(s) {
        Some(c) => c[1].to_string(),
        None => PREFIX.replace(s, "").into_owned(),
    };
    let body = SUFFIX.replace(&body, "");
    let body = body.strip_suffix('/').unwrap_or(&body);
    let m = ID.captures(body)?;
    m.get(1).or(m.get(2)).map(|x| x.as_str().to_string())
}

fn on_arxiv(url: &str) -> bool {
    let host = url.split("//").nth(1).and_then(|r| r.split(['/', ':', '?', '#']).next()).unwrap_or_default();
    host == "arxiv.org" || host.ends_with(".arxiv.org")
}

pub fn download(id: &str, cancel: &AtomicBool) -> Result<Vec<u8>> {
    if !ID.is_match(id) {
        bail!("Invalid arXiv ID: {id}");
    }
    let agent = ureq::AgentBuilder::new().try_proxy_from_env(true).timeout_connect(Duration::from_secs(15)).timeout(Duration::from_secs(120)).user_agent("boardcast (https://github.com/HsiangNianian/boardcast)").build();
    let response = match agent.get(&format!("https://arxiv.org/pdf/{id}")).call() {
        Ok(response) => response,
        Err(ureq::Error::Status(code, _)) => bail!("arXiv returned {code}: the ID may be incorrect"),
        Err(e) => return Err(e).context("Can't connect to arXiv"),
    };
    // Still on arxiv.org after any redirects.
    if !on_arxiv(response.get_url()) {
        bail!("arXiv redirected to another site, so the download was cancelled");
    }
    let mut reader = response.into_reader().take(LIMIT + 1);
    let mut bytes = vec![];
    let mut chunk = [0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            bail!("cancelled");
        }
        let n = reader.read(&mut chunk).context("Can't download from arXiv")?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
    }
    if bytes.len() as u64 > LIMIT {
        bail!("This paper is too large to import");
    }
    if !bytes.starts_with(b"%PDF") {
        bail!("arXiv did not return a PDF for {id}");
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn ids_are_read_as_the_typescript_engine_reads_them() {
        let goldens: Value = serde_json::from_str(include_str!("../../board/fixtures/goldens.json")).unwrap();
        let cases = goldens["arxiv"].as_array().unwrap();
        assert!(cases.len() >= 10);
        for case in cases {
            assert_eq!(parse(case["input"].as_str().unwrap()).as_deref(), case["id"].as_str(), "{}", case["input"]);
        }
    }

    #[test]
    fn only_arxiv_hosts_are_accepted() {
        assert!(on_arxiv("https://arxiv.org/pdf/2401.12345"));
        assert!(on_arxiv("https://export.arxiv.org/pdf/2401.12345"));
        assert!(!on_arxiv("https://arxiv.org.evil.example/pdf/1"));
        assert!(!on_arxiv("https://evil.example/arxiv.org/pdf/1"));
    }

    #[test]
    fn a_bad_id_never_reaches_the_network() {
        assert!(download("../../etc/passwd", &AtomicBool::new(false)).is_err());
    }
}
