//! Signing packages and writing the updater's manifest:
//!   sign <file>...                       writes <file>.sig with the update key
//!   release-manifest <packages> <tag>    prints manifest.json for every Linux package in a folder
//!
//! The key is the older app's, in the form `tauri signer` wrote it: minisign's encrypted secret key,
//! base64-encoded, in BOARDCAST_SIGNING_KEY with its password in BOARDCAST_SIGNING_KEY_PASSWORD.
//! A signature is written the same way, so installs already out there accept it.

use crate::fetch::hex;
use anyhow::{anyhow, bail, ensure, Context, Result};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn decode_text(encoded: &str) -> Result<String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded.trim()).context("not base64")?;
    String::from_utf8(bytes).context("not text")
}

pub fn sign(files: impl Iterator<Item = String>) -> Result<()> {
    let key = std::env::var("BOARDCAST_SIGNING_KEY").context("BOARDCAST_SIGNING_KEY is not set")?;
    let password = std::env::var("BOARDCAST_SIGNING_KEY_PASSWORD").unwrap_or_default();
    let key = secret_key(&key, &password)?;
    let mut any = false;
    for file in files {
        let path = Path::new(&file);
        fs::write(format!("{file}.sig"), signature(&key, path)?)?;
        println!("signed {file}");
        any = true;
    }
    ensure!(any, "usage: sign <file>...");
    Ok(())
}

fn secret_key(encoded: &str, password: &str) -> Result<minisign::SecretKey> {
    let text = decode_text(encoded).context("the signing key is damaged")?;
    let boxed = minisign::SecretKeyBox::from_string(&text).map_err(|e| anyhow!("the signing key is damaged: {e}"))?;
    boxed.into_secret_key(Some(password.to_string())).map_err(|e| anyhow!("the signing key cannot be opened: {e}"))
}

/// The base64 of minisign's signature file, with the comments `tauri signer sign` writes.
fn signature(key: &minisign::SecretKey, path: &Path) -> Result<String> {
    let name = path.file_name().context("file name")?.to_string_lossy();
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let trusted = format!("timestamp:{timestamp}\tfile:{name}");
    let file = File::open(path).with_context(|| format!("reading {}", path.display()))?;
    let boxed = minisign::sign(None, key, file, Some(&trusted), Some("signature from tauri secret key")).map_err(|e| anyhow!("signing {}: {e}", path.display()))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(boxed.to_string()))
}

/// Every Linux package variant in `dir`, keyed by platform and variant (x86_64-linux for the full package,
/// x86_64-linux-ja for another), with its size, hash and signature. VERSION and NOTES come from the
/// environment, and GITHUB_REPOSITORY names where the packages are downloaded from.
pub fn manifest(mut args: impl Iterator<Item = String>) -> Result<()> {
    let dir = args.next().context("usage: release-manifest <packages> <tag>")?;
    let tag = args.next().context("usage: release-manifest <packages> <tag>")?;
    let version = std::env::var("VERSION").context("VERSION is not set")?;
    let notes = std::env::var("NOTES").unwrap_or_default();
    let repo = std::env::var("GITHUB_REPOSITORY").unwrap_or_else(|_| "HsiangNianian/boardcast".into());
    println!("{}", serde_json::to_string_pretty(&build_manifest(Path::new(&dir), &tag, &version, &notes, &repo)?)?);
    Ok(())
}

fn build_manifest(dir: &Path, tag: &str, version: &str, notes: &str, repo: &str) -> Result<serde_json::Value> {
    let mut platforms = BTreeMap::new();
    let mut names: Vec<String> = fs::read_dir(dir)?.map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned())).collect::<std::io::Result<_>>()?;
    names.sort();
    for name in names {
        let Some((packaged, platform)) = package_name(&name) else { continue };
        if packaged != version {
            bail!("{name} is not version {version}");
        }
        let bytes = fs::read(dir.join(&name))?;
        let signature = fs::read_to_string(dir.join(format!("{name}.sig"))).with_context(|| format!("{name} is not signed"))?;
        platforms.insert(
            platform,
            serde_json::json!({
                "url": format!("https://github.com/{repo}/releases/download/{tag}/{name}"),
                "size": bytes.len(),
                "sha256": hex(&Sha256::digest(&bytes)),
                "signature": signature.trim(),
            }),
        );
    }
    ensure!(platforms.contains_key("x86_64-linux"), "the full Linux package is missing");
    Ok(serde_json::json!({ "version": version, "notes": notes, "platforms": platforms }))
}

