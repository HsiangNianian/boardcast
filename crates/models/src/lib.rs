//! The board models too large for the installer: what can be downloaded, and downloading it.
//!
//! Sources are tried in order, keeping what has arrived: the same file from any source is verified the
//! same way. Unfinished data waits in `<name>.part`, so a cancelled or broken download resumes. The file
//! only takes its real name after its size and SHA-256 match, so a damaged download is never used.

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Which model to use, in the same file the older app reads.
const SELECTED: &str = "selected.txt";
/// Room left over after a download, so the disk does not end up completely full.
const SPACE_MARGIN: u64 = 1_000_000_000;
const CHUNK: usize = 1 << 20;
const STALL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct Source {
    pub name: String,
    pub url: String,
    /// Skip any system proxy: domestic mirrors are far slower through one.
    pub direct: bool,
}

#[derive(Clone, Debug)]
pub struct Model {
    pub id: &'static str,
    pub label: &'static str,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    /// The VRAM this model suits, in MiB; 0 means it runs anywhere.
    pub vram_mib: u64,
    /// What it falls short on, in English and Chinese. A model with a caveat is never recommended.
    pub note: Option<(&'static str, &'static str)>,
    pub sources: Vec<Source>,
}

fn sources(modelscope: &str, huggingface: &str) -> Vec<Source> {
    vec![
        Source { name: "ModelScope".into(), url: format!("https://modelscope.cn/models/{modelscope}"), direct: true },
        Source { name: "hf-mirror".into(), url: format!("https://hf-mirror.com/{huggingface}"), direct: false },
        Source { name: "Hugging Face".into(), url: format!("https://huggingface.co/{huggingface}"), direct: false },
    ]
}

fn minicpm(id: &'static str, label: &'static str, repo: &str, file: &str, size: u64, sha256: &str, note: (&'static str, &'static str)) -> Model {
    Model {
        id,
        label,
        file: file.to_string(),
        size,
        sha256: sha256.to_string(),
        vram_mib: 0,
        note: Some(note),
        sources: sources(&format!("OpenBMB/{repo}/resolve/master/{file}"), &format!("openbmb/{repo}/resolve/main/{file}")),
    }
}

/// The best boards first, as the first one present is used when none was chosen; among those without a
/// caveat, smallest first. Qwen's files and hashes are the older app's, so a model it downloaded is reused.
/// The notes follow the evaluation in `crates/README.md`.
pub fn catalog() -> Vec<Model> {
    vec![
        Model {
            id: "qwen3.5-4b",
            label: "Qwen3.5-4B",
            file: "Qwen3.5-4B-Q4_K_M.gguf".into(),
            size: 2_740_937_888,
            sha256: "00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4".into(),
            vram_mib: 0,
            note: None,
            sources: sources("unsloth/Qwen3.5-4B-GGUF/resolve/master/Qwen3.5-4B-Q4_K_M.gguf", "unsloth/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf"),
        },
        Model {
            id: "qwen3.5-9b",
            label: "Qwen3.5-9B",
            file: "Qwen3.5-9B-Q4_K_M.gguf".into(),
            size: 5_680_522_464,
            sha256: "03b74727a860a56338e042c4420bb3f04b2fec5734175f4cb9fa853daf52b7e8".into(),
            vram_mib: 6000,
            note: None,
            sources: sources("unsloth/Qwen3.5-9B-GGUF/resolve/master/Qwen3.5-9B-Q4_K_M.gguf", "unsloth/Qwen3.5-9B-GGUF/resolve/main/Qwen3.5-9B-Q4_K_M.gguf"),
        },
        minicpm(
            "minicpm5-2b",
            "MiniCPM5-2B",
            "MiniCPM5-2B-GGUF",
            "MiniCPM5-2B-Q4_K_M.gguf",
            1_561_318_368,
            "ec2d5801640099e97d8d7e8003ad4d81f336e757811f03a26173dddf386602fd",
            ("Smaller and faster, but its boards miss more of what was said.", "更小更快，但板书会漏掉更多内容。"),
        ),
        minicpm(
            "minicpm-v-4.7-1b",
            "MiniCPM-V 4.7 1B",
            "MiniCPM-V-4.7-1B-gguf",
            "MiniCPM-V-4_7-1B-Q4_K_M.gguf",
            672_003_680,
            "f18f594747e5317ab490cff9c06339c9cfb89acbe2f02ebc48a4e167bc23576d",
            ("Draws boards poorly. It can read images with its add-on.", "板书效果较差。配上看图组件可以读图。"),
        ),
        minicpm(
            "minicpm5-1b",
            "MiniCPM5-1B",
            "MiniCPM5-1B-GGUF",
            "MiniCPM5-1B-Q4_K_M.gguf",
            688_065_920,
            "81b64d05a23b17b34c475f42b3e72fbde62d4b92cc34541f7a8031d0752deafa",
            ("Very fast, but draws boards poorly and loses most points.", "速度很快，但板书效果较差，大部分要点会丢失。"),
        ),
    ]
}

