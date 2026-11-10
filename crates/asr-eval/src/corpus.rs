//! The recordings to evaluate on, from eval/asr/corpus.json and eval/asr/meetings.json: checking the manifest,
//! downloading and verifying each recording, and a listening page to hear them.

use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Deserialize)]
pub struct Corpus {
    pub version: String,
    #[serde(rename = "baseUrl", default)]
    pub base_url: String,
    pub sources: BTreeMap<String, Source>,
    pub samples: Vec<Sample>,
}

#[derive(Deserialize)]
pub struct Source {
    pub name: String,
    pub url: String,
    pub license: String,
    #[serde(rename = "licenseUrl")]
    pub license_url: String,
}

#[derive(Deserialize)]
pub struct Sample {
    pub id: String,
    #[serde(default)]
    pub title: String,
    pub file: String,
    pub language: String,
    pub source: String,
    pub duration: f64,
    pub bytes: u64,
    pub sha256: String,
    #[serde(default)]
    pub quick: bool,
    pub scoring: String,
    pub reference: Vec<Span>,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub industry: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    /// For recordings kept at their upstream host: where the longer original is, and where this clip starts.
    #[serde(default)]
    pub original: Option<Original>,
}

#[derive(Deserialize)]
pub struct Span {
    pub start: f64,
    pub end: f64,
    #[serde(default)]
    pub speaker: String,
    pub text: String,
}

#[derive(Deserialize)]
pub struct Original {
    pub url: String,
    pub offset: f64,
    pub sha256: String,
    pub bytes: u64,
}

impl Sample {
    /// The reference text in reading order.
    pub fn reference_text(&self) -> String {
        self.reference.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join(" ")
    }

    pub fn scored(&self) -> bool {
        matches!(self.scoring.as_str(), "reference" | "ordered" | "provisional")
    }
}

/// `quick` is the corpus' quick samples, `full` all of it, `meetings` the long meeting clips.
pub fn load(root: &Path, suite: &str) -> Result<Corpus> {
    let file = match suite {
        "quick" | "full" => "corpus.json",
        "meetings" => "meetings.json",
        _ => bail!("the suite is one of quick, full and meetings"),
    };
    let mut corpus: Corpus = serde_json::from_slice(&fs::read(root.join("eval/asr").join(file))?).with_context(|| format!("reading eval/asr/{file}"))?;
    validate(&corpus)?;
    if suite == "quick" {
        corpus.samples.retain(|s| s.quick);
    }
    Ok(corpus)
}

