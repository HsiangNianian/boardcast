//! Updating an installed app: find a newer release, download it, prove it is genuine, and swap it in.
//!
//! The manifest is read from the latest release. `gpui-latest`, where the alphas were published, carries a
//! copy so those installs find it too; the older app's update channel never sees either.
//! A package comes in variants that differ only in their speech models, and an install keeps to its own.
//! Packages are signed with the older app's update key. The signature covers the whole package, so
//! nothing in the manifest needs to be trusted, including the version: that is read from inside the
//! verified package, which is what stops an old signed package being served as an update.
//! A new version must run from where it was unpacked before it replaces anything.

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use alcyo_models::{download, Model, Outcome, Progress, Source};
use flate2::read::GzDecoder;
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde_json::Value;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// Where the latest release lists its packages.
const MANIFEST_URL: &str = "https://github.com/HsiangNianian/alcyo/releases/latest/download/manifest.json";
/// The name of the program and of its resources folder.
const NAME: &str = "alcyo";
/// The older app's update key. A test build can name another at compile time, never at run time.
const PUBLIC_KEY: &str = match option_env!("ALCYO_UPDATE_PUBKEY") {
    Some(key) => key,
    None => "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDg3RUMzMzZGNzQ2NDA2OEEKUldTS0JtUjBielBzaHlNNkVTYmZXMkx5SHdXNndBWDhCSFJiSmJwcG5KdVhIbXcwNmFBaktzNVEK",
};

/// This build's version.
pub const VERSION: &str = match option_env!("ALCYO_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub signature: String,
}

fn platform() -> String {
    format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)
}

/// Which package the resources in `lib` came from: empty for the one with every model ("all", once
/// "full"), or a name like "ja". A package from before variants has no record of one, and had every model.
pub fn variant_of(lib: &Path) -> String {
    fs::read_to_string(lib.join("variant")).map(|v| v.trim().to_string()).ok().filter(|v| v != "full" && v != "all").unwrap_or_default()
}

/// The manifest's key for a platform and variant: `x86_64-linux` for the full package, `x86_64-linux-ja` for another.
fn platform_key(platform: &str, variant: &str) -> String {
    if variant.is_empty() {
        platform.to_string()
    } else {
        format!("{platform}-{variant}")
    }
}

/// The release for this platform, if it is newer than `current`.
fn parse_manifest(text: &str, current: &str, platform: &str) -> Result<Option<Release>> {
    let manifest: Value = serde_json::from_str(text).context("the update information is damaged")?;
    let version = manifest["version"].as_str().context("the update information has no version")?;
    if Version::parse(version).context("the update has a malformed version")? <= Version::parse(current)? {
        return Ok(None);
    }
    let Some(entry) = manifest["platforms"].get(platform) else { return Ok(None) };
    let text = |key: &str| entry[key].as_str().map(str::to_string).with_context(|| format!("the update information has no {key}"));
    Ok(Some(Release {
        version: version.to_string(),
        notes: manifest["notes"].as_str().unwrap_or_default().to_string(),
        url: text("url")?,
        size: entry["size"].as_u64().context("the update information has no size")?,
        sha256: text("sha256")?,
        signature: text("signature")?,
    }))
}

/// Ask whether there is a newer release. `ALCYO_UPDATE_URL` points elsewhere for testing, which
/// is safe because the signature check does not depend on where a package came from.
pub fn check() -> Result<Option<Release>> {
    let url = std::env::var("ALCYO_UPDATE_URL").unwrap_or_else(|_| MANIFEST_URL.to_string());
    let agent = ureq::AgentBuilder::new().try_proxy_from_env(true).timeout(Duration::from_secs(30)).build();
    let text = match agent.get(&url).call() {
        Ok(response) => response.into_string()?,
        // Nothing has been published yet, which is not a failure.
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(e) => return Err(anyhow!("could not reach the update server: {e}")),
    };
    let variant = installed_prefix().and_then(|prefix| lib_dir(&prefix)).map(|lib| variant_of(&lib)).unwrap_or_default();
    parse_manifest(&text, VERSION, &platform_key(&platform(), &variant))
}