/// The largest model the discrete graphics memory can carry; without one, the smallest.
pub fn recommended(vram_mib: u64) -> &'static str {
    let models = catalog();
    models.iter().filter(|m| m.note.is_none()).rfind(|m| m.vram_mib <= vram_mib).unwrap_or(&models[0]).id
}

/// What lets a board model read images, by the board model's file name. It is downloaded the same way
/// and never chosen as a model itself.
pub fn vision_for(model_file: &str) -> Option<Model> {
    let add_on = |id, label, file: &str, size, sha256: &str, source: Vec<Source>| Model {
        id,
        label,
        file: file.to_string(),
        size,
        sha256: sha256.to_string(),
        vram_mib: 0,
        note: None,
        sources: source,
    };
    let unsloth = |repo: &str| sources(&format!("{repo}/resolve/master/mmproj-F16.gguf"), &format!("{repo}/resolve/main/mmproj-F16.gguf"));
    if model_file.starts_with("Qwen3.5-4B") {
        Some(add_on("qwen3.5-4b-vision", "Vision add-on · Qwen3.5-4B", "Qwen3.5-4B-mmproj-F16.gguf", 672_423_616, "cd88edcf8d031894960bb0c9c5b9b7e1fea6ebee02b9f7ce925a00d12891f864", unsloth("unsloth/Qwen3.5-4B-GGUF")))
    } else if model_file.starts_with("Qwen3.5-9B") {
        Some(add_on("qwen3.5-9b-vision", "Vision add-on · Qwen3.5-9B", "Qwen3.5-9B-mmproj-F16.gguf", 918_166_080, "f70dc3509053962b0d0d3ee8a7eacebf5d60aa560cad78254ae8698516ae029f", unsloth("unsloth/Qwen3.5-9B-GGUF")))
    } else if model_file.starts_with("MiniCPM-V-4_7-1B") {
        let source = sources("OpenBMB/MiniCPM-V-4.7-1B-gguf/resolve/master/mmproj-model-f16.gguf", "openbmb/MiniCPM-V-4.7-1B-gguf/resolve/main/mmproj-model-f16.gguf");
        Some(add_on("minicpm-v-4.7-1b-vision", "Vision add-on · MiniCPM-V 4.7 1B", "MiniCPM-V-4_7-1B-mmproj-f16.gguf", 1_110_117_984, "fb9cc047a3cdad5317fd8433a4aeda2d22f00f02ca4bd93b34ad84165b25a900", source))
    } else {
        None
    }
}

/// What finds things in past meetings by what they mean: EmbeddingGemma 300M, never chosen as a board model.
/// Every package carries it (`boardcast-tools fetch search`); a checkout without it downloads it when meeting
/// search is turned on. It is used and shipped under the Gemma Terms of Use (assets/licenses/gemma-*).
pub fn embedding() -> Model {
    let file = "embeddinggemma-300M-Q8_0.gguf";
    let path = format!("ggml-org/embeddinggemma-300M-GGUF/resolve/{{}}/{file}");
    Model {
        id: "embeddinggemma-300m",
        label: "Meeting search · EmbeddingGemma 300M",
        file: file.to_string(),
        size: 333_590_944,
        sha256: "b5ce9d77a3fc4b3b39ccb5643c36777911cc4eb46a66962eadfa3f5f60490d63".to_string(),
        vram_mib: 0,
        note: None,
        sources: sources(&path.replace("{}", "master"), &path.replace("{}", "main")),
    }
}