/// `boardcast-<version>-<arch>-linux[-<variant>].tar.gz` as its version and platform key.
fn package_name(name: &str) -> Option<(&str, String)> {
    let rest = name.strip_prefix("boardcast-")?.strip_suffix(".tar.gz")?;
    let (head, variant) = match rest.rsplit_once("-linux") {
        Some((head, "")) => (head, None),
        Some((head, tail)) => (head, Some(tail.strip_prefix('-').filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_lowercase()))?)),
        None => return None,
    };
    let (version, arch) = head.rsplit_once('-')?;
    if !matches!(arch, "x86_64" | "aarch64") {
        return None;
    }
    Some((version, format!("{arch}-linux{}", variant.map(|v| format!("-{v}")).unwrap_or_default())))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/signing");

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("boardcast-release-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The key was made with `tauri signer generate -p fixture`, so this pins Rust signing to the format
    /// that signed every release so far, checked by the app's own update code.
    #[test]
    fn a_tauri_key_signs_what_the_updater_accepts() {
        let key = secret_key(&fs::read_to_string(format!("{FIXTURES}/tauri-test.key")).unwrap(), "fixture").unwrap();
        let public = fs::read_to_string(format!("{FIXTURES}/tauri-test.key.pub")).unwrap();
        let dir = temp("sign");
        let package = dir.join("payload.bin");
        fs::write(&package, b"boardcast signing fixture\n").unwrap();
        let signed = signature(&key, &package).unwrap();
        assert!(decode_text(&signed).unwrap().contains("\tfile:payload.bin\n"));
        boardcast_update::verify(&package, &signed, public.trim()).unwrap();

        fs::write(&package, b"boardcast signing fixturE\n").unwrap();
        assert!(boardcast_update::verify(&package, &signed, public.trim()).is_err());
        assert!(secret_key(&fs::read_to_string(format!("{FIXTURES}/tauri-test.key")).unwrap(), "wrong").is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn package_names_become_platform_keys() {
        assert_eq!(package_name("boardcast-0.30.0-x86_64-linux.tar.gz"), Some(("0.30.0", "x86_64-linux".into())));
        assert_eq!(package_name("boardcast-0.30.0-alpha.1-aarch64-linux-ja.tar.gz"), Some(("0.30.0-alpha.1", "aarch64-linux-ja".into())));
        assert_eq!(package_name("boardcast-0.30.0-x86_64-linux.tar.gz.sig"), None);
        assert_eq!(package_name("boardcast-0.30.0-aarch64-macos.zip"), None);
    }

    #[test]
    fn the_manifest_lists_signed_packages_and_needs_the_full_one() {
        let dir = temp("manifest");
        for (name, body) in [("boardcast-1.2.0-x86_64-linux.tar.gz", "full"), ("boardcast-1.2.0-x86_64-linux-ja.tar.gz", "ja")] {
            fs::write(dir.join(name), body).unwrap();
            fs::write(dir.join(format!("{name}.sig")), "c2ln\n").unwrap();
        }
        fs::write(dir.join("boardcast-1.2.0-aarch64-macos.zip"), "mac").unwrap();
        let manifest = build_manifest(&dir, "v1.2.0", "1.2.0", "notes", "o/r").unwrap();
        assert_eq!(manifest["platforms"]["x86_64-linux-ja"]["url"], "https://github.com/o/r/releases/download/v1.2.0/boardcast-1.2.0-x86_64-linux-ja.tar.gz");
        assert_eq!(manifest["platforms"]["x86_64-linux"]["size"], 4);
        assert_eq!(manifest["platforms"]["x86_64-linux"]["signature"], "c2ln");
        assert_eq!(manifest["platforms"].as_object().unwrap().len(), 2);
        assert!(build_manifest(&dir, "v1.3.0", "1.3.0", "", "o/r").is_err(), "a package of another version");
        fs::remove_file(dir.join("boardcast-1.2.0-x86_64-linux.tar.gz")).unwrap();
        assert!(build_manifest(&dir, "v1.2.0", "1.2.0", "", "o/r").is_err(), "no full package");
        fs::remove_dir_all(dir).unwrap();
    }
}
