//! What a checkout needs before it builds or packages, downloaded and checked:
//!   fetch voice                 the speech models, into resources/voice (see voice_files.rs)
//!   fetch pdfium [platform]     a pinned PDFium build, into target/pdfium
//!   fetch llama [platform]      llama.cpp's prebuilt llama-server, trimmed, into resources/llama
//!   fetch glyphs                stroke data for Chinese and Japanese characters, into assets/hanzi
//! Each is skipped when what is there already matches.

use crate::voice_files;
use anyhow::{bail, ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn run(mut args: impl Iterator<Item = String>) -> Result<()> {
    let what = args.next();
    let platform = args.next();
    match what.as_deref() {
        Some("voice") => voice(),
        Some("pdfium") => pdfium(platform.as_deref()),
        Some("llama") => llama(platform.as_deref()),
        Some("glyphs") => glyphs(),
        _ => bail!("usage: fetch voice | pdfium [platform] | llama [platform] | glyphs"),
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn cache() -> Result<PathBuf> {
    let dir = root().join("target/cache");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn sha256_of(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    io::copy(&mut File::open(path)?, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn good(path: &Path, sha256: &str) -> bool {
    path.is_file() && sha256_of(path).is_ok_and(|actual| actual == sha256)
}

/// A whole download, through the system proxy if there is one.
fn get(url: &str) -> Result<Vec<u8>> {
    println!("fetch: {url}");
    let agent = ureq::AgentBuilder::new()
        .try_proxy_from_env(true)
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .build();
    let mut bytes = vec![];
    agent
        .get(url)
        .call()
        .with_context(|| url.to_string())?
        .into_reader()
        .read_to_end(&mut bytes)
        .with_context(|| url.to_string())?;
    Ok(bytes)
}

/// A download kept in target/cache and checked against its hash, so it is fetched once.
fn cached(url: &str, name: &str, sha256: &str) -> Result<PathBuf> {
    let path = cache()?.join(name);
    if good(&path, sha256) {
        return Ok(path);
    }
    let bytes = get(url)?;
    let actual = hex(&Sha256::digest(&bytes));
    ensure!(
        actual == sha256,
        "{url} does not match its checksum: {actual}"
    );
    fs::write(&path, bytes)?;
    Ok(path)
}

fn unpack(archive: &Path, into: &Path) -> Result<()> {
    let _ = fs::remove_dir_all(into);
    fs::create_dir_all(into)?;
    tar::Archive::new(flate2::read::GzDecoder::new(File::open(archive)?))
        .unpack(into)
        .with_context(|| format!("unpacking {}", archive.display()))
}

/// This machine as a download names it: the Mac and Windows are called differently by different projects.
fn unzip(archive: &Path, into: &Path) -> Result<()> {
    let _ = fs::remove_dir_all(into);
    fs::create_dir_all(into)?;
    zip::ZipArchive::new(File::open(archive)?)?
        .extract(into)
        .with_context(|| format!("unpacking {}", archive.display()))
}

fn host(mac: &str, windows: &str) -> Result<String> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => mac,
        "windows" => windows,
        other => bail!("no build for {other}"),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => bail!("no build for {other}"),
    };
    Ok(format!("{os}-{arch}"))
}

fn voice() -> Result<()> {
    let out = root().join("resources");
    let local = std::env::var_os("BOARDCAST_VOICE_DIR").map(PathBuf::from);
    let mut fetched = 0;
    for file in voice_files::files() {
        let dest = out.join(file.name);
        if good(&dest, file.sha256) {
            continue;
        }
        let dir = dest.parent().context("voice file folder")?;
        fs::create_dir_all(dir)?;
        let _ = fs::remove_file(&dest);
        fetched += 1;
        if let Some(copy) = local
            .as_ref()
            .map(|dir| dir.join(file.name))
            .filter(|copy| good(copy, file.sha256))
        {
            fs::copy(copy, &dest)?;
            continue;
        }
        let mut sources = vec![];
        if let Some(path) = file.modelscope {
            sources.push(boardcast_models::Source {
                name: "ModelScope".into(),
                url: format!("https://modelscope.cn/models/{path}"),
                direct: true,
            });
        }
        for (name, host) in [
            ("hf-mirror", "https://hf-mirror.com"),
            ("Hugging Face", "https://huggingface.co"),
        ] {
            sources.push(boardcast_models::Source {
                name: name.into(),
                url: format!("{host}/{}", file.huggingface),
                direct: false,
            });
        }
        if let Some(url) = file.github {
            sources.push(boardcast_models::Source {
                name: "GitHub".into(),
                url: url.into(),
                direct: false,
            });
        }
        let model = boardcast_models::Model {
            id: file.name,
            label: file.name,
            file: dest
                .file_name()
                .context("voice file name")?
                .to_string_lossy()
                .into_owned(),
            size: file.size,
            sha256: file.sha256.into(),
            vram_mib: 0,
            note: None,
            sources,
        };
        println!("fetch: {}", file.name);
        let mut last = u64::MAX;
        let cancel = std::sync::atomic::AtomicBool::new(false);
        boardcast_models::download(&model, dir, &cancel, &mut |p| {
            let tenth = p.done * 10 / p.total.max(1);
            if tenth != last {
                last = tenth;
                println!("  {}% from {}", tenth * 10, p.source.unwrap_or_default());
            }
        })?;
    }
    if fetched > 0 {
        println!("fetch: {fetched} voice files into resources/voice");
    }
    Ok(())
}

const PDFIUM_BUILD: &str = "chromium/8138";

fn pdfium(platform: Option<&str>) -> Result<()> {
    let platform = platform.map_or_else(|| host("mac", "win"), |p| Ok(p.to_string()))?;
    let sha256 = match platform.as_str() {
        "linux-x64" => "d6b1e28950a0392590d79bbdbb7eb19686872b688f8a36f170acb19e41473113",
        "linux-arm64" => "c70b1ce4d13257f741af19e0aeee575890875995eb81370a65be942e601a1862",
        "mac-arm64" => "21a1567b397b571f4f77f21ca91eac7b892774219c3c715a5127a20950ccb18b",
        "mac-x64" => "72662ac8eff7bf5381ef82ceebd0a0ad5bddb051b303981115114e6972fd201c",
        "win-x64" => "8d48973e3eb472dda0f5f2435e595ac043f9f2219dd71883392f549445afa8a5",
        other => bail!("no PDFium build for {other}"),
    };
    let out = root().join("target/pdfium");
    let marker = out.join(".version");
    let wanted = format!("{PDFIUM_BUILD} {platform}");
    if fs::read_to_string(&marker).is_ok_and(|v| v.trim() == wanted) {
        return Ok(());
    }
    let url = format!(
        "https://github.com/bblanchon/pdfium-binaries/releases/download/{}/pdfium-{platform}.tgz",
        PDFIUM_BUILD.replace('/', "%2F")
    );
    let archive = cached(
        &url,
        &format!("pdfium-{}-{platform}.tgz", PDFIUM_BUILD.replace('/', "-")),
        sha256,
    )?;
    let work = cache()?.join(format!("pdfium-{platform}"));
    unpack(&archive, &work)?;
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out)?;
    let library = match &platform[..3] {
        "mac" => "lib/libpdfium.dylib",
        "win" => "bin/pdfium.dll",
        _ => "lib/libpdfium.so",
    };
    fs::copy(
        work.join(library),
        out.join(Path::new(library).file_name().context("library name")?),
    )?;
    fs::copy(work.join("LICENSE"), out.join("LICENSE"))?;
    fs::write(marker, wanted)?;
    fs::remove_dir_all(work)?;
    Ok(())
}

/// Linux and Windows take the Vulkan build, which falls back to the CPU on a machine without Vulkan; the Mac the
/// Metal build.
/// Keep LLAMA_BUILD in .github/workflows/eval.yml in step.
const LLAMA_BUILD: &str = "b11954";

fn llama(platform: Option<&str>) -> Result<()> {
    let platform = platform.map_or_else(|| host("macos", "windows"), |p| Ok(p.to_string()))?;
    let asset = match platform.as_str() {
        "linux-x64" => format!("llama-{LLAMA_BUILD}-bin-ubuntu-vulkan-x64.tar.gz"),
        "windows-x64" => format!("llama-{LLAMA_BUILD}-bin-win-vulkan-x64.zip"),
        "macos-arm64" | "macos-x64" => format!("llama-{LLAMA_BUILD}-bin-{platform}.tar.gz"),
        other => bail!("no llama-server build for {other}"),
    };
    let out = root().join("resources/llama");
    let marker = out.join(".version");
    let wanted = format!("{LLAMA_BUILD} {platform}");
    if fs::read_to_string(&marker).is_ok_and(|v| v.trim() == wanted) {
        return Ok(());
    }
    let archive = cache()?.join(&asset);
    if !archive.is_file() {
        let bytes = get(&format!(
            "https://github.com/ggml-org/llama.cpp/releases/download/{LLAMA_BUILD}/{asset}"
        ))?;
        fs::write(&archive, bytes)?;
    }
    let work = cache()?.join(format!("llama-{platform}"));
    if asset.ends_with(".zip") {
        unzip(&archive, &work)?;
    } else {
        unpack(&archive, &work)?;
    }
    // The archive may hold one folder.
    let top: Vec<PathBuf> = fs::read_dir(&work)?
        .map(|e| e.map(|e| e.path()))
        .collect::<io::Result<_>>()?;
    let src = if let [only] = top.as_slice() {
        if only.is_dir() {
            only.clone()
        } else {
            work.clone()
        }
    } else {
        work.clone()
    };
    let names: Vec<String> = fs::read_dir(&src)?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<io::Result<_>>()?;

    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out)?;
    let (mut kept, mut bytes) = (0, 0);
    for name in names
        .iter()
        .filter(|name| keep_for_llama(name, &names, platform.starts_with("macos")))
    {
        // Copying follows links, so the package holds real files: packaging tools handle links poorly.
        bytes += fs::copy(src.join(name), out.join(name))?;
        kept += 1;
    }
    let server = if platform.starts_with("windows") {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    ensure!(out.join(server).is_file(), "{asset} has no {server}");
    fs::write(marker, format!("{wanted}\n"))?;
    fs::remove_dir_all(work)?;
    println!(
        "fetch: {platform} llama-server into resources/llama ({kept} files, {:.1} MB)",
        bytes as f64 / 1e6
    );
    Ok(())
}

/// llama-server, the libraries it loads, and the licence; not the other tools or duplicate library names.
fn keep_for_llama(name: &str, names: &[String], mac: bool) -> bool {
    if name == "llama-server" || name == "llama-server.exe" || name == "LICENSE" {
        return true;
    }
    // Windows: the libraries are DLLs with one name each.
    if let Some(stem) = name.strip_suffix(".dll") {
        return !(stem.contains("ggml-rpc")
            || (stem.contains("llama-")
                && stem.ends_with("-impl")
                && !stem.contains("llama-server-impl")));
    }
    let numbered = |s: &str| {
        !s.is_empty()
            && s.split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    };
    let library = name.ends_with(".so")
        || name.ends_with(".dylib")
        || name.split_once(".so.").is_some_and(|(_, v)| numbered(v));
    if !library {
        return false;
    }
    // Each other command-line tool's own implementation library.
    if name.contains("llama-") && name.contains("-impl.") && !name.contains("llama-server-impl") {
        return false;
    }
    // The RPC backend goes unused, but on the Mac llama-server and every library link it directly.
    if name.contains("ggml-rpc") && !mac {
        return false;
    }
    // Programs load libX.so.0 by its SONAME; libX.so.x.y.z is the same file and libX.so is for linking. Backends
    // without a SONAME version are loaded by the bare name, so that one stays.
    if name
        .split_once(".so.")
        .is_some_and(|(_, v)| v.split('.').count() == 3)
    {
        return false;
    }
    if name.ends_with(".so") && names.iter().any(|n| *n == format!("{name}.0")) {
        return false;
    }
    // The same on the Mac: libX.0.dylib is loaded, libX.0.27.0.dylib and libX.dylib are not.
    if let Some(stem) = name.strip_suffix(".dylib") {
        let parts: Vec<&str> = stem.rsplitn(4, '.').collect();
        if parts.len() == 4 && parts[..3].iter().all(|p| numbered(p)) {
            return false;
        }
        let versioned = stem.rsplit_once('.').is_some_and(|(_, v)| numbered(v));
        if !versioned && names.iter().any(|n| *n == format!("{stem}.0.dylib")) {
            return false;
        }
    }
    true
}

const HANZI_WRITER_DATA: &str = "2.0.1";
const HANZI_WRITER_SHA256: &str =
    "72baf3d82b114e60d6e40ea05f24d2262a05cd39d544e2f322ba2fceb7beff15";

/// One file per character holding only its stroke medians in order, from hanzi-writer-data (Arphic Public
/// License), then the Japanese kanji it lacks from KanjiVG, leaving its Chinese shapes as they are.
fn glyphs() -> Result<()> {
    let root = root();
    let archive = cached(
        &format!("https://registry.npmjs.org/hanzi-writer-data/-/hanzi-writer-data-{HANZI_WRITER_DATA}.tgz"),
        &format!("hanzi-writer-data-{HANZI_WRITER_DATA}.tgz"),
        HANZI_WRITER_SHA256,
    )?;
    let out = root.join("assets/hanzi");
    let marker = out.join(format!(".done-{HANZI_WRITER_DATA}"));
    let write = !marker.exists();
    fs::create_dir_all(&out)?;
    let mut chinese = HashSet::new();
    let mut written = 0;
    let mut tarball = tar::Archive::new(flate2::read::GzDecoder::new(File::open(&archive)?));
    for entry in tarball.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let Some(name) = path
            .strip_prefix("package")
            .ok()
            .and_then(|p| p.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if name == "ARPHICPL.TXT" && write {
            entry.unpack(out.join(&name))?;
        }
        let Some(character) = name
            .strip_suffix(".json")
            .filter(|_| name != "package.json")
        else {
            continue;
        };
        chinese.insert(character.to_string());
        if write {
            let mut text = String::new();
            entry.read_to_string(&mut text)?;
            let data: serde_json::Value =
                serde_json::from_str(&text).with_context(|| name.clone())?;
            if let Some(medians) = data.get("medians") {
                fs::write(
                    out.join(&name),
                    serde_json::to_string(&as_javascript(medians))?,
                )?;
                written += 1;
            }
        }
    }
    if write {
        fs::write(&marker, written.to_string())?;
        println!("fetch: {written} characters into assets/hanzi");
    }

    let kanji: serde_json::Map<String, serde_json::Value> = serde_json::from_reader(
        flate2::read::GzDecoder::new(File::open(root.join("assets/glyphs/kanji.json.gz"))?),
    )?;
    for (character, medians) in kanji.iter().filter(|(c, _)| !chinese.contains(*c)) {
        let file = out.join(format!("{character}.json"));
        let data = serde_json::to_string(&as_javascript(medians))?;
        if fs::read_to_string(&file).ok().as_deref() != Some(data.as_str()) {
            fs::write(file, data)?;
        }
    }
    fs::copy(
        root.join("assets/licenses/kanjivg-CC-BY-SA-3.0.txt"),
        out.join("KANJIVG-LICENSE.txt"),
    )?;
    Ok(())
}

/// Whole numbers written as JavaScript writes them, 333 rather than 333.0, so files match the old ones.
fn as_javascript(value: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::Array(items) => Value::Array(items.iter().map(as_javascript).collect()),
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < 1e15 => Value::from(f as i64),
            _ => value.clone(),
        },
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_numbers_drop_their_point() {
        let value: serde_json::Value = serde_json::from_str("[[333.0, 1.5], [-2.0, 7]]").unwrap();
        assert_eq!(
            serde_json::to_string(&as_javascript(&value)).unwrap(),
            "[[333,1.5],[-2,7]]"
        );
    }

    #[test]
    fn llama_keeps_the_server_and_the_names_it_loads() {
        let linux: Vec<String> = [
            "llama-server",
            "llama-cli",
            "LICENSE",
            "libllama.so",
            "libllama.so.0",
            "libllama.so.0.0.1",
            "libggml-vulkan.so",
            "libggml-rpc.so",
            "libllama-cli-impl.so",
            "libllama-server-impl.so",
            "README.md",
        ]
        .map(String::from)
        .into();
        let kept: Vec<&str> = linux
            .iter()
            .filter(|n| keep_for_llama(n, &linux, false))
            .map(String::as_str)
            .collect();
        assert_eq!(
            kept,
            [
                "llama-server",
                "LICENSE",
                "libllama.so.0",
                "libggml-vulkan.so",
                "libllama-server-impl.so"
            ]
        );

        let windows: Vec<String> = [
            "llama-server.exe",
            "llama-cli.exe",
            "llama.dll",
            "ggml-vulkan.dll",
            "ggml-rpc.dll",
            "llama-cli-impl.dll",
            "llama-server-impl.dll",
        ]
        .map(String::from)
        .into();
        let kept: Vec<&str> = windows
            .iter()
            .filter(|n| keep_for_llama(n, &windows, false))
            .map(String::as_str)
            .collect();
        assert_eq!(
            kept,
            [
                "llama-server.exe",
                "llama.dll",
                "ggml-vulkan.dll",
                "llama-server-impl.dll"
            ]
        );

        let mac: Vec<String> = [
            "llama-server",
            "libllama.dylib",
            "libllama.0.dylib",
            "libllama.0.27.0.dylib",
            "libggml-rpc.dylib",
            "libggml-metal.dylib",
        ]
        .map(String::from)
        .into();
        let kept: Vec<&str> = mac
            .iter()
            .filter(|n| keep_for_llama(n, &mac, true))
            .map(String::as_str)
            .collect();
        assert_eq!(
            kept,
            [
                "llama-server",
                "libllama.0.dylib",
                "libggml-rpc.dylib",
                "libggml-metal.dylib"
            ]
        );
    }
}