pub fn validate(corpus: &Corpus) -> Result<()> {
    ensure!(!corpus.version.is_empty() && !corpus.samples.is_empty(), "empty corpus");
    let mut ids = std::collections::HashSet::new();
    for s in &corpus.samples {
        ensure!(ids.insert(&s.id), "duplicate sample {}", s.id);
        ensure!(!s.id.is_empty() && s.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'), "invalid sample id {}", s.id);
        ensure!(s.file == format!("{}.wav", s.id) && corpus.sources.contains_key(&s.source), "invalid sample {}", s.id);
        ensure!(["zh", "ja", "en", "zh-en", "silence"].contains(&s.language.as_str()) && s.duration.is_finite() && s.duration > 0.0, "invalid language or duration: {}", s.id);
        ensure!(["reference", "ordered", "provisional", "unscored", "silence"].contains(&s.scoring.as_str()), "missing reference policy: {}", s.id);
        ensure!(s.sha256.len() == 64 && s.bytes > 0, "invalid checksum or size: {}", s.id);
        if let Some(o) = &s.original {
            ensure!(o.url.starts_with("https://") && o.offset.is_finite() && o.offset >= 0.0, "invalid original audio: {}", s.id);
        }
        for r in &s.reference {
            ensure!(r.start >= 0.0 && r.start <= r.end && r.end <= s.duration + 0.001, "invalid reference span: {}", s.id);
        }
        ensure!(!s.scored() || s.reference.iter().any(|r| !r.text.trim().is_empty()), "empty reference: {}", s.id);
    }
    Ok(())
}

pub fn sha256_of(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buf = vec![0; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn intact(path: &Path, bytes: u64, sha256: &str) -> bool {
    fs::metadata(path).is_ok_and(|m| m.len() == bytes) && sha256_of(path).is_ok_and(|h| h == sha256)
}

/// Fetch a file unless an intact copy is there; a cached copy is checked too, since a cache hit says nothing
/// of its contents. A partial or wrong download is removed rather than left to pass as the file.
pub fn download(url: &str, bytes: u64, sha256: &str, destination: &Path) -> Result<()> {
    if intact(destination, bytes, sha256) {
        return Ok(());
    }
    fs::create_dir_all(destination.parent().context("destination")?)?;
    let partial = destination.with_extension(format!("{}.part", std::process::id()));
    let result = (|| -> Result<()> {
        eprintln!("download {url}");
        let reply = ureq::get(url).timeout(std::time::Duration::from_secs(900)).call()?;
        std::io::copy(&mut reply.into_reader(), &mut fs::File::create(&partial)?)?;
        ensure!(intact(&partial, bytes, sha256), "size or SHA-256 mismatch: {url}");
        fs::rename(&partial, destination)?;
        Ok(())
    })();
    let _ = fs::remove_file(&partial);
    result
}

/// Where the recordings are kept, outside version control.
pub fn audio_dir(root: &Path) -> PathBuf {
    root.join("eval/asr/.data")
}

/// Make every recording of the corpus present and intact, and write the listening page beside them.
pub fn prepare(root: &Path, corpus: &Corpus) -> Result<()> {
    let dir = audio_dir(root);
    for s in &corpus.samples {
        let path = dir.join(&s.file);
        let Some(original) = &s.original else {
            let url = s.url.clone().unwrap_or_else(|| format!("{}/{}", corpus.base_url, s.file));
            download(&url, s.bytes, &s.sha256, &path)?;
            continue;
        };
        // Recordings whose licence allows only the text are cut from their upstream original.
        if intact(&path, s.bytes, &s.sha256) {
            continue;
        }
        let source = dir.join("sources").join(format!("{}.audio", s.id));
        download(&original.url, original.bytes, &original.sha256, &source)?;
        let partial = path.with_extension(format!("{}.part.wav", std::process::id()));
        let status = Command::new("ffmpeg")
            .args(["-nostdin", "-v", "error", "-y", "-fflags", "+bitexact", "-ss", &original.offset.to_string(), "-i"])
            .arg(&source)
            .args(["-t", &s.duration.to_string(), "-map_metadata", "-1", "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", "-flags:a", "+bitexact", "-fflags", "+bitexact"])
            .arg(&partial)
            .status()
            .context("running ffmpeg")?;
        let ok = status.success() && intact(&partial, s.bytes, &s.sha256);
        if ok {
            fs::rename(&partial, &path)?;
        }
        let _ = fs::remove_file(&partial);
        ensure!(ok, "converted audio does not match its SHA-256: {}; check the documented ffmpeg version", s.id);
    }
    fs::write(dir.join("index.html"), listening_page(corpus))?;
    fs::copy(root.join("eval/asr/LICENSES.md"), dir.join("LICENSES.md"))?;
    println!("Audio ready: {}", dir.join("index.html").display());
    Ok(())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// A page to hear each recording beside its reference, with nothing but HTML: open it from the folder.
pub fn listening_page(corpus: &Corpus) -> String {
    let cards: String = corpus
        .samples
        .iter()
        .map(|s| {
            let source = &corpus.sources[&s.source];
            let text: Vec<String> = s.reference.iter().map(|r| format!("[{:.2}–{:.2}] {}: {}", r.start, r.end, r.speaker, r.text)).collect();
            let text = if text.is_empty() { "(silence)".to_string() } else { text.join("\n") };
            let industry = s.industry.as_ref().map_or_else(String::new, |i| format!(" · {} · {}", escape(i), escape(s.kind.as_deref().unwrap_or_default())));
            format!(
                "<article><h2>{title}</h2><p>{lang} · {min:.1} min · {scoring}{quick}{industry}</p>\n<audio controls preload=\"none\" src=\"{file}\" aria-label=\"{title}\"></audio>\n<p><a href=\"{file}\" download>Download WAV</a> · <a href=\"{src}\">{name}</a> · <a href=\"{lic_url}\">{lic}</a></p>\n<p>{note}</p><details><summary>Reference transcript</summary><pre>{text}</pre></details></article>",
                title = escape(&s.title),
                lang = escape(&s.language),
                min = s.duration / 60.0,
                scoring = escape(&s.scoring),
                quick = if s.quick { " · quick" } else { "" },
                file = escape(&s.file),
                src = escape(&source.url),
                name = escape(&source.name),
                lic_url = escape(&source.license_url),
                lic = escape(&source.license),
                note = escape(&s.note),
                text = escape(&text),
            )
        })
        .collect();
    format!(
        "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>boardcast · ASR audio</title>\n<style>body{{font:16px/1.6 system-ui,sans-serif;max-width:1080px;margin:40px auto;padding:0 24px;background:#f6f5f0;color:#242824}}h1{{font-size:30px}}h2{{font-size:18px;margin:0}}main{{display:grid;grid-template-columns:repeat(auto-fit,minmax(300px,1fr));gap:16px}}article{{padding:20px;background:white;border:1px solid #deded6;border-radius:12px}}article p{{font-size:14px;color:#62665f}}audio{{width:100%}}a{{color:#176251}}pre{{white-space:pre-wrap;overflow-wrap:anywhere;font:14px/1.7 system-ui}}summary{{cursor:pointer}}</style>\n<h1>Chinese / Japanese / English · ASR listening samples</h1><p>{version} · 16 kHz / mono / PCM16 · the audio is downloaded locally.</p>\n<p>reference: the publisher's verbatim transcript. ordered: overlapping speech joined by start time, a diagnostic score. provisional: edited minutes or text not yet checked. unscored: text shown only. Sources and licences are in <a href=\"LICENSES.md\">LICENSES.md</a>.</p><main>{cards}</main></html>",
        version = escape(&corpus.version),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    }

    #[test]
    fn the_manifests_are_valid_and_cover_each_language() {
        let quick = load(&root(), "quick").unwrap();
        for language in ["zh", "ja", "en", "zh-en"] {
            assert!(quick.samples.iter().any(|s| s.language == language), "{language}");
        }
        let full = load(&root(), "full").unwrap();
        assert!(full.samples.len() > quick.samples.len());
        let meetings = load(&root(), "meetings").unwrap();
        // Continuous clips of about ten minutes each, beside a short silence control.
        let speech: Vec<&Sample> = meetings.samples.iter().filter(|s| s.scoring != "silence").collect();
        assert!(speech.iter().all(|s| s.duration >= 540.0 && s.duration <= 660.0));
        for language in ["zh", "ja", "en"] {
            assert!(speech.iter().any(|s| s.language == language), "{language}");
        }
        assert!(load(&root(), "unknown").is_err());
    }

    #[test]
    fn invalid_samples_are_rejected() {
        let text = std::fs::read_to_string(root().join("eval/asr/corpus.json")).unwrap();
        for (from, to) in [(r#""file": "zh-"#, r#""file": "../zh-"#), (r#""scoring": "reference""#, r#""scoring": "maybe""#)] {
            let changed = text.replacen(from, to, 1);
            assert_ne!(changed, text, "{from}");
            let corpus: Corpus = serde_json::from_str(&changed).unwrap();
            assert!(validate(&corpus).is_err(), "{from}");
        }
    }

    #[test]
    fn the_listening_page_needs_no_script_and_escapes_transcripts() {
        let mut corpus = load(&root(), "full").unwrap();
        corpus.samples[0].note = "<script>alert(1)</script>".into();
        let html = listening_page(&corpus);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert_eq!(html.matches("<audio ").count(), corpus.samples.len());
        assert!(html.contains("preload=\"none\""));
    }

    #[test]
    fn a_corrupt_download_is_removed_and_a_good_cache_kept() {
        let dir = std::env::temp_dir().join(format!("asr-eval-download-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("a.wav");
        fs::write(&target, b"hello").unwrap();
        let sha = sha256_of(&target).unwrap();
        // Intact: no network is touched.
        download("http://127.0.0.1:9/never", 5, &sha, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"hello");
        fs::write(&target, b"hellp").unwrap();
        assert!(download("http://127.0.0.1:9/never", 5, &sha, &target).is_err());
        assert!(!dir.read_dir().unwrap().any(|e| e.unwrap().file_name().to_string_lossy().contains(".part")));
        let _ = fs::remove_dir_all(dir);
    }
}