pub fn installed(model: &Model, dir: &Path) -> bool {
    dir.join(&model.file).is_file()
}

/// The file the user chose, or else the first one present.
pub fn current(dir: &Path) -> Option<PathBuf> {
    let chosen = fs::read_to_string(dir.join(SELECTED)).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if let Some(path) = chosen.map(|file| dir.join(file)).filter(|p| p.is_file()) {
        return Some(path);
    }
    catalog().iter().find(|m| installed(m, dir)).map(|m| dir.join(&m.file))
}

pub fn select(model: &Model, dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    fs::write(dir.join(SELECTED), &model.file).context("saving the chosen model")
}

fn part_of(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

/// Bytes free on the disk holding `dir`.
#[cfg(windows)]
fn free_space(dir: &Path) -> Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
    let mut free = 0_u64;
    // SAFETY: `wide` is a NUL-terminated path and `free` is written by the call.
    let ok = unsafe { windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(free)
}

/// Bytes free on the disk holding `dir`.
#[cfg(unix)]
fn free_space(dir: &Path) -> Result<u64> {
    let path = CString::new(dir.as_os_str().as_bytes())?;
    // SAFETY: `path` is a valid C string and `stat` is written by the call before being read.
    unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(path.as_ptr(), &mut stat) != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(u64::from(stat.f_bavail).saturating_mul(u64::from(stat.f_frsize)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
    /// Bytes per second over the last moments; 0 until known.
    pub speed: u64,
    pub source: Option<String>,
    /// Checking what an earlier attempt left, before asking for the rest.
    pub verifying: bool,
}

#[derive(Debug)]
pub enum Outcome {
    Finished(PathBuf),
    Cancelled,
}

fn agent(direct: bool) -> ureq::Agent {
    ureq::AgentBuilder::new().try_proxy_from_env(!direct).timeout_connect(Duration::from_secs(20)).timeout_read(STALL).build()
}

/// Download a model, resuming anything left by an earlier attempt. `report` runs a few times a second.
pub fn download(model: &Model, dir: &Path, cancel: &AtomicBool, report: &mut dyn FnMut(Progress)) -> Result<Outcome> {
    let path = dir.join(&model.file);
    if path.is_file() {
        return Ok(Outcome::Finished(path));
    }
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let part = part_of(&path);

    // Resuming means hashing what is already there, so a large leftover takes a moment.
    let mut hasher = Sha256::new();
    let mut have = 0_u64;
    if let Ok(mut file) = File::open(&part) {
        report(Progress { done: 0, total: model.size, speed: 0, source: None, verifying: true });
        let mut buffer = vec![0; CHUNK];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Ok(Outcome::Cancelled);
            }
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            have += n as u64;
        }
    }
    if have > model.size {
        fs::remove_file(&part)?;
        hasher = Sha256::new();
        have = 0;
    }
    let needed = (model.size - have) + SPACE_MARGIN;
    let free = free_space(dir)?;
    if needed > free {
        let gb = |n: u64| n as f64 / 1e9;
        bail!("Not enough disk space for {}: it needs {:.1} GB and {:.1} GB is free.", model.label, gb(needed), gb(free));
    }

    let mut failure = anyhow!("all download sources failed");
    let mut buffer = vec![0; CHUNK];
    for source in &model.sources {
        if have >= model.size {
            break;
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(Outcome::Cancelled);
        }
        report(Progress { done: have, total: model.size, speed: 0, source: Some(source.name.clone()), verifying: false });
        let mut request = agent(source.direct).get(&source.url);
        if have > 0 {
            request = request.set("Range", &format!("bytes={have}-"));
        }
        let response = match request.call() {
            Ok(response) => response,
            Err(e) => {
                failure = anyhow!("{}: {e}", source.name);
                continue;
            }
        };
        // A source that ignores the range sends the whole file, so start over from it.
        let append = have > 0 && response.status() == 206;
        if have > 0 && !append {
            hasher = Sha256::new();
            have = 0;
        }
        let mut file = OpenOptions::new().create(true).write(true).append(append).truncate(!append).open(&part).with_context(|| format!("writing {}", part.display()))?;
        let mut reader = response.into_reader();
        let (mut last_report, mut window_start, mut window_bytes, mut speed) = (Instant::now(), Instant::now(), 0_u64, 0_u64);
        loop {
            if cancel.load(Ordering::Relaxed) {
                file.flush()?;
                return Ok(Outcome::Cancelled);
            }
            let n = match reader.read(&mut buffer) {
                Ok(n) => n,
                // Keep what arrived and let the next source continue from there.
                Err(e) => {
                    failure = anyhow!("{}: {e}", source.name);
                    break;
                }
            };
            if n == 0 {
                break;
            }
            file.write_all(&buffer[..n]).with_context(|| format!("writing {}", part.display()))?;
            hasher.update(&buffer[..n]);
            have += n as u64;
            window_bytes += n as u64;
            if window_start.elapsed() >= Duration::from_secs(1) {
                speed = (window_bytes as f64 / window_start.elapsed().as_secs_f64()) as u64;
                window_start = Instant::now();
                window_bytes = 0;
            }
            if last_report.elapsed() >= Duration::from_millis(200) {
                last_report = Instant::now();
                report(Progress { done: have, total: model.size, speed, source: Some(source.name.clone()), verifying: false });
            }
        }
        file.sync_all()?;
    }
    if have < model.size {
        return Err(failure.context(format!("{} stopped at {have} of {} bytes; the next attempt continues from there", model.label, model.size)));
    }
    let actual: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if have != model.size || actual != model.sha256 {
        // Bad data must not be resumed from, or every retry would fail the same way.
        fs::remove_file(&part)?;
        bail!("{} failed verification and was discarded. Please try again.", model.label);
    }
    fs::rename(&part, &path).with_context(|| format!("finishing {}", path.display()))?;
    Ok(Outcome::Finished(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::sync::atomic::AtomicU32;
    use std::sync::{Arc, Mutex};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("boardcast-models-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// How the test server treats a request.
    #[derive(Clone, Copy)]
    enum Behaviour {
        Honest,
        /// Answers with the whole file even when asked for a range.
        IgnoresRange,
        /// Cuts the connection after this many bytes of the body.
        Breaks(usize),
        Missing,
    }

    fn serve(body: Vec<u8>, behaviour: Behaviour) -> (String, Arc<Mutex<Vec<Option<usize>>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model", listener.local_addr().unwrap());
        let ranges = Arc::new(Mutex::new(vec![]));
        let seen = ranges.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                answer(stream, &body, behaviour, &seen);
            }
        });
        (url, ranges)
    }

    fn answer(mut stream: TcpStream, body: &[u8], behaviour: Behaviour, seen: &Mutex<Vec<Option<usize>>>) {
        let mut range = None;
        let reader = BufReader::new(stream.try_clone().unwrap());
        for line in reader.lines().map_while(Result::ok) {
            if line.is_empty() {
                break;
            }
            if let Some(v) = line.to_lowercase().strip_prefix("range: bytes=") {
                range = v.trim_end_matches('-').parse::<usize>().ok();
            }
        }
        seen.lock().unwrap().push(range);
        let (status, slice) = match (behaviour, range) {
            (Behaviour::Missing, _) => ("404 Not Found", &body[..0]),
            (Behaviour::Honest | Behaviour::Breaks(_), Some(from)) => ("206 Partial Content", &body[from..]),
            _ => ("200 OK", body),
        };
        let header = format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", slice.len());
        stream.write_all(header.as_bytes()).unwrap();
        let sent = if let Behaviour::Breaks(n) = behaviour { &slice[..n.min(slice.len())] } else { slice };
        let _ = stream.write_all(sent);
    }

    fn model(body: &[u8], urls: &[&str]) -> Model {
        Model {
            id: "test",
            label: "Test",
            file: "model.gguf".into(),
            size: body.len() as u64,
            sha256: Sha256::digest(body).iter().map(|b| format!("{b:02x}")).collect(),
            vram_mib: 0,
            note: None,
            sources: urls.iter().enumerate().map(|(i, u)| Source { name: format!("source {i}"), url: (*u).to_string(), direct: true }).collect(),
        }
    }

    fn body() -> Vec<u8> {
        (0..3_000_000_u32).map(|i| (i % 251) as u8).collect()
    }

    fn fetch(m: &Model, dir: &Path) -> Result<Outcome> {
        download(m, dir, &AtomicBool::new(false), &mut |_| {})
    }

    #[test]
    fn a_download_arrives_verified_under_its_real_name() {
        let (dir, data) = (Dir::new(), body());
        let (url, _) = serve(data.clone(), Behaviour::Honest);
        let m = model(&data, &[&url]);
        let Outcome::Finished(path) = fetch(&m, &dir.0).unwrap() else { panic!("cancelled") };
        assert_eq!(fs::read(&path).unwrap(), data);
        assert!(!part_of(&path).exists());
        assert!(installed(&m, &dir.0));
    }

    #[test]
    fn a_leftover_resumes_with_a_range_request() {
        let (dir, data) = (Dir::new(), body());
        let m = model(&data, &[]);
        fs::write(part_of(&dir.0.join(&m.file)), &data[..1_000_000]).unwrap();
        let (url, ranges) = serve(data.clone(), Behaviour::Honest);
        let m = model(&data, &[&url]);
        fetch(&m, &dir.0).unwrap();
        assert_eq!(*ranges.lock().unwrap(), [Some(1_000_000)]);
        assert_eq!(fs::read(dir.0.join(&m.file)).unwrap(), data);
    }

    #[test]
    fn a_source_ignoring_the_range_starts_the_file_over() {
        let (dir, data) = (Dir::new(), body());
        let m = model(&data, &[]);
        fs::write(part_of(&dir.0.join(&m.file)), &data[..1_000_000]).unwrap();
        let (url, _) = serve(data.clone(), Behaviour::IgnoresRange);
        let m = model(&data, &[&url]);
        fetch(&m, &dir.0).unwrap();
        assert_eq!(fs::read(dir.0.join(&m.file)).unwrap(), data);
    }

    #[test]
    fn a_broken_source_hands_over_to_the_next_keeping_what_arrived() {
        let (dir, data) = (Dir::new(), body());
        let (bad, _) = serve(data.clone(), Behaviour::Breaks(1_500_000));
        let (good, ranges) = serve(data.clone(), Behaviour::Honest);
        let m = model(&data, &[&bad, &good]);
        fetch(&m, &dir.0).unwrap();
        let asked = ranges.lock().unwrap()[0].unwrap();
        assert!(asked > 0, "the next source must continue, not restart");
        assert_eq!(fs::read(dir.0.join(&m.file)).unwrap(), data);
    }

    #[test]
    fn a_missing_source_is_skipped() {
        let (dir, data) = (Dir::new(), body());
        let (bad, _) = serve(vec![], Behaviour::Missing);
        let (good, _) = serve(data.clone(), Behaviour::Honest);
        fetch(&model(&data, &[&bad, &good]), &dir.0).unwrap();
        assert!(dir.0.join("model.gguf").is_file());
    }

    #[test]
    fn when_every_source_fails_the_progress_is_kept_and_the_error_says_so() {
        let (dir, data) = (Dir::new(), body());
        let (bad, _) = serve(data.clone(), Behaviour::Breaks(1_000_000));
        let m = model(&data, &[&bad]);
        let error = fetch(&m, &dir.0).unwrap_err();
        assert!(format!("{error:#}").contains("continues"), "{error:#}");
        let part = part_of(&dir.0.join(&m.file));
        assert!(fs::metadata(&part).unwrap().len() >= 1_000_000);
        assert!(!dir.0.join(&m.file).exists());
    }

    #[test]
    fn corrupt_data_is_discarded_never_installed() {
        let (dir, data) = (Dir::new(), body());
        let mut wrong = data.clone();
        wrong[100] ^= 0xff;
        let (url, _) = serve(wrong, Behaviour::Honest);
        let m = model(&data, &[&url]);
        assert!(fetch(&m, &dir.0).is_err());
        assert!(!dir.0.join(&m.file).exists());
        assert!(!part_of(&dir.0.join(&m.file)).exists(), "a bad leftover would poison every retry");
    }

    #[test]
    fn cancelling_keeps_what_arrived_for_later() {
        let (dir, data) = (Dir::new(), body());
        let (url, _) = serve(data.clone(), Behaviour::Honest);
        let m = model(&data, &[&url]);
        let cancel = AtomicBool::new(true);
        assert!(matches!(download(&m, &dir.0, &cancel, &mut |_| {}).unwrap(), Outcome::Cancelled));
        assert!(!dir.0.join(&m.file).exists());
        fetch(&m, &dir.0).unwrap();
        assert_eq!(fs::read(dir.0.join(&m.file)).unwrap(), data);
    }

    #[test]
    fn not_enough_disk_space_is_said_before_downloading() {
        let (dir, data) = (Dir::new(), body());
        let (url, ranges) = serve(data.clone(), Behaviour::Honest);
        let mut m = model(&data, &[&url]);
        m.size = u64::MAX / 4;
        let error = fetch(&m, &dir.0).unwrap_err();
        assert!(error.to_string().contains("disk space"), "{error}");
        assert!(ranges.lock().unwrap().is_empty(), "nothing should be fetched");
    }

    #[test]
    fn the_choice_of_model_follows_the_file_then_what_is_present() {
        let dir = Dir::new();
        assert_eq!(current(&dir.0), None);
        let models = catalog();
        fs::write(dir.0.join(&models[1].file), b"x").unwrap();
        assert_eq!(current(&dir.0), Some(dir.0.join(&models[1].file)));
        fs::write(dir.0.join(&models[0].file), b"x").unwrap();
        assert_eq!(current(&dir.0), Some(dir.0.join(&models[0].file)));
        select(&models[1], &dir.0).unwrap();
        assert_eq!(current(&dir.0), Some(dir.0.join(&models[1].file)));
    }

    #[test]
    fn recommendations_follow_graphics_memory() {
        assert_eq!(recommended(0), "qwen3.5-4b");
        assert_eq!(recommended(4096), "qwen3.5-4b");
        assert_eq!(recommended(8192), "qwen3.5-9b");
    }

    #[test]
    fn qwen_stays_the_default_when_a_small_model_is_also_there() {
        let dir = Dir::new();
        let models = catalog();
        let small = models.iter().find(|m| m.id == "minicpm5-1b").unwrap();
        fs::write(dir.0.join(&small.file), b"x").unwrap();
        assert_eq!(current(&dir.0), Some(dir.0.join(&small.file)));
        fs::write(dir.0.join(&models[0].file), b"x").unwrap();
        assert_eq!(current(&dir.0), Some(dir.0.join(&models[0].file)));
        select(small, &dir.0).unwrap();
        assert_eq!(current(&dir.0), Some(dir.0.join(&small.file)));
    }

    #[test]
    fn a_model_with_a_caveat_can_be_chosen_but_is_not_recommended() {
        let models = catalog();
        for id in ["minicpm5-1b", "minicpm5-2b", "minicpm-v-4.7-1b"] {
            assert!(models.iter().find(|m| m.id == id).unwrap().note.is_some(), "{id}");
        }
        for vram in [0, 2048, 4096, 8192, 65536] {
            assert!(recommended(vram).starts_with("qwen"), "{vram}");
        }
    }

    #[test]
    fn the_search_model_is_an_add_on_and_never_a_board_model() {
        let m = embedding();
        assert!(m.sources.iter().any(|s| s.url.starts_with("https://modelscope.cn/models/ggml-org/embeddinggemma-300M-GGUF/resolve/master/")));
        assert!(catalog().iter().all(|c| c.file != m.file));
    }

    #[test]
    fn only_models_that_can_see_have_a_vision_add_on() {
        let has = |id: &str| vision_for(&catalog().into_iter().find(|m| m.id == id).unwrap().file).is_some();
        assert!(has("qwen3.5-4b") && has("qwen3.5-9b") && has("minicpm-v-4.7-1b"));
        assert!(!has("minicpm5-1b") && !has("minicpm5-2b"));
    }

    #[test]
    fn every_download_has_its_own_file_name() {
        let mut names: Vec<String> = catalog().into_iter().flat_map(|m| [vision_for(&m.file).map(|v| v.file), Some(m.file)]).flatten().collect();
        let total = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), total);
    }
}