/// Download a release into `dir`, resuming an earlier attempt and checking its size and hash.
pub fn fetch(release: &Release, dir: &Path, cancel: &AtomicBool, report: &mut dyn FnMut(Progress)) -> Result<Option<PathBuf>> {
    let file = format!("alcyo-{}.tar.gz", release.version);
    let model = Model {
        id: "update",
        label: "Alcyo",
        file,
        size: release.size,
        sha256: release.sha256.clone(),
        vram_mib: 0,
        note: None,
        sources: vec![Source { name: "GitHub".into(), url: release.url.clone(), direct: false }],
    };
    match download(&model, dir, cancel, report)? {
        Outcome::Finished(path) => Ok(Some(path)),
        Outcome::Cancelled => Ok(None),
    }
}

/// Keys and signatures travel base64-encoded around minisign's own text format, as Tauri writes them.
fn decode_text(encoded: &str) -> Result<String> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded.trim()).context("not base64")?;
    String::from_utf8(bytes).context("not text")
}

/// Check a file against a signature and public key, both in the base64 form Tauri writes.
pub fn verify(path: &Path, signature: &str, key: &str) -> Result<()> {
    let key = PublicKey::decode(&decode_text(key).context("the update key is damaged")?).map_err(|e| anyhow!("the update key is damaged: {e}"))?;
    let signature = Signature::decode(&decode_text(signature).context("the signature is damaged")?).map_err(|e| anyhow!("the signature is damaged: {e}"))?;
    let mut stream = key.verify_stream(&signature).map_err(|e| anyhow!("the signature cannot be checked: {e}"))?;
    let mut file = File::open(path)?;
    let mut buffer = vec![0; 1 << 20];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        stream.update(&buffer[..n]);
    }
    stream.finalize().map_err(|_| anyhow!("this update is not signed by Alcyo, so it was not installed"))
}

/// Unpack below `dest`, dropping the package's top folder. Nothing may land outside `dest`.
fn unpack(archive: &Path, dest: &Path) -> Result<()> {
    let mut tar = tar::Archive::new(GzDecoder::new(File::open(archive)?));
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let mut parts = path.components();
        parts.next();
        let inner = parts.as_path().to_path_buf();
        if inner.components().any(|c| !matches!(c, Component::Normal(_))) {
            bail!("the package contains an unsafe path: {}", path.display());
        }
        if inner.as_os_str().is_empty() {
            continue;
        }
        if let Some(target) = entry.link_name()? {
            let target = target.into_owned();
            if target.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
                bail!("the package contains an unsafe link: {}", path.display());
            }
        }
        let out = dest.join(inner);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        entry.unpack(&out).with_context(|| format!("unpacking {}", path.display()))?;
    }
    Ok(())
}

/// The folder an install lives in, if this program runs from one: `<prefix>/bin/alcyo`.
pub fn installed_prefix() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let prefix = exe.parent()?.parent()?;
    lib_dir(prefix).filter(|lib| lib.join("glyphs.pack").is_file()).map(|_| prefix.to_path_buf())
}

/// The resources folder of an install or an unpacked package.
pub fn lib_dir(root: &Path) -> Option<PathBuf> {
    Some(root.join("lib").join(NAME)).filter(|dir| dir.is_dir())
}

/// The program of an install or an unpacked package.
pub fn program(root: &Path) -> PathBuf {
    root.join("bin").join(NAME)
}

fn check_unpacked(staged: &Path, current: &str) -> Result<()> {
    let version = fs::read_to_string(staged.join("VERSION")).context("the package has no version")?;
    let version = version.trim();
    // From inside the signed package, so an old signed package cannot pass as a new one.
    if Version::parse(version).context("the package has a malformed version")? <= Version::parse(current)? {
        bail!("this package is version {version}, which is not newer than {current}");
    }
    let output = Command::new(program(staged)).arg("--self-check").output().context("the new version could not run")?;
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    if !output.status.success() || report["ok"] != true || report["version"] != version {
        bail!("the new version failed its self-check, so it was not installed");
    }
    Ok(())
}

/// Replace the app and its resources, undoing the swap if any part of it fails.
fn swap(staged: &Path, prefix: &Path) -> Result<()> {
    let staged_lib = lib_dir(staged).context("the package has no resources folder")?;
    let staged_program = program(staged);
    let current = lib_dir(prefix).context("this install has no resources folder")?;
    let (lib, old_lib) = (prefix.join("lib").join(NAME), prefix.join("lib/.update-old"));
    let _ = fs::remove_dir_all(&old_lib);
    fs::rename(&current, &old_lib).context("moving the current version aside")?;
    let replace = || -> Result<()> {
        fs::rename(&staged_lib, &lib)?;
        // A rename over the running program is safe: it keeps running from the old file.
        fs::rename(&staged_program, prefix.join("bin").join(NAME))?;
        Ok(())
    };
    if let Err(e) = replace() {
        let _ = fs::remove_dir_all(&lib);
        fs::rename(&old_lib, &current).context("restoring the previous version after a failed update")?;
        return Err(e.context("the update failed and the previous version was restored"));
    }
    let _ = fs::remove_dir_all(&old_lib);
    // Licences follow the code; failing to refresh them must not undo a working update.
    let licenses = prefix.join("share/licenses").join(NAME);
    if licenses.is_dir() {
        if let Ok(files) = fs::read_dir(staged.join("share/licenses").join(NAME)) {
            for file in files.flatten() {
                let _ = fs::copy(file.path(), licenses.join(file.file_name()));
            }
        }
    }
    Ok(())
}

/// Verify a downloaded package and install it over `prefix`. On failure the install is as it was.
pub fn install(archive: &Path, release: &Release, prefix: &Path) -> Result<()> {
    install_with(archive, &release.signature, PUBLIC_KEY, prefix, VERSION)
}

fn install_with(archive: &Path, signature: &str, key: &str, prefix: &Path, current: &str) -> Result<()> {
    verify(archive, signature, key)?;
    // Inside the prefix, so the swap is a rename on one disk, and this also proves it is writable.
    let staged = prefix.join(".alcyo-update");
    let _ = fs::remove_dir_all(&staged);
    fs::create_dir_all(&staged).with_context(|| format!("{} cannot be written to. Install the update the way this copy was installed.", prefix.display()))?;
    let result = unpack(archive, &staged).and_then(|()| check_unpacked(&staged, current)).and_then(|()| swap(&staged, prefix));
    let _ = fs::remove_dir_all(&staged);
    result
}

// Installing swaps Unix files and permissions, and only Linux installs itself.
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    /// A real keypair and signature from `tauri signer`, the tool that signs releases. The private
    /// key was thrown away. This pins the format, so a tooling change cannot silently break updates.
    const TEST_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEY5NTc0MUQ5NTlDOTY0QkEKUldTNlpNbFoyVUZYK1hnWGFoT01zZnFjYjNDQXNFZEdUQ3pXS2U2S0tubjAyNThxQStGaUpsN1IK";
    const TEST_SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTNlpNbFoyVUZYK2ZtU2I4WVZGVWRzQ1lmQ1ZiL3QydGdQTVdXWDdyeDlvZVpOT004TVQ0UGtadWVZbkxkQnRDbG4vNTM1c0tNSEIwVDRRZHEyNE5JRmx3dmFaN2lScEFFPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk1MjQyMDI5CWZpbGU6cGF5bG9hZC5iaW4KNnZaT3prK3hhUW5WSFh2MUhaS2txY3c2b2IzZjdBRWVPNE5sZGJnbDlFS1phY05ueVp4RGhTUVdyWnhVVmVnK3liclduVmovTzBqMlZ3MkNnRVB6RFE9PQo=";
    const TEST_PAYLOAD: &[u8] = b"alcyo update signature fixture\n";

    struct Dir(PathBuf);

    impl Dir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("alcyo-update-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_real_tauri_signature_verifies_and_tampering_fails() {
        let dir = Dir::new();
        let good = dir.0.join("good");
        fs::write(&good, TEST_PAYLOAD).unwrap();
        verify(&good, TEST_SIGNATURE, TEST_KEY).unwrap();
        let bad = dir.0.join("bad");
        fs::write(&bad, b"alcyo update signature fixturE\n").unwrap();
        assert!(verify(&bad, TEST_SIGNATURE, TEST_KEY).unwrap_err().to_string().contains("not signed"));
        assert!(verify(&good, "not base64!", TEST_KEY).is_err());
    }

    #[test]
    fn a_signature_from_another_key_fails_and_the_shipped_key_is_readable() {
        let dir = Dir::new();
        let good = dir.0.join("good");
        fs::write(&good, TEST_PAYLOAD).unwrap();
        assert!(verify(&good, TEST_SIGNATURE, PUBLIC_KEY).is_err());
        PublicKey::decode(&decode_text(PUBLIC_KEY).unwrap()).unwrap();
    }

    fn manifest(version: &str) -> String {
        format!(r#"{{"version":"{version}","notes":"新功能","platforms":{{"x86_64-linux":{{"url":"https://x/a.tar.gz","size":5,"sha256":"ab","signature":"s"}}}}}}"#)
    }

    #[test]
    fn only_a_newer_release_for_this_platform_is_offered() {
        let release = parse_manifest(&manifest("0.22.0"), "0.21.0", "x86_64-linux").unwrap().unwrap();
        assert_eq!((release.version.as_str(), release.size, release.notes.as_str()), ("0.22.0", 5, "新功能"));
        assert_eq!(parse_manifest(&manifest("0.21.0"), "0.21.0", "x86_64-linux").unwrap(), None);
        assert_eq!(parse_manifest(&manifest("0.20.0"), "0.21.0", "x86_64-linux").unwrap(), None);
        assert_eq!(parse_manifest(&manifest("0.22.0"), "0.21.0", "aarch64-linux").unwrap(), None);
        assert!(parse_manifest(&manifest("0.22.0-alpha.2"), "0.22.0-alpha.1", "x86_64-linux").unwrap().is_some());
        assert!(parse_manifest(&manifest("0.22.0-alpha.2"), "0.22.0", "x86_64-linux").unwrap().is_none());
        assert!(parse_manifest("{ damaged", "0.21.0", "x86_64-linux").is_err());
        assert!(parse_manifest(&manifest("banana"), "0.21.0", "x86_64-linux").is_err());
        // Another variant's install looks for its own package, which this manifest does not have.
        assert_eq!(parse_manifest(&manifest("0.22.0"), "0.21.0", &platform_key("x86_64-linux", "ja")).unwrap(), None);
        assert_eq!(platform_key("x86_64-linux", ""), "x86_64-linux");
    }

    #[test]
    fn an_install_without_a_variant_is_the_full_package() {
        let dir = std::env::temp_dir().join(format!("alcyo-variant-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(variant_of(&dir), "");
        fs::write(dir.join("variant"), "full\n").unwrap();
        assert_eq!(variant_of(&dir), "");
        fs::write(dir.join("variant"), "ja\n").unwrap();
        assert_eq!(variant_of(&dir), "ja");
        let _ = fs::remove_dir_all(dir);
    }

    /// A package like the real one, with a program that reports `reported` and whether it is healthy.
    fn package(dir: &Path, version: &str, reported: &str, healthy: bool) -> PathBuf {
        let name = NAME;
        let root = dir.join("source");
        let _ = fs::remove_dir_all(&root);
        let lib = root.join("lib").join(name);
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(lib.join("llama")).unwrap();
        fs::create_dir_all(root.join("share/licenses").join(name)).unwrap();
        fs::write(root.join("VERSION"), format!("{version}\n")).unwrap();
        fs::write(lib.join("glyphs.pack"), version).unwrap();
        fs::write(lib.join("llama/llama-server"), version).unwrap();
        fs::write(root.join("share/licenses").join(name).join("LICENSE"), version).unwrap();
        let code = if healthy { 0 } else { 1 };
        let script = format!("#!/bin/sh\necho '{{\"ok\":{healthy},\"version\":\"{reported}\"}}'\nexit {code}\n");
        fs::write(root.join("bin").join(name), &script).unwrap();
        fs::set_permissions(root.join("bin").join(name), fs::Permissions::from_mode(0o755)).unwrap();
        let path = dir.join("package.tar.gz");
        let mut tar = tar::Builder::new(GzEncoder::new(File::create(&path).unwrap(), Compression::fast()));
        tar.append_dir_all("alcyo-x", &root).unwrap();
        tar.into_inner().unwrap().finish().unwrap();
        path
    }

    fn installed(dir: &Path) -> PathBuf {
        let prefix = dir.join("prefix");
        fs::create_dir_all(prefix.join("bin")).unwrap();
        fs::create_dir_all(prefix.join("lib/alcyo/leftover")).unwrap();
        fs::create_dir_all(prefix.join("share/licenses/alcyo")).unwrap();
        fs::write(prefix.join("bin/alcyo"), "old program").unwrap();
        fs::write(prefix.join("lib/alcyo/glyphs.pack"), "old").unwrap();
        fs::write(prefix.join("lib/alcyo/leftover/file"), "old").unwrap();
        prefix
    }

    fn untouched(prefix: &Path) -> bool {
        fs::read_to_string(prefix.join("bin/alcyo")).unwrap() == "old program"
            && fs::read_to_string(prefix.join("lib/alcyo/glyphs.pack")).unwrap() == "old"
            && !prefix.join(".alcyo-update").exists()
    }

    fn steps(archive: &Path, prefix: &Path, current: &str) -> Result<()> {
        let staged = prefix.join(".alcyo-update");
        fs::create_dir_all(&staged)?;
        let result = unpack(archive, &staged).and_then(|()| check_unpacked(&staged, current)).and_then(|()| swap(&staged, prefix));
        let _ = fs::remove_dir_all(&staged);
        result
    }

    #[test]
    fn a_good_package_replaces_everything_and_leaves_nothing_behind() {
        let dir = Dir::new();
        let prefix = installed(&dir.0);
        let archive = package(&dir.0, "0.3.0", "0.3.0", true);
        steps(&archive, &prefix, "0.2.0").unwrap();
        assert!(fs::read_to_string(prefix.join("bin/alcyo")).unwrap().contains("0.3.0"));
        assert_eq!(fs::read_to_string(prefix.join("lib/alcyo/glyphs.pack")).unwrap(), "0.3.0");
        assert!(prefix.join("lib/alcyo/llama/llama-server").is_file());
        assert!(!prefix.join("lib/alcyo/leftover").exists(), "old resources must not linger");
        assert_eq!(fs::read_to_string(prefix.join("share/licenses/alcyo/LICENSE")).unwrap(), "0.3.0");
        assert!(!prefix.join(".alcyo-update").exists() && !prefix.join("lib/.update-old").exists());
        assert!(fs::metadata(prefix.join("bin/alcyo")).unwrap().permissions().mode() & 0o111 != 0, "it must stay runnable");
    }

    #[test]
    fn an_old_package_cannot_pass_as_an_update() {
        let dir = Dir::new();
        let prefix = installed(&dir.0);
        let archive = package(&dir.0, "0.1.0", "0.1.0", true);
        assert!(steps(&archive, &prefix, "0.2.0").unwrap_err().to_string().contains("not newer"));
        assert!(untouched(&prefix));
    }

    #[test]
    fn a_version_that_does_not_run_or_disagrees_is_not_installed() {
        let dir = Dir::new();
        let prefix = installed(&dir.0);
        let broken = package(&dir.0, "0.3.0", "0.3.0", false);
        assert!(steps(&broken, &prefix, "0.2.0").unwrap_err().to_string().contains("self-check"));
        assert!(untouched(&prefix));
        let mismatched = package(&dir.0, "0.3.0", "0.2.5", true);
        assert!(steps(&mismatched, &prefix, "0.2.0").is_err());
        assert!(untouched(&prefix));
    }

    #[test]
    fn a_swap_that_fails_midway_restores_the_previous_version() {
        let dir = Dir::new();
        let prefix = installed(&dir.0);
        let staged = dir.0.join("staged");
        fs::create_dir_all(staged.join("lib/alcyo")).unwrap();
        fs::write(staged.join("lib/alcyo/glyphs.pack"), "new").unwrap();
        // No staged program, so the second rename fails after the first has happened.
        let error = swap(&staged, &prefix).unwrap_err();
        assert!(format!("{error:#}").contains("restored"), "{error:#}");
        assert_eq!(fs::read_to_string(prefix.join("lib/alcyo/glyphs.pack")).unwrap(), "old");
        assert_eq!(fs::read_to_string(prefix.join("bin/alcyo")).unwrap(), "old program");
    }

    /// tar's builder refuses dangerous names, so these are written by hand.
    fn raw_tar(dir: &Path, path: &str, link: Option<&str>) -> PathBuf {
        let mut header = [0_u8; 512];
        header[..path.len()].copy_from_slice(path.as_bytes());
        header[100..108].copy_from_slice(b"0000644\0");
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        header[124..136].copy_from_slice(b"00000000000\0");
        header[136..148].copy_from_slice(b"00000000000\0");
        header[156] = if link.is_some() { b'2' } else { b'0' };
        if let Some(link) = link {
            header[157..157 + link.len()].copy_from_slice(link.as_bytes());
        }
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[148..156].copy_from_slice(b"        ");
        let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
        header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        let out = dir.join("evil.tar.gz");
        let mut gz = GzEncoder::new(File::create(&out).unwrap(), Compression::fast());
        std::io::Write::write_all(&mut gz, &header).unwrap();
        std::io::Write::write_all(&mut gz, &[0; 1024]).unwrap();
        gz.finish().unwrap();
        out
    }

    #[test]
    fn a_package_cannot_write_outside_its_folder() {
        let dir = Dir::new();
        let dest = dir.0.join("dest");
        fs::create_dir_all(&dest).unwrap();
        let escape = raw_tar(&dir.0, "alcyo-x/../../escaped", None);
        assert!(unpack(&escape, &dest).unwrap_err().to_string().contains("unsafe"));
        assert!(!dir.0.join("escaped").exists());
        let link = raw_tar(&dir.0, "alcyo-x/link", Some("../../../etc/passwd"));
        assert!(unpack(&link, &dest).unwrap_err().to_string().contains("unsafe"));
        assert!(!dest.join("link").exists());
    }

    #[test]
    fn a_bad_signature_stops_everything_before_any_file_is_touched() {
        let dir = Dir::new();
        let prefix = installed(&dir.0);
        let archive = package(&dir.0, "0.3.0", "0.3.0", true);
        assert!(install_with(&archive, TEST_SIGNATURE, TEST_KEY, &prefix, "0.2.0").is_err());
        assert!(untouched(&prefix));
    }

    #[test]
    fn a_read_only_install_says_so_and_stays_intact() {
        let dir = Dir::new();
        let prefix = installed(&dir.0);
        let archive = dir.0.join("signed");
        fs::write(&archive, TEST_PAYLOAD).unwrap();
        fs::set_permissions(&prefix, fs::Permissions::from_mode(0o555)).unwrap();
        let error = install_with(&archive, TEST_SIGNATURE, TEST_KEY, &prefix, "0.2.0").unwrap_err();
        fs::set_permissions(&prefix, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(error.to_string().contains("cannot be written"), "{error:#}");
        assert_eq!(fs::read_to_string(prefix.join("bin/alcyo")).unwrap(), "old program");
    }
}
